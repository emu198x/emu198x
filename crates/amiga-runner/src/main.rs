//! Minimal windowed runner for the Amiga machine core.
//!
//! Scope: video output only (no host audio yet). Loads a Kickstart ROM and
//! optionally inserts an ADF into DF0:, then continuously runs the machine and
//! displays the raw 320x256 framebuffer.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use machine_amiga::format_adf::Adf;
use machine_amiga::{
    Amiga, TICKS_PER_CCK, commodore_agnus_ocs, commodore_denise_ocs,
};
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const FB_WIDTH: u32 = commodore_denise_ocs::FB_WIDTH;
const FB_HEIGHT: u32 = commodore_denise_ocs::FB_HEIGHT;
const SCALE: u32 = 3;
const FRAME_DURATION: Duration = Duration::from_millis(20); // PAL ~50 Hz
const PAL_FRAME_TICKS: u64 = (commodore_agnus_ocs::PAL_CCKS_PER_LINE as u64)
    * (commodore_agnus_ocs::PAL_LINES_PER_FRAME as u64)
    * TICKS_PER_CCK;

struct CliArgs {
    rom_path: PathBuf,
    adf_path: Option<PathBuf>,
}

fn print_usage_and_exit(code: i32) -> ! {
    eprintln!("Usage: amiga-runner [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --rom <file>   Kickstart ROM file (or use AMIGA_KS13_ROM env var)");
    eprintln!("  --adf <file>   Optional ADF disk image to insert into DF0:");
    eprintln!("  -h, --help     Show this help");
    process::exit(code);
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut rom_path: Option<PathBuf> = None;
    let mut adf_path: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--rom" => {
                i += 1;
                rom_path = args.get(i).map(PathBuf::from);
            }
            "--adf" => {
                i += 1;
                adf_path = args.get(i).map(PathBuf::from);
            }
            "-h" | "--help" => print_usage_and_exit(0),
            other => {
                eprintln!("Unknown argument: {other}");
                print_usage_and_exit(1);
            }
        }
        i += 1;
    }

    let rom_path = rom_path
        .or_else(|| std::env::var_os("AMIGA_KS13_ROM").map(PathBuf::from))
        .unwrap_or_else(|| {
            eprintln!("No Kickstart ROM specified.");
            print_usage_and_exit(1);
        });

    CliArgs { rom_path, adf_path }
}

fn make_amiga(cli: &CliArgs) -> Amiga {
    let kickstart = match std::fs::read(&cli.rom_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!(
                "Failed to read Kickstart ROM {}: {e}",
                cli.rom_path.display()
            );
            process::exit(1);
        }
    };

    let mut amiga = Amiga::new(kickstart);

    if let Some(adf_path) = &cli.adf_path {
        let adf_bytes = match std::fs::read(adf_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read ADF {}: {e}", adf_path.display());
                process::exit(1);
            }
        };
        let adf = match Adf::from_bytes(adf_bytes) {
            Ok(adf) => adf,
            Err(e) => {
                eprintln!("Invalid ADF {}: {e}", adf_path.display());
                process::exit(1);
            }
        };
        amiga.insert_disk(adf);
        eprintln!("Inserted disk: {}", adf_path.display());
    }

    eprintln!("Loaded Kickstart ROM: {}", cli.rom_path.display());
    amiga
}

fn run_one_pal_frame(amiga: &mut Amiga) {
    for _ in 0..PAL_FRAME_TICKS {
        amiga.tick();
    }
}

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

        let frame = pixels.frame_mut();
        let fb = &self.amiga.denise.framebuffer;

        for (i, &argb) in fb.iter().enumerate() {
            let o = i * 4;
            frame[o] = ((argb >> 16) & 0xFF) as u8; // R
            frame[o + 1] = ((argb >> 8) & 0xFF) as u8; // G
            frame[o + 2] = (argb & 0xFF) as u8; // B
            frame[o + 3] = ((argb >> 24) & 0xFF) as u8; // A
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let size = winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, FB_HEIGHT * SCALE);
        let attrs = WindowAttributes::default()
            .with_title("Amiga Runner (A500/OCS)")
            .with_inner_size(size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window: &'static Window = Box::leak(Box::new(window));
                let inner = window.inner_size();
                let surface = SurfaceTexture::new(inner.width, inner.height, window);
                let pixels = match Pixels::new(FB_WIDTH, FB_HEIGHT, surface) {
                    Ok(pixels) => pixels,
                    Err(e) => {
                        eprintln!("Failed to create pixels surface: {e}");
                        event_loop.exit();
                        return;
                    }
                };

                self.pixels = Some(pixels);
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
                if let PhysicalKey::Code(code) = event.physical_key
                    && code == KeyCode::Escape
                    && event.state == ElementState::Pressed
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    run_one_pal_frame(&mut self.amiga);
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

fn main() {
    let cli = parse_args();
    let amiga = make_amiga(&cli);
    let mut app = App::new(amiga);

    let event_loop = match EventLoop::new() {
        Ok(loop_) => loop_,
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
