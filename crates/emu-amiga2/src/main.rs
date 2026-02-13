//! Amiga emulator binary.
//!
//! Supports windowed mode (winit + pixels) and headless mode for
//! screenshots and batch testing.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu_amiga2::{capture, Amiga, AmigaConfig};
use emu_amiga2::config::AmigaModel;
use emu_amiga2::denise;
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const FB_WIDTH: u32 = denise::FB_WIDTH;
const FB_HEIGHT: u32 = denise::FB_HEIGHT;
const SCALE: u32 = 2;
const FRAME_DURATION: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct CliArgs {
    kickstart_path: Option<PathBuf>,
    model: AmigaModel,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        kickstart_path: None,
        model: AmigaModel::A1000,
        headless: false,
        frames: 200,
        screenshot_path: None,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--kickstart" | "--rom" => {
                i += 1;
                cli.kickstart_path = args.get(i).map(PathBuf::from);
            }
            "--model" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.model = match s.to_lowercase().as_str() {
                        "a1000" => AmigaModel::A1000,
                        "a500" => AmigaModel::A500,
                        "a500+" | "a500plus" => AmigaModel::A500Plus,
                        "a600" => AmigaModel::A600,
                        "a2000" => AmigaModel::A2000,
                        "a1200" => AmigaModel::A1200,
                        other => {
                            eprintln!("Unknown model: {other}");
                            process::exit(1);
                        }
                    };
                }
            }
            "--headless" => cli.headless = true,
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
            "--help" | "-h" => {
                eprintln!("Usage: emu-amiga2 [OPTIONS]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --kickstart <file>   Kickstart ROM/WCS file (256K)");
                eprintln!("  --model <name>       Model: a1000, a500, a500+, a600, a2000, a1200");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --frames <n>         Number of frames in headless mode [default: 200]");
                eprintln!("  --screenshot <file>  Save a PNG screenshot (headless)");
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

    // Diagnostic: dump initial CPU state
    {
        let regs = amiga.cpu().registers();
        eprintln!("Initial: PC=${:08X} SSP=${:08X} SR=${:04X}", regs.pc, regs.ssp, regs.sr);
        eprintln!("Stopped={} Halted={}", amiga.cpu().is_stopped(), amiga.cpu().is_halted());
    }

    for i in 0..cli.frames {
        amiga.run_frame();
        if i < 5 || (15..=55).contains(&i) || i % 50 == 0 {
            let regs = amiga.cpu().registers();
            let overlay = amiga.bus().memory.overlay;
            let cpu_ticks = amiga.cpu().total_cycles().0;
            eprintln!(
                "Frame {i}: PC=${:08X} SR=${:04X} D0=${:08X} D1=${:08X} A0=${:08X} A7=${:08X} ovl={overlay} ticks={cpu_ticks}",
                regs.pc, regs.sr, regs.d[0], regs.d[1], regs.a[0], regs.ssp,
            );
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
// Windowed mode
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
        let model_name = format!("Amiga ({:?})", self.amiga.model());
        let attrs = WindowAttributes::default()
            .with_title(model_name)
            .with_inner_size(window_size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window: &'static Window = Box::leak(Box::new(window));
                let inner = window.inner_size();
                let surface = SurfaceTexture::new(inner.width, inner.height, window);
                match Pixels::new(FB_WIDTH, FB_HEIGHT, surface) {
                    Ok(pixels) => self.pixels = Some(pixels),
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    if keycode == KeyCode::Escape && event.state == ElementState::Pressed {
                        event_loop.exit();
                    }
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

    let config = AmigaConfig::preset(cli.model, kickstart_data);
    match Amiga::new(&config) {
        Ok(amiga) => {
            eprintln!(
                "Loaded Kickstart: {} (model: {:?})",
                kickstart_path.display(),
                cli.model,
            );
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
