//! ZX Spectrum 48K emulator binary.
//!
//! Runs the Spectrum with a winit window and wgpu renderer, or in
//! headless mode for screenshots and audio capture.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu_core::Cpu;
use emu_core::renderer::Renderer;
use emu_spectrum::keyboard_map::MappedKey;
use emu_spectrum::mcp::{McpServer, SpectrumMcp};
use emu_spectrum::{
    Spectrum, SpectrumConfig, SpectrumModel, TapFile, TzxFile, capture, keyboard_map, load_sna,
    load_z80,
};
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Embedded ROMs — compiled into the binary.
const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");
const ROM_128K: &[u8] = include_bytes!("../../../roms/128.rom");
const ROM_PLUS2: &[u8] = include_bytes!("../../../roms/plus2.rom");
const ROM_PLUS3: &[u8] = include_bytes!("../../../roms/plus3.rom");

/// Spectrum framebuffer dimensions (4:3).
const FB_WIDTH: u32 = 384;
const FB_HEIGHT: u32 = 288;

/// Window scale factor.
const SCALE: u32 = 3;

/// Frame duration for 50 Hz PAL.
const FRAME_DURATION: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct CliArgs {
    model: String,
    rom_path: Option<PathBuf>,
    sna_path: Option<PathBuf>,
    z80_path: Option<PathBuf>,
    tap_path: Option<PathBuf>,
    bas_path: Option<PathBuf>,
    tzx_path: Option<PathBuf>,
    dsk_path: Option<PathBuf>,
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
        model: "48k".to_string(),
        rom_path: None,
        sna_path: None,
        z80_path: None,
        tap_path: None,
        bas_path: None,
        tzx_path: None,
        dsk_path: None,
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
            "--model" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.model = s.to_lowercase();
                }
            }
            "--rom" => {
                i += 1;
                cli.rom_path = args.get(i).map(PathBuf::from);
            }
            "--sna" => {
                i += 1;
                cli.sna_path = args.get(i).map(PathBuf::from);
            }
            "--z80" => {
                i += 1;
                cli.z80_path = args.get(i).map(PathBuf::from);
            }
            "--tap" => {
                i += 1;
                cli.tap_path = args.get(i).map(PathBuf::from);
            }
            "--bas" => {
                i += 1;
                cli.bas_path = args.get(i).map(PathBuf::from);
            }
            "--tzx" => {
                i += 1;
                cli.tzx_path = args.get(i).map(PathBuf::from);
            }
            "--dsk" => {
                i += 1;
                cli.dsk_path = args.get(i).map(PathBuf::from);
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
                eprintln!(
                    "  --model <model>      Spectrum model: 48k, 128k, plus2, plus2a, plus3 [default: 48k]"
                );
                eprintln!("  --rom <file>         ROM file (overrides built-in ROM)");
                eprintln!("  --sna <file>         Load a SNA snapshot (48K or 128K)");
                eprintln!("  --z80 <file>         Load a .Z80 snapshot (v1/v2/v3)");
                eprintln!("  --tap <file>         Insert a TAP file into the tape deck");
                eprintln!("  --tzx <file>         Insert a TZX file (real-time tape signal)");
                eprintln!("  --dsk <file>         Insert a DSK disk image (+3 only)");
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
    let mut all_audio: Vec<[f32; 2]> = Vec::new();
    for _ in 0..cli.frames {
        spectrum.run_frame();
        all_audio.extend_from_slice(&spectrum.take_audio_buffer());
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
// Native menus (muda)
// ---------------------------------------------------------------------------

struct MenuIds {
    soft_reset: MenuId,
    hard_reset: MenuId,
    screenshot: MenuId,
    quit: MenuId,
    model_48k: MenuId,
    model_128k: MenuId,
    model_plus2: MenuId,
    model_plus2a: MenuId,
    model_plus3: MenuId,
}

fn build_menu() -> (Menu, MenuIds) {
    let menu = Menu::new();

    // File menu.
    let file_menu = Submenu::new("File", true);
    let screenshot = MenuItem::new("Screenshot\tCtrl+P", true, None);
    let quit = MenuItem::new("Quit\tCtrl+Q", true, None);
    file_menu.append(&screenshot).ok();
    file_menu.append(&PredefinedMenuItem::separator()).ok();
    file_menu.append(&quit).ok();

    // System menu.
    let system_menu = Submenu::new("System", true);
    let soft_reset = MenuItem::new("Soft Reset\tCtrl+R", true, None);
    let hard_reset = MenuItem::new("Hard Reset\tCtrl+Shift+R", true, None);
    system_menu.append(&soft_reset).ok();
    system_menu.append(&hard_reset).ok();

    // Model submenu.
    let model_menu = Submenu::new("Model", true);
    let model_48k = MenuItem::new("Spectrum 48K", true, None);
    let model_128k = MenuItem::new("Spectrum 128K", true, None);
    let model_plus2 = MenuItem::new("Spectrum +2", true, None);
    let model_plus2a = MenuItem::new("Spectrum +2A", true, None);
    let model_plus3 = MenuItem::new("Spectrum +3", true, None);
    model_menu.append(&model_48k).ok();
    model_menu.append(&model_128k).ok();
    model_menu.append(&PredefinedMenuItem::separator()).ok();
    model_menu.append(&model_plus2).ok();
    model_menu.append(&model_plus2a).ok();
    model_menu.append(&model_plus3).ok();

    system_menu.append(&PredefinedMenuItem::separator()).ok();
    system_menu.append(&model_menu).ok();

    menu.append(&file_menu).ok();
    menu.append(&system_menu).ok();

    let ids = MenuIds {
        soft_reset: soft_reset.id().clone(),
        hard_reset: hard_reset.id().clone(),
        screenshot: screenshot.id().clone(),
        quit: quit.id().clone(),
        model_48k: model_48k.id().clone(),
        model_128k: model_128k.id().clone(),
        model_plus2: model_plus2.id().clone(),
        model_plus2a: model_plus2a.id().clone(),
        model_plus3: model_plus3.id().clone(),
    };

    (menu, ids)
}

// ---------------------------------------------------------------------------
// Windowed mode (winit + wgpu + muda)
// ---------------------------------------------------------------------------

struct App {
    spectrum: Spectrum,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    last_frame_time: Instant,
    title: String,
    menu_ids: MenuIds,
    _menu: Menu,
}

impl App {
    fn new(spectrum: Spectrum, title: String, menu: Menu, menu_ids: MenuIds) -> Self {
        Self {
            spectrum,
            renderer: None,
            window: None,
            last_frame_time: Instant::now(),
            title,
            menu_ids,
            _menu: menu,
        }
    }

    fn handle_key(&mut self, keycode: KeyCode, pressed: bool) {
        if let Some(mapped) = keyboard_map::map_keycode(keycode) {
            let keys: &[_] = match &mapped {
                MappedKey::Single(k) => std::slice::from_ref(k),
                MappedKey::Combo(pair) => pair,
            };
            for &key in keys {
                if pressed {
                    self.spectrum.press_key(key);
                } else {
                    self.spectrum.release_key(key);
                }
            }
        }
    }

    fn switch_model(&mut self, model: SpectrumModel) {
        let rom = embedded_rom(model).to_vec();
        let config = SpectrumConfig { model, rom };
        self.spectrum = Spectrum::new(&config);
        let title = match model {
            SpectrumModel::Spectrum48K => "ZX Spectrum 48K",
            SpectrumModel::Spectrum128K => "ZX Spectrum 128K",
            SpectrumModel::SpectrumPlus2 => "ZX Spectrum +2",
            SpectrumModel::SpectrumPlus2A => "ZX Spectrum +2A",
            SpectrumModel::SpectrumPlus3 => "ZX Spectrum +3",
            _ => "ZX Spectrum",
        };
        if let Some(window) = &self.window {
            window.set_title(title);
        }
        eprintln!("Switched to {title}");
    }

    fn handle_menu_event(&mut self, id: &MenuId, event_loop: &ActiveEventLoop) {
        if *id == self.menu_ids.quit {
            event_loop.exit();
        } else if *id == self.menu_ids.soft_reset {
            self.spectrum.cpu_mut().reset();
            eprintln!("Soft reset");
        } else if *id == self.menu_ids.hard_reset {
            self.spectrum.cpu_mut().reset();
            eprintln!("Hard reset");
        } else if *id == self.menu_ids.screenshot {
            let path = std::path::PathBuf::from("screenshot.png");
            match capture::save_screenshot(&self.spectrum, &path) {
                Ok(()) => eprintln!("Screenshot saved to {}", path.display()),
                Err(e) => eprintln!("Screenshot error: {e}"),
            }
        } else if *id == self.menu_ids.model_48k {
            self.switch_model(SpectrumModel::Spectrum48K);
        } else if *id == self.menu_ids.model_128k {
            self.switch_model(SpectrumModel::Spectrum128K);
        } else if *id == self.menu_ids.model_plus2 {
            self.switch_model(SpectrumModel::SpectrumPlus2);
        } else if *id == self.menu_ids.model_plus2a {
            self.switch_model(SpectrumModel::SpectrumPlus2A);
        } else if *id == self.menu_ids.model_plus3 {
            self.switch_model(SpectrumModel::SpectrumPlus3);
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
            .with_title(&self.title)
            .with_inner_size(window_size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);

                // Attach native menu.
                #[cfg(target_os = "macos")]
                {
                    self._menu.init_for_nsapp();
                }
                #[cfg(target_os = "windows")]
                {
                    use winit::raw_window_handle::HasWindowHandle;
                    if let Ok(handle) = window.window_handle() {
                        if let winit::raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw()
                        {
                            unsafe {
                                self._menu
                                    .init_for_hwnd(h.hwnd.get() as _)
                                    .ok();
                            }
                        }
                    }
                }

                let renderer = Renderer::new(window.clone(), FB_WIDTH, FB_HEIGHT, emu_core::renderer::FilterMode::Nearest);
                self.renderer = Some(renderer);
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
                // Throttle to ~50 Hz.
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.spectrum.run_frame();
                    let _ = self.spectrum.take_audio_buffer();

                    if let Some(renderer) = &mut self.renderer {
                        renderer.upload_framebuffer(self.spectrum.framebuffer());
                    }

                    self.last_frame_time = now;
                }

                if let Some(renderer) = &self.renderer {
                    if let Err(e) = renderer.render() {
                        eprintln!("Render error: {e}");
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Process menu events.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle_menu_event(event.id(), event_loop);
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn embedded_rom(model: SpectrumModel) -> &'static [u8] {
    match model {
        SpectrumModel::Spectrum48K => ROM_48K,
        SpectrumModel::Spectrum128K => ROM_128K,
        SpectrumModel::SpectrumPlus2 => ROM_PLUS2,
        SpectrumModel::SpectrumPlus2A | SpectrumModel::SpectrumPlus3 => ROM_PLUS3,
        _ => ROM_48K, // Unsupported models fall back to 48K
    }
}

fn make_spectrum(cli: &CliArgs) -> Spectrum {
    let model = match cli.model.as_str() {
        "48k" | "48" => SpectrumModel::Spectrum48K,
        "128k" | "128" => SpectrumModel::Spectrum128K,
        "plus2" | "+2" => SpectrumModel::SpectrumPlus2,
        "plus2a" | "+2a" => SpectrumModel::SpectrumPlus2A,
        "plus3" | "+3" => SpectrumModel::SpectrumPlus3,
        other => {
            eprintln!("Unknown model: {other}. Use 48k, 128k, plus2, plus2a, or plus3.");
            process::exit(1);
        }
    };
    // Use --rom override if provided, otherwise use embedded ROM.
    let rom = if let Some(ref path) = cli.rom_path {
        match std::fs::read(path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read ROM file {}: {e}", path.display());
                process::exit(1);
            }
        }
    } else {
        embedded_rom(model).to_vec()
    };

    let config = SpectrumConfig { model, rom };
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

    // Load .Z80 snapshot if provided.
    if let Some(ref path) = cli.z80_path {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read Z80 file {}: {e}", path.display());
                process::exit(1);
            }
        };
        if let Err(e) = load_z80(&mut spectrum, &data) {
            eprintln!("Failed to load Z80: {e}");
            process::exit(1);
        }
        eprintln!("Loaded Z80: {}", path.display());
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

    // Tokenise and insert BAS file if provided.
    if let Some(ref path) = cli.bas_path {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read BAS file {}: {e}", path.display());
                process::exit(1);
            }
        };
        let program = match format_spectrum_bas::tokenise(&source) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to tokenise BASIC: {e}");
                process::exit(1);
            }
        };
        let data_len = program.bytes.len() as u16;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("PROGRAM");
        let header =
            format_spectrum_tap::TapBlock::program_header(name, data_len, Some(1), data_len);
        let data = format_spectrum_tap::TapBlock::data(program.bytes);
        let tap = TapFile {
            blocks: vec![header, data],
        };
        eprintln!(
            "Inserted BAS as TAP: {} ({} blocks)",
            path.display(),
            tap.blocks.len()
        );
        spectrum.insert_tap(tap);
    }

    // Insert TZX file if provided.
    if let Some(ref path) = cli.tzx_path {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read TZX file {}: {e}", path.display());
                process::exit(1);
            }
        };
        match TzxFile::parse(&data) {
            Ok(tzx) => {
                eprintln!(
                    "Inserted TZX: {} ({} blocks)",
                    path.display(),
                    tzx.blocks.len()
                );
                spectrum.insert_tzx(tzx);
            }
            Err(e) => {
                eprintln!("Failed to parse TZX file: {e}");
                process::exit(1);
            }
        }
    }

    // Insert DSK if provided.
    if let Some(ref path) = cli.dsk_path {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read DSK file {}: {e}", path.display());
                process::exit(1);
            }
        };
        if let Err(e) = spectrum.load_dsk(&data) {
            eprintln!("Failed to load DSK: {e}");
            process::exit(1);
        }
        eprintln!("Inserted DSK: {}", path.display());
    }

    // Enqueue typed text if provided.
    if let Some(ref text) = cli.type_text {
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
        let mut server = McpServer::new(SpectrumMcp::new());
        server.run();
        return;
    }

    if let Some(ref path) = cli.script_path {
        let mut server = McpServer::new(SpectrumMcp::new());
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

    let title = match cli.model.as_str() {
        "128k" | "128" => "ZX Spectrum 128K",
        "plus2" | "+2" => "ZX Spectrum +2",
        "plus2a" | "+2a" => "ZX Spectrum +2A",
        "plus3" | "+3" => "ZX Spectrum +3",
        _ => "ZX Spectrum 48K",
    };

    let (menu, menu_ids) = build_menu();
    let spectrum = make_spectrum(&cli);
    let mut app = App::new(spectrum, title.to_string(), menu, menu_ids);

    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("Failed to create event loop: {e}");
            process::exit(1);
        }
    };

    // Poll menu events in the event loop.
    let menu_channel = MenuEvent::receiver().clone();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {e}");
        process::exit(1);
    }

    // Menu events are checked in about_to_wait, but we keep the receiver alive here.
    drop(menu_channel);
}
