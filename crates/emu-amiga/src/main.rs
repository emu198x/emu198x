//! Amiga 500 emulator binary.
//!
//! Runs the Amiga with a winit window and pixels framebuffer, or in
//! headless mode for screenshots, or as an MCP server.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu_amiga::{capture, mcp::McpServer, Amiga, AmigaConfig};
use emu_amiga::denise;
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Framebuffer dimensions.
const FB_WIDTH: u32 = denise::FB_WIDTH;
const FB_HEIGHT: u32 = denise::FB_HEIGHT;

/// Window scale factor.
const SCALE: u32 = 2;

/// Frame duration for ~50 Hz PAL.
const FRAME_DURATION: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct CliArgs {
    kickstart_path: Option<PathBuf>,
    headless: bool,
    mcp: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    record_dir: Option<PathBuf>,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        kickstart_path: None,
        headless: false,
        mcp: false,
        frames: 200,
        screenshot_path: None,
        record_dir: None,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--kickstart" | "--rom" => {
                i += 1;
                cli.kickstart_path = args.get(i).map(PathBuf::from);
            }
            "--headless" => {
                cli.headless = true;
            }
            "--mcp" => {
                cli.mcp = true;
            }
            "--frames" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.frames = s.parse().unwrap_or(200);
                }
            }
            "--screenshot" => {
                i += 1;
                cli.screenshot_path = args.get(i).map(PathBuf::from);
            }
            "--record" => {
                i += 1;
                cli.record_dir = args.get(i).map(PathBuf::from);
            }
            "--help" | "-h" => {
                eprintln!("Usage: emu-amiga [OPTIONS]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --kickstart <file>   Kickstart ROM file (256K)");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --mcp                Run as MCP server (JSON-RPC over stdio)");
                eprintln!("  --frames <n>         Number of frames in headless mode [default: 200]");
                eprintln!("  --screenshot <file>  Save a PNG screenshot (headless)");
                eprintln!("  --record <dir>       Record frames to directory (headless)");
                process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                process::exit(1);
            }
        }
        i += 1;
    }

    cli
}

// ---------------------------------------------------------------------------
// Headless mode
// ---------------------------------------------------------------------------

fn run_headless(cli: &CliArgs) {
    let mut amiga = make_amiga(cli);

    if let Some(ref dir) = cli.record_dir {
        if let Err(e) = capture::record(&mut amiga, dir, cli.frames) {
            eprintln!("Record error: {e}");
            process::exit(1);
        }
        return;
    }

    let trace_state = std::env::var("EMU_AMIGA_TRACE_STATE").is_ok();
    let trace_mem = std::env::var("EMU_AMIGA_TRACE_MEM").is_ok();
    let trace_mem_addrs = std::env::var("EMU_AMIGA_TRACE_MEM_ADDRS")
        .ok()
        .and_then(|spec| {
            let mut addrs = Vec::new();
            for part in spec.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                let part = part.trim_start_matches("0x").trim_start_matches("0X");
                if let Ok(addr) = u32::from_str_radix(part, 16) {
                    addrs.push(addr);
                }
            }
            if addrs.is_empty() { None } else { Some(addrs) }
        });
    for i in 0..cli.frames {
        amiga.run_frame();
        if i % 50 == 0 {
            if trace_state {
                let cpu = amiga.cpu();
                eprintln!(
                    "Frame {i}: PC=${:08X} SR=${:04X} D0=${:08X} D1=${:08X} A0=${:08X} A1=${:08X}",
                    cpu.regs.pc,
                    cpu.regs.sr,
                    cpu.regs.d[0],
                    cpu.regs.d[1],
                    cpu.regs.a(0),
                    cpu.regs.a(1),
                );
                if trace_mem {
                    let b0 = amiga.bus().memory.read(0x00000400);
                    let b1 = amiga.bus().memory.read(0x00000401);
                    let b2 = amiga.bus().memory.read(0x00000402);
                    let b3 = amiga.bus().memory.read(0x00000403);
                    let val = u32::from(b0) << 24
                        | u32::from(b1) << 16
                        | u32::from(b2) << 8
                        | u32::from(b3);
                    eprintln!("  MEM[00000400] = ${:08X}", val);
                }
                if let Some(ref addrs) = trace_mem_addrs {
                    for &addr in addrs {
                        let b0 = amiga.bus().memory.read(addr);
                        let b1 = amiga.bus().memory.read(addr.wrapping_add(1));
                        let b2 = amiga.bus().memory.read(addr.wrapping_add(2));
                        let b3 = amiga.bus().memory.read(addr.wrapping_add(3));
                        let val = u32::from(b0) << 24
                            | u32::from(b1) << 16
                            | u32::from(b2) << 8
                            | u32::from(b3);
                        eprintln!("  MEM[{addr:08X}] = ${val:08X}");
                    }
                }
            } else {
                eprintln!(
                    "Frame {i}: PC=${:08X}",
                    amiga.cpu().regs.pc,
                );
            }
        }
    }

    if let Some(ref path) = cli.screenshot_path {
        if let Err(e) = capture::save_screenshot(&amiga, path) {
            eprintln!("Screenshot error: {e}");
            process::exit(1);
        }
        eprintln!("Screenshot saved to {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// Windowed mode (winit + pixels)
// ---------------------------------------------------------------------------

struct App {
    amiga: Amiga,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    last_frame_time: Instant,
}

impl App {
    fn new(amiga: Amiga) -> Self {
        Self {
            amiga,
            window: None,
            pixels: None,
            last_frame_time: Instant::now(),
        }
    }

    fn update_pixels(&mut self) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        let fb = self.amiga.framebuffer();
        let frame = pixels.frame_mut();

        for (i, &argb) in fb.iter().enumerate() {
            let offset = i * 4;
            frame[offset] = ((argb >> 16) & 0xFF) as u8;
            frame[offset + 1] = ((argb >> 8) & 0xFF) as u8;
            frame[offset + 2] = (argb & 0xFF) as u8;
            frame[offset + 3] = 0xFF;
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_size = winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, FB_HEIGHT * SCALE);
        let attrs = WindowAttributes::default()
            .with_title("Amiga 500")
            .with_inner_size(window_size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window: &'static Window = Box::leak(Box::new(window));
                let inner = window.inner_size();
                let surface = SurfaceTexture::new(inner.width, inner.height, window);
                match Pixels::new(FB_WIDTH, FB_HEIGHT, surface) {
                    Ok(pixels) => {
                        self.pixels = Some(pixels);
                    }
                    Err(e) => {
                        eprintln!("Failed to create pixels: {e}");
                        event_loop.exit();
                        return;
                    }
                }
                self.window = Some(window);
            }
            Err(e) => {
                eprintln!("Failed to create window: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key
                    && keycode == KeyCode::Escape
                    && event.state == ElementState::Pressed
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.amiga.run_frame();
                    self.update_pixels();
                    self.last_frame_time = now;
                }

                if let Some(pixels) = self.pixels.as_ref()
                    && let Err(e) = pixels.render()
                {
                    eprintln!("Render error: {e}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window {
            window.request_redraw();
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn make_amiga(cli: &CliArgs) -> Amiga {
    let kickstart_path = cli.kickstart_path.as_ref().unwrap_or_else(|| {
        eprintln!("No Kickstart ROM specified. Use --kickstart <file>");
        process::exit(1);
    });

    let kickstart_data = match std::fs::read(kickstart_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "Failed to read Kickstart ROM {}: {e}",
                kickstart_path.display()
            );
            process::exit(1);
        }
    };

    let config = AmigaConfig {
        kickstart: kickstart_data,
    };
    match Amiga::new(&config) {
        Ok(amiga) => {
            eprintln!("Loaded Kickstart: {}", kickstart_path.display());
            amiga
        }
        Err(e) => {
            eprintln!("Failed to initialize Amiga: {e}");
            process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let cli = parse_args();

    if cli.mcp {
        let mut server = McpServer::new();
        if let Some(ref path) = cli.kickstart_path {
            server.set_kickstart_path(path.clone());
        }
        server.run();
        return;
    }

    if cli.headless {
        run_headless(&cli);
        return;
    }

    let amiga = make_amiga(&cli);
    let mut app = App::new(amiga);

    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("Failed to create event loop: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {e}");
        process::exit(1);
    }
}
