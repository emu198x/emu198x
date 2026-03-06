//! NES emulator binary.
//!
//! Runs the NES with a winit window and pixels framebuffer, or in
//! headless mode for screenshots, or as an MCP server.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu_nes::mcp::{McpServer, NesMcp};
use emu_nes::ppu;
use emu_nes::{Nes, NesConfig, NesRegion, capture, controller_map};
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

#[derive(Debug)]
struct CliArgs {
    rom_path: Option<PathBuf>,
    headless: bool,
    mcp: bool,
    script_path: Option<PathBuf>,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    record_dir: Option<PathBuf>,
    region: NesRegion,
}

fn print_usage() {
    eprintln!("Usage: emu-nes [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --rom <file>         iNES ROM file (.nes)");
    eprintln!("  --region <ntsc|pal>  Video region (default: ntsc)");
    eprintln!("  --headless           Run without a window");
    eprintln!("  --mcp                Run as MCP server (JSON-RPC over stdio)");
    eprintln!("  --script <file>      Run a JSON script file (headless batch mode)");
    eprintln!("  --frames <n>         Number of frames in headless mode [default: 200]");
    eprintln!("  --screenshot <file>  Save a PNG screenshot (headless)");
    eprintln!("  --record <dir>       Record frames to directory (headless)");
}

fn print_usage_and_exit(code: i32) -> ! {
    print_usage();
    process::exit(code);
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();

    match parse_args_from(&args) {
        Ok(Some(cli)) => cli,
        Ok(None) => print_usage_and_exit(0),
        Err(e) => {
            eprintln!("{e}");
            print_usage_and_exit(1);
        }
    }
}

fn parse_args_from(args: &[String]) -> Result<Option<CliArgs>, String> {
    let mut cli = CliArgs {
        rom_path: None,
        headless: false,
        mcp: false,
        script_path: None,
        frames: 200,
        screenshot_path: None,
        record_dir: None,
        region: NesRegion::Ntsc,
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
            "--script" => {
                i += 1;
                cli.script_path = args.get(i).map(PathBuf::from);
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
            "--region" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.region = match s.to_lowercase().as_str() {
                        "pal" => NesRegion::Pal,
                        _ => NesRegion::Ntsc,
                    };
                }
            }
            "--help" | "-h" => return Ok(None),
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
        i += 1;
    }

    Ok(Some(cli))
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
                    // Drain audio buffer to prevent unbounded growth
                    let _ = self.nes.take_audio_buffer();
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

    let config = NesConfig {
        rom_data,
        region: cli.region,
    };
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
        let mut inner = NesMcp::new();
        if let Some(ref path) = cli.rom_path {
            inner.set_rom_path(path.clone());
        }
        let mut server = McpServer::new(inner);
        server.run();
        return;
    }

    if let Some(ref path) = cli.script_path {
        let mut inner = NesMcp::new();
        if let Some(ref rom) = cli.rom_path {
            inner.set_rom_path(rom.clone());
        }
        let mut server = McpServer::new(inner);
        if let Err(e) = server.run_script(path) {
            eprintln!("Script error: {e}");
            process::exit(1);
        }
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

#[cfg(test)]
mod tests {
    use super::{CliArgs, parse_args_from};
    use emu_nes::NesRegion;
    use std::path::PathBuf;

    fn parse_cli(args: &[&str]) -> Result<Option<CliArgs>, String> {
        let args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        parse_args_from(&args)
    }

    #[test]
    fn cli_parser_reads_basic_modes_and_paths() {
        let cli = parse_cli(&[
            "emu-nes",
            "--rom",
            "mario.nes",
            "--headless",
            "--script",
            "demo.json",
            "--screenshot",
            "out.png",
            "--record",
            "frames",
            "--region",
            "pal",
            "--frames",
            "42",
        ])
        .expect("parse should succeed")
        .expect("help was not requested");

        assert_eq!(cli.rom_path, Some(PathBuf::from("mario.nes")));
        assert!(cli.headless);
        assert_eq!(cli.script_path, Some(PathBuf::from("demo.json")));
        assert_eq!(cli.screenshot_path, Some(PathBuf::from("out.png")));
        assert_eq!(cli.record_dir, Some(PathBuf::from("frames")));
        assert_eq!(cli.region, NesRegion::Pal);
        assert_eq!(cli.frames, 42);
    }

    #[test]
    fn cli_parser_defaults_invalid_frames_and_region_to_ntsc() {
        let cli = parse_cli(&["emu-nes", "--frames", "abc", "--region", "weird"])
            .expect("parse should succeed")
            .expect("help was not requested");

        assert_eq!(cli.frames, 200);
        assert_eq!(cli.region, NesRegion::Ntsc);
    }

    #[test]
    fn cli_parser_reports_help_and_unknown_args() {
        assert!(matches!(
            parse_cli(&["emu-nes", "--help"]).expect("help parse should succeed"),
            None
        ));

        let result = parse_cli(&["emu-nes", "--bogus"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown argument: --bogus"));
    }

    #[test]
    fn cli_parser_keeps_current_missing_value_behavior_for_path_flags() {
        let cli = parse_cli(&["emu-nes", "--rom", "--script"])
            .expect("parse should succeed")
            .expect("help was not requested");

        assert_eq!(cli.rom_path, Some(PathBuf::from("--script")));
        assert_eq!(cli.script_path, None);
    }
}
