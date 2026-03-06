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

fn next_option_value(args: &[String], index: &mut usize, flag: &str) -> Result<PathBuf, String> {
    *index += 1;
    let value = args
        .get(*index)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("{flag} requires a value"))?;
    Ok(PathBuf::from(value))
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
                cli.rom_path = Some(next_option_value(args, &mut i, "--rom")?);
            }
            "--headless" => {
                cli.headless = true;
            }
            "--mcp" => {
                cli.mcp = true;
            }
            "--script" => {
                cli.script_path = Some(next_option_value(args, &mut i, "--script")?);
            }
            "--frames" => {
                i += 1;
                let value = args
                    .get(i)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--frames requires a value".to_string())?;
                cli.frames = value
                    .parse()
                    .map_err(|_| format!("Invalid value for --frames: {value}"))?;
            }
            "--screenshot" => {
                cli.screenshot_path = Some(next_option_value(args, &mut i, "--screenshot")?);
            }
            "--record" => {
                cli.record_dir = Some(next_option_value(args, &mut i, "--record")?);
            }
            "--region" => {
                i += 1;
                let value = args
                    .get(i)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--region requires a value".to_string())?;
                cli.region = match value.to_lowercase().as_str() {
                    "ntsc" => NesRegion::Ntsc,
                    "pal" => NesRegion::Pal,
                    _ => return Err(format!("Invalid value for --region: {value}")),
                };
            }
            "--help" | "-h" => return Ok(None),
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
        i += 1;
    }

    if cli.screenshot_path.is_some() || cli.record_dir.is_some() {
        cli.headless = true;
    }

    if cli.mcp && cli.script_path.is_some() {
        return Err("--mcp and --script are mutually exclusive".to_string());
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

fn make_nes_result(cli: &CliArgs) -> Result<Nes, String> {
    let rom_path = cli
        .rom_path
        .as_ref()
        .ok_or_else(|| "No ROM file specified. Use --rom <file.nes>".to_string())?;

    let rom_data = std::fs::read(rom_path)
        .map_err(|e| format!("Failed to read ROM file {}: {e}", rom_path.display()))?;

    let config = NesConfig {
        rom_data,
        region: cli.region,
    };
    Nes::new(&config).map_err(|e| format!("Failed to load ROM: {e}"))
}

fn make_nes(cli: &CliArgs) -> Nes {
    match make_nes_result(cli) {
        Ok(nes) => {
            if let Some(ref rom_path) = cli.rom_path {
                eprintln!("Loaded ROM: {}", rom_path.display());
            }
            nes
        }
        Err(e) => {
            eprintln!("{e}");
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
    use super::{CliArgs, make_nes_result, parse_args_from};
    use emu_nes::NesRegion;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parse_cli(args: &[&str]) -> Result<Option<CliArgs>, String> {
        let args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        parse_args_from(&args)
    }

    struct TempRomFile {
        path: PathBuf,
    }

    impl TempRomFile {
        fn new(contents: &[u8]) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            path.push(format!("emu-nes-test-{}-{nanos}.nes", process::id()));
            fs::write(&path, contents).expect("temp rom should be written");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRomFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn minimal_ines_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 16 + 16_384];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 1;
        rom[5] = 0;
        rom[16] = 0xEA;
        rom[16 + 0x3FFC] = 0x00;
        rom[16 + 0x3FFD] = 0x80;
        rom
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
    fn cli_parser_rejects_missing_or_invalid_values() {
        let invalid_frames = parse_cli(&["emu-nes", "--frames", "abc"])
            .expect_err("invalid frame count should fail");
        assert!(invalid_frames.contains("Invalid value for --frames: abc"));

        let invalid_region =
            parse_cli(&["emu-nes", "--region", "weird"]).expect_err("invalid region should fail");
        assert!(invalid_region.contains("Invalid value for --region: weird"));

        let missing_rom = parse_cli(&["emu-nes", "--rom", "--headless"])
            .expect_err("missing rom value should fail");
        assert!(missing_rom.contains("--rom requires a value"));
    }

    #[test]
    fn cli_parser_promotes_capture_modes_to_headless() {
        let cli = parse_cli(&[
            "emu-nes",
            "--rom",
            "mario.nes",
            "--screenshot",
            "out.png",
            "--record",
            "frames",
        ])
        .expect("parse should succeed")
        .expect("help was not requested");

        assert!(cli.headless);
        assert_eq!(cli.screenshot_path, Some(PathBuf::from("out.png")));
        assert_eq!(cli.record_dir, Some(PathBuf::from("frames")));
    }

    #[test]
    fn cli_parser_reports_help_unknown_args_and_conflicts() {
        assert!(matches!(
            parse_cli(&["emu-nes", "--help"]).expect("help parse should succeed"),
            None
        ));

        let result = parse_cli(&["emu-nes", "--bogus"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown argument: --bogus"));

        let conflict = parse_cli(&["emu-nes", "--mcp", "--script", "demo.json"])
            .expect_err("mcp/script conflict should fail");
        assert!(conflict.contains("--mcp and --script are mutually exclusive"));
    }

    #[test]
    fn make_nes_result_requires_rom_path() {
        let cli = CliArgs {
            rom_path: None,
            headless: true,
            mcp: false,
            script_path: None,
            frames: 1,
            screenshot_path: None,
            record_dir: None,
            region: NesRegion::Ntsc,
        };

        let error = match make_nes_result(&cli) {
            Ok(_) => panic!("missing rom should fail"),
            Err(error) => error,
        };
        assert!(error.contains("No ROM file specified"));
    }

    #[test]
    fn make_nes_result_reports_missing_and_invalid_roms() {
        let mut missing_path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        missing_path.push(format!("emu-nes-missing-{}-{nanos}.nes", process::id()));

        let missing_cli = CliArgs {
            rom_path: Some(missing_path.clone()),
            headless: true,
            mcp: false,
            script_path: None,
            frames: 1,
            screenshot_path: None,
            record_dir: None,
            region: NesRegion::Ntsc,
        };
        let missing = match make_nes_result(&missing_cli) {
            Ok(_) => panic!("missing file should fail"),
            Err(error) => error,
        };
        assert!(missing.contains("Failed to read ROM file"));
        assert!(missing.contains(missing_path.to_string_lossy().as_ref()));

        let invalid_rom = TempRomFile::new(b"this is not a valid iNES file");
        let invalid_cli = CliArgs {
            rom_path: Some(invalid_rom.path().to_path_buf()),
            headless: true,
            mcp: false,
            script_path: None,
            frames: 1,
            screenshot_path: None,
            record_dir: None,
            region: NesRegion::Pal,
        };
        let invalid = match make_nes_result(&invalid_cli) {
            Ok(_) => panic!("invalid rom should fail"),
            Err(error) => error,
        };
        assert!(invalid.contains("Failed to load ROM: Invalid iNES magic"));
    }

    #[test]
    fn make_nes_result_loads_minimal_valid_rom() {
        let rom = TempRomFile::new(&minimal_ines_rom());
        let cli = CliArgs {
            rom_path: Some(rom.path().to_path_buf()),
            headless: true,
            mcp: false,
            script_path: None,
            frames: 1,
            screenshot_path: None,
            record_dir: None,
            region: NesRegion::Pal,
        };

        let nes = make_nes_result(&cli).expect("valid rom should load");
        assert_eq!(nes.region(), NesRegion::Pal);
    }
}
