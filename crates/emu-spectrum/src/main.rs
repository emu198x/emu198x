//! ZX Spectrum 48K emulator binary.
//!
//! Runs the Spectrum with a winit window and pixels framebuffer, or in
//! headless mode for screenshots and audio capture.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu_spectrum::{
    Spectrum, SpectrumConfig, SpectrumModel, TapFile, capture, keyboard_map, load_sna,
    mcp::McpServer,
};
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Embedded 48K ROM — compiled into the binary.
const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

/// Spectrum framebuffer dimensions.
const FB_WIDTH: u32 = 320;
const FB_HEIGHT: u32 = 288;

/// Window scale factor.
const SCALE: u32 = 3;

/// Frame duration for 50 Hz PAL.
const FRAME_DURATION: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct CliArgs {
    sna_path: Option<PathBuf>,
    tap_path: Option<PathBuf>,
    headless: bool,
    mcp: bool,
    script_path: Option<PathBuf>,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    record_dir: Option<PathBuf>,
    type_text: Option<String>,
    type_at: u64,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        sna_path: None,
        tap_path: None,
        headless: false,
        mcp: false,
        script_path: None,
        frames: 200,
        screenshot_path: None,
        audio_path: None,
        record_dir: None,
        type_text: None,
        type_at: 100,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--sna" => {
                i += 1;
                cli.sna_path = args.get(i).map(PathBuf::from);
            }
            "--tap" => {
                i += 1;
                cli.tap_path = args.get(i).map(PathBuf::from);
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
            "--audio" => {
                i += 1;
                cli.audio_path = args.get(i).map(PathBuf::from);
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
                eprintln!("Usage: emu-spectrum [OPTIONS]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --sna <file>         Load a 48K SNA snapshot");
                eprintln!("  --tap <file>         Insert a TAP file into the tape deck");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --mcp                Run as MCP server (JSON-RPC over stdio)");
                eprintln!("  --script <file>      Run a JSON script file (headless batch mode)");
                eprintln!(
                    "  --frames <n>         Number of frames in headless mode [default: 200]"
                );
                eprintln!("  --screenshot <file>  Save a PNG screenshot (headless)");
                eprintln!("  --audio <file>       Save a WAV audio dump (headless)");
                eprintln!("  --record <dir>       Record frames + audio to directory (headless)");
                eprintln!("  --type <text>        Type text into the Spectrum (use \\n for Enter)");
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
    let mut spectrum = make_spectrum(cli);

    // If recording, use the record function which handles its own frame loop.
    if let Some(ref dir) = cli.record_dir {
        if let Err(e) = capture::record(&mut spectrum, dir, cli.frames) {
            eprintln!("Record error: {e}");
            process::exit(1);
        }
        return;
    }

    // Run frames, collecting audio.
    let mut all_audio = Vec::new();
    for _ in 0..cli.frames {
        spectrum.run_frame();
        let audio = spectrum.take_audio_buffer();
        all_audio.extend_from_slice(&audio);
    }

    // Save screenshot.
    if let Some(ref path) = cli.screenshot_path {
        if let Err(e) = capture::save_screenshot(&spectrum, path) {
            eprintln!("Screenshot error: {e}");
            process::exit(1);
        }
        eprintln!("Screenshot saved to {}", path.display());
    }

    // Save audio.
    if let Some(ref path) = cli.audio_path {
        if let Err(e) = capture::save_audio(&all_audio, path) {
            eprintln!("Audio error: {e}");
            process::exit(1);
        }
        eprintln!("Audio saved to {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// Windowed mode (winit + pixels)
// ---------------------------------------------------------------------------

struct App {
    spectrum: Spectrum,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    last_frame_time: Instant,
}

impl App {
    fn new(spectrum: Spectrum) -> Self {
        Self {
            spectrum,
            window: None,
            pixels: None,
            last_frame_time: Instant::now(),
        }
    }

    fn handle_key(&mut self, keycode: KeyCode, pressed: bool) {
        // Backspace is a combo key (CAPS SHIFT + 0)
        if keycode == KeyCode::Backspace {
            let keys = keyboard_map::backspace_keys();
            for key in keys {
                if pressed {
                    self.spectrum.press_key(key);
                } else {
                    self.spectrum.release_key(key);
                }
            }
            return;
        }

        if let Some(key) = keyboard_map::map_keycode(keycode) {
            if pressed {
                self.spectrum.press_key(key);
            } else {
                self.spectrum.release_key(key);
            }
        }
    }

    fn update_pixels(&mut self) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        let fb = self.spectrum.framebuffer();
        let frame = pixels.frame_mut();

        // Convert ARGB32 → RGBA8 for pixels buffer
        for (i, &argb) in fb.iter().enumerate() {
            let offset = i * 4;
            frame[offset] = ((argb >> 16) & 0xFF) as u8; // R
            frame[offset + 1] = ((argb >> 8) & 0xFF) as u8; // G
            frame[offset + 2] = (argb & 0xFF) as u8; // B
            frame[offset + 3] = 0xFF; // A
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already created
        }

        let window_size = winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, FB_HEIGHT * SCALE);
        let attrs = WindowAttributes::default()
            .with_title("ZX Spectrum 48K")
            .with_inner_size(window_size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                // Leak the window to get a 'static reference. This is intentional:
                // the window lives for the entire application lifetime and is never
                // reclaimed (the OS reclaims it on process exit).
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
                    // Escape exits
                    if keycode == KeyCode::Escape && event.state == ElementState::Pressed {
                        event_loop.exit();
                        return;
                    }
                    self.handle_key(keycode, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => {
                // Throttle to ~50 Hz
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.spectrum.run_frame();
                    // Drain audio buffer (not wired to output device yet)
                    let _ = self.spectrum.take_audio_buffer();
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

fn make_spectrum(cli: &CliArgs) -> Spectrum {
    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: ROM_48K.to_vec(),
    };
    let mut spectrum = Spectrum::new(&config);

    // Load SNA snapshot if provided.
    if let Some(ref path) = cli.sna_path {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read SNA file {}: {e}", path.display());
                process::exit(1);
            }
        };
        if let Err(e) = load_sna(&mut spectrum, &data) {
            eprintln!("Failed to load SNA: {e}");
            process::exit(1);
        }
        eprintln!("Loaded SNA: {}", path.display());
    }

    // Insert TAP file if provided.
    if let Some(ref path) = cli.tap_path {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read TAP file {}: {e}", path.display());
                process::exit(1);
            }
        };
        match TapFile::parse(&data) {
            Ok(tap) => {
                eprintln!(
                    "Inserted TAP: {} ({} blocks)",
                    path.display(),
                    tap.blocks.len()
                );
                spectrum.insert_tap(tap);
            }
            Err(e) => {
                eprintln!("Failed to parse TAP file: {e}");
                process::exit(1);
            }
        }
    }

    // Enqueue typed text if provided.
    if let Some(ref text) = cli.type_text {
        // Unescape \n to actual newlines.
        let text = text.replace("\\n", "\n");
        spectrum.input_queue().enqueue_text(&text, cli.type_at);
    }

    spectrum
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

    if let Some(ref path) = cli.script_path {
        let mut server = McpServer::new();
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

    let spectrum = make_spectrum(&cli);
    let mut app = App::new(spectrum);

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
