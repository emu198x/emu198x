//! NES emulator binary.
//!
//! Runs the NES with a winit window and pixels framebuffer, or in
//! headless mode for screenshots, or as an MCP server.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu_nes::ppu;
use emu_nes::{Nes, NesConfig, capture, controller_map, mcp::McpServer};
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// NES framebuffer dimensions.
const FB_WIDTH: u32 = ppu::FB_WIDTH;
const FB_HEIGHT: u32 = ppu::FB_HEIGHT;

/// Window scale factor.
const SCALE: u32 = 3;

/// Frame duration for ~60 Hz NTSC.
const FRAME_DURATION: Duration = Duration::from_micros(16_639);

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct CliArgs {
    rom_path: Option<PathBuf>,
    headless: bool,
    mcp: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    record_dir: Option<PathBuf>,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        rom_path: None,
        headless: false,
        mcp: false,
        frames: 200,
        screenshot_path: None,
        record_dir: None,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--rom" => {
                i += 1;
                cli.rom_path = args.get(i).map(PathBuf::from);
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
                eprintln!("Usage: emu-nes [OPTIONS]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --rom <file>         iNES ROM file (.nes)");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --mcp                Run as MCP server (JSON-RPC over stdio)");
                eprintln!(
                    "  --frames <n>         Number of frames in headless mode [default: 200]"
                );
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
    let mut nes = make_nes(cli);

    if let Some(ref dir) = cli.record_dir {
        if let Err(e) = capture::record(&mut nes, dir, cli.frames) {
            eprintln!("Record error: {e}");
            process::exit(1);
        }
        return;
    }

    for _ in 0..cli.frames {
        nes.run_frame();
    }

    if let Some(ref path) = cli.screenshot_path {
        if let Err(e) = capture::save_screenshot(&nes, path) {
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
    nes: Nes,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    last_frame_time: Instant,
}

impl App {
    fn new(nes: Nes) -> Self {
        Self {
            nes,
            window: None,
            pixels: None,
            last_frame_time: Instant::now(),
        }
    }

    fn handle_key(&mut self, keycode: KeyCode, pressed: bool) {
        if let Some(button) = controller_map::map_keycode(keycode) {
            if pressed {
                self.nes.press_button(button);
            } else {
                self.nes.release_button(button);
            }
        }
    }

    fn update_pixels(&mut self) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        let fb = self.nes.framebuffer();
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
            .with_title("NES")
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
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    if keycode == KeyCode::Escape && event.state == ElementState::Pressed {
                        event_loop.exit();
                        return;
                    }
                    self.handle_key(keycode, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.nes.run_frame();
                    self.update_pixels();
                    self.last_frame_time = now;
                }

                if let Some(pixels) = self.pixels.as_ref() {
                    if let Err(e) = pixels.render() {
                        eprintln!("Render error: {e}");
                        event_loop.exit();
                    }
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

fn make_nes(cli: &CliArgs) -> Nes {
    let rom_path = cli.rom_path.as_ref().unwrap_or_else(|| {
        eprintln!("No ROM file specified. Use --rom <file.nes>");
        process::exit(1);
    });

    let rom_data = match std::fs::read(rom_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read ROM file {}: {e}", rom_path.display());
            process::exit(1);
        }
    };

    let config = NesConfig { rom_data };
    match Nes::new(&config) {
        Ok(nes) => {
            eprintln!("Loaded ROM: {}", rom_path.display());
            nes
        }
        Err(e) => {
            eprintln!("Failed to load ROM: {e}");
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
        if let Some(ref path) = cli.rom_path {
            server.set_rom_path(path.clone());
        }
        server.run();
        return;
    }

    if cli.headless {
        run_headless(&cli);
        return;
    }

    let nes = make_nes(&cli);
    let mut app = App::new(nes);

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
