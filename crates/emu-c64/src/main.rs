//! Commodore 64 emulator binary.
//!
//! Runs the C64 with a winit window and pixels framebuffer, or in
//! headless mode for screenshots, or as an MCP server.

#![allow(clippy::cast_possible_truncation)]

use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant};

use emu_c64::{C64, C64Config, C64Model, capture, keyboard_map, mcp::McpServer};
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// C64 framebuffer dimensions.
const FB_WIDTH: u32 = emu_c64::vic::FB_WIDTH;
const FB_HEIGHT: u32 = emu_c64::vic::FB_HEIGHT;

/// Window scale factor.
const SCALE: u32 = 3;

/// Frame duration for ~50 Hz PAL.
const FRAME_DURATION: Duration = Duration::from_micros(19_950);

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct CliArgs {
    prg_path: Option<PathBuf>,
    headless: bool,
    mcp: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    record_dir: Option<PathBuf>,
    type_text: Option<String>,
    type_at: u64,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        prg_path: None,
        headless: false,
        mcp: false,
        frames: 200,
        screenshot_path: None,
        record_dir: None,
        type_text: None,
        type_at: 100,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--prg" => {
                i += 1;
                cli.prg_path = args.get(i).map(PathBuf::from);
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
            "--type" => {
                i += 1;
                cli.type_text = args.get(i).cloned();
            }
            "--type-at" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.type_at = s.parse().unwrap_or(100);
                }
            }
            "--help" | "-h" => {
                eprintln!("Usage: emu-c64 [OPTIONS]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --prg <file>         Load a PRG file into memory");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --mcp                Run as MCP server (JSON-RPC over stdio)");
                eprintln!(
                    "  --frames <n>         Number of frames in headless mode [default: 200]"
                );
                eprintln!("  --screenshot <file>  Save a PNG screenshot (headless)");
                eprintln!("  --record <dir>       Record frames to directory (headless)");
                eprintln!("  --type <text>        Type text into the C64 (use \\n for Return)");
                eprintln!("  --type-at <frame>    Frame at which to start typing [default: 100]");
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
    let mut c64 = make_c64(cli);

    if let Some(ref dir) = cli.record_dir {
        if let Err(e) = capture::record(&mut c64, dir, cli.frames) {
            eprintln!("Record error: {e}");
            process::exit(1);
        }
        return;
    }

    for _ in 0..cli.frames {
        c64.run_frame();
    }

    if let Some(ref path) = cli.screenshot_path {
        if let Err(e) = capture::save_screenshot(&c64, path) {
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
    c64: C64,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    last_frame_time: Instant,
}

impl App {
    fn new(c64: C64) -> Self {
        Self {
            c64,
            window: None,
            pixels: None,
            last_frame_time: Instant::now(),
        }
    }

    fn handle_key(&mut self, keycode: KeyCode, pressed: bool) {
        // Cursor up = SHIFT + CURSOR DOWN
        if keycode == KeyCode::ArrowUp {
            let keys = keyboard_map::cursor_up_keys();
            for key in keys {
                if pressed {
                    self.c64.press_key(key);
                } else {
                    self.c64.release_key(key);
                }
            }
            return;
        }

        // Cursor left = SHIFT + CURSOR RIGHT
        if keycode == KeyCode::ArrowLeft {
            let keys = keyboard_map::cursor_left_keys();
            for key in keys {
                if pressed {
                    self.c64.press_key(key);
                } else {
                    self.c64.release_key(key);
                }
            }
            return;
        }

        if let Some(key) = keyboard_map::map_keycode(keycode) {
            if pressed {
                self.c64.press_key(key);
            } else {
                self.c64.release_key(key);
            }
        }
    }

    fn update_pixels(&mut self) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        let fb = self.c64.framebuffer();
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
            .with_title("Commodore 64")
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
                    self.c64.run_frame();
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

/// Load a ROM file, or exit with an error message.
fn load_rom(path: &Path, name: &str, expected_size: usize) -> Vec<u8> {
    match std::fs::read(path) {
        Ok(data) => {
            if data.len() != expected_size {
                eprintln!(
                    "{name} ROM at {} is {} bytes, expected {expected_size}",
                    path.display(),
                    data.len()
                );
                process::exit(1);
            }
            data
        }
        Err(e) => {
            eprintln!("Cannot read {name} ROM at {}: {e}", path.display());
            eprintln!();
            eprintln!("Place C64 ROM files in the roms/ directory:");
            eprintln!("  roms/kernal.rom  (8192 bytes)");
            eprintln!("  roms/basic.rom   (8192 bytes)");
            eprintln!("  roms/chargen.rom (4096 bytes)");
            process::exit(1);
        }
    }
}

/// Find the roms/ directory relative to the executable or current directory.
fn find_roms_dir() -> PathBuf {
    // Try relative to the executable
    if let Ok(exe) = std::env::current_exe() {
        // Walk up from target/debug or target/release to workspace root
        let mut dir = exe.parent().map(Path::to_path_buf);
        for _ in 0..5 {
            if let Some(ref d) = dir {
                let roms = d.join("roms");
                if roms.is_dir() {
                    return roms;
                }
                dir = d.parent().map(Path::to_path_buf);
            }
        }
    }
    // Fallback: roms/ relative to cwd
    PathBuf::from("roms")
}

fn load_c64_config() -> C64Config {
    let roms_dir = find_roms_dir();
    C64Config {
        model: C64Model::C64Pal,
        kernal_rom: load_rom(&roms_dir.join("kernal.rom"), "Kernal", 8192),
        basic_rom: load_rom(&roms_dir.join("basic.rom"), "BASIC", 8192),
        char_rom: load_rom(&roms_dir.join("chargen.rom"), "Character", 4096),
    }
}

fn make_c64(cli: &CliArgs) -> C64 {
    let config = load_c64_config();
    let mut c64 = C64::new(&config);

    if let Some(ref path) = cli.prg_path {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read PRG file {}: {e}", path.display());
                process::exit(1);
            }
        };
        match c64.load_prg(&data) {
            Ok(addr) => eprintln!("Loaded PRG at ${addr:04X}: {}", path.display()),
            Err(e) => {
                eprintln!("Failed to load PRG: {e}");
                process::exit(1);
            }
        }
    }

    if let Some(ref text) = cli.type_text {
        let text = text.replace("\\n", "\n");
        c64.input_queue().enqueue_text(&text, cli.type_at);
    }

    c64
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let cli = parse_args();

    if cli.mcp {
        let mut server = McpServer::new();
        server.run();
        return;
    }

    if cli.headless {
        run_headless(&cli);
        return;
    }

    let c64 = make_c64(&cli);
    let mut app = App::new(c64);

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
