//! Commodore 64 emulator binary.
//!
//! Runs the C64 with a winit window and wgpu renderer, or in
//! headless mode for screenshots, or as an MCP server.

#![allow(clippy::cast_possible_truncation)]

use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu_c64::config::SidModel;
use emu_c64::mcp::{C64Mcp, McpServer};
use emu_c64::{C64, C64Config, C64Model, capture, keyboard_map};
use emu_core::Cpu;
use emu_core::renderer::Renderer;
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Window scale factor.
const SCALE: u32 = 3;

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct CliArgs {
    model: String,
    sid_model: String,
    reu_size: Option<u32>,
    prg_path: Option<PathBuf>,
    bas_path: Option<PathBuf>,
    d64_path: Option<PathBuf>,
    drive_rom_path: Option<PathBuf>,
    headless: bool,
    mcp: bool,
    script_path: Option<PathBuf>,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    record_dir: Option<PathBuf>,
    type_text: Option<String>,
    type_at: u64,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        model: "pal".to_string(),
        sid_model: "6581".to_string(),
        reu_size: None,
        prg_path: None,
        bas_path: None,
        d64_path: None,
        drive_rom_path: None,
        headless: false,
        mcp: false,
        script_path: None,
        frames: 200,
        screenshot_path: None,
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
            "--sid" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.sid_model.clone_from(s);
                }
            }
            "--reu" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.reu_size = s.parse().ok();
                }
            }
            "--prg" => {
                i += 1;
                cli.prg_path = args.get(i).map(PathBuf::from);
            }
            "--bas" => {
                i += 1;
                cli.bas_path = args.get(i).map(PathBuf::from);
            }
            "--d64" => {
                i += 1;
                cli.d64_path = args.get(i).map(PathBuf::from);
            }
            "--drive-rom" => {
                i += 1;
                cli.drive_rom_path = args.get(i).map(PathBuf::from);
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
                eprintln!("  --model <pal|ntsc>   C64 model variant [default: pal]");
                eprintln!("  --sid <6581|8580>    SID chip revision [default: 6581]");
                eprintln!("  --reu <128|256|512>  Enable REU with given KB");
                eprintln!("  --prg <file>         Load a PRG file into memory");
                eprintln!("  --d64 <file>         Insert a D64 disk image");
                eprintln!("  --drive-rom <file>   Load 1541 drive ROM (16384 bytes)");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --mcp                Run as MCP server (JSON-RPC over stdio)");
                eprintln!("  --script <file>      Run a JSON script file (headless batch mode)");
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
        let _ = c64.take_audio_buffer();
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
// Native menus (muda)
// ---------------------------------------------------------------------------

struct MenuIds {
    soft_reset: MenuId,
    hard_reset: MenuId,
    screenshot: MenuId,
    quit: MenuId,
    model_pal: MenuId,
    model_ntsc: MenuId,
    sid_6581: MenuId,
    sid_8580: MenuId,
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
    let model_pal = MenuItem::new("PAL (6569)", true, None);
    let model_ntsc = MenuItem::new("NTSC (6567)", true, None);
    model_menu.append(&model_pal).ok();
    model_menu.append(&model_ntsc).ok();

    // SID submenu.
    let sid_menu = Submenu::new("SID Chip", true);
    let sid_6581 = MenuItem::new("MOS 6581", true, None);
    let sid_8580 = MenuItem::new("MOS 8580", true, None);
    sid_menu.append(&sid_6581).ok();
    sid_menu.append(&sid_8580).ok();

    system_menu.append(&PredefinedMenuItem::separator()).ok();
    system_menu.append(&model_menu).ok();
    system_menu.append(&sid_menu).ok();

    menu.append(&file_menu).ok();
    menu.append(&system_menu).ok();

    let ids = MenuIds {
        soft_reset: soft_reset.id().clone(),
        hard_reset: hard_reset.id().clone(),
        screenshot: screenshot.id().clone(),
        quit: quit.id().clone(),
        model_pal: model_pal.id().clone(),
        model_ntsc: model_ntsc.id().clone(),
        sid_6581: sid_6581.id().clone(),
        sid_8580: sid_8580.id().clone(),
    };

    (menu, ids)
}

// ---------------------------------------------------------------------------
// Windowed mode (winit + wgpu + muda)
// ---------------------------------------------------------------------------

struct App {
    c64: C64,
    config: C64Config,
    d64_data: Option<Vec<u8>>,
    prg_data: Option<Vec<u8>>,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    last_frame_time: Instant,
    frame_duration: Duration,
    fb_width: u32,
    fb_height: u32,
    menu_ids: MenuIds,
    _menu: Menu,
}

impl App {
    fn new(c64: C64, config: C64Config, menu: Menu, menu_ids: MenuIds) -> Self {
        let fb_width = c64.framebuffer_width();
        let fb_height = c64.framebuffer_height();
        let frame_duration = if config.model == C64Model::C64Ntsc {
            Duration::from_micros(16_667)
        } else {
            Duration::from_micros(19_950)
        };
        Self {
            c64,
            config,
            d64_data: None,
            prg_data: None,
            renderer: None,
            window: None,
            last_frame_time: Instant::now(),
            frame_duration,
            fb_width,
            fb_height,
            menu_ids,
            _menu: menu,
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

    fn rebuild_c64(&mut self) {
        let mut c64 = C64::new(&self.config);

        // Reload media.
        if let Some(ref data) = self.d64_data {
            if let Err(e) = c64.load_d64(data) {
                eprintln!("Failed to reload D64: {e}");
            }
        }
        if let Some(ref data) = self.prg_data {
            if let Err(e) = c64.load_prg(data) {
                eprintln!("Failed to reload PRG: {e}");
            }
        }

        self.c64 = c64;

        // Update frame duration.
        self.frame_duration = if self.config.model == C64Model::C64Ntsc {
            Duration::from_micros(16_667)
        } else {
            Duration::from_micros(19_950)
        };

        // Rebuild renderer if framebuffer size changed.
        let new_width = self.c64.framebuffer_width();
        let new_height = self.c64.framebuffer_height();
        if new_width != self.fb_width || new_height != self.fb_height {
            self.fb_width = new_width;
            self.fb_height = new_height;
            if let Some(window) = &self.window {
                let size = winit::dpi::LogicalSize::new(
                    self.fb_width * SCALE,
                    self.fb_height * SCALE,
                );
                if let Some(sz) = window.request_inner_size(size) {
                    let _ = sz;
                }
                self.renderer = Some(Renderer::new(
                    window.clone(),
                    self.fb_width,
                    self.fb_height,
                    emu_core::renderer::FilterMode::Nearest,
                ));
            }
        }

        // Update window title.
        let title = c64_title(self.config.model, self.config.sid_model);
        if let Some(window) = &self.window {
            window.set_title(&title);
        }
        eprintln!("Switched to {title}");
    }

    fn switch_model(&mut self, model: C64Model) {
        if model == self.config.model {
            return;
        }
        self.config.model = model;
        self.rebuild_c64();
    }

    fn switch_sid(&mut self, sid_model: SidModel) {
        if sid_model == self.config.sid_model {
            return;
        }
        self.config.sid_model = sid_model;
        self.rebuild_c64();
    }

    fn handle_menu_event(&mut self, id: &MenuId, event_loop: &ActiveEventLoop) {
        if *id == self.menu_ids.quit {
            event_loop.exit();
        } else if *id == self.menu_ids.soft_reset {
            self.c64.cpu_mut().reset();
            eprintln!("Soft reset");
        } else if *id == self.menu_ids.hard_reset {
            self.c64.cpu_mut().reset();
            eprintln!("Hard reset");
        } else if *id == self.menu_ids.screenshot {
            let path = std::path::PathBuf::from("screenshot.png");
            match capture::save_screenshot(&self.c64, &path) {
                Ok(()) => eprintln!("Screenshot saved to {}", path.display()),
                Err(e) => eprintln!("Screenshot error: {e}"),
            }
        } else if *id == self.menu_ids.model_pal {
            self.switch_model(C64Model::C64Pal);
        } else if *id == self.menu_ids.model_ntsc {
            self.switch_model(C64Model::C64Ntsc);
        } else if *id == self.menu_ids.sid_6581 {
            self.switch_sid(SidModel::Sid6581);
        } else if *id == self.menu_ids.sid_8580 {
            self.switch_sid(SidModel::Sid8580);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_size =
            winit::dpi::LogicalSize::new(self.fb_width * SCALE, self.fb_height * SCALE);
        let attrs = WindowAttributes::default()
            .with_title(c64_title(self.config.model, self.config.sid_model))
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
                        if let winit::raw_window_handle::RawWindowHandle::Win32(h) =
                            handle.as_raw()
                        {
                            unsafe {
                                self._menu
                                    .init_for_hwnd(h.hwnd.get() as _)
                                    .ok();
                            }
                        }
                    }
                }

                let renderer =
                    Renderer::new(window.clone(), self.fb_width, self.fb_height, emu_core::renderer::FilterMode::Nearest);
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
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= self.frame_duration {
                    self.c64.run_frame();
                    // Drain SID audio buffer (prevent unbounded growth).
                    // Future: feed to audio output device.
                    let _ = self.c64.take_audio_buffer();

                    if let Some(renderer) = &mut self.renderer {
                        renderer.upload_framebuffer(self.c64.framebuffer());
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

fn c64_title(model: C64Model, sid: SidModel) -> String {
    let region = match model {
        C64Model::C64Pal => "PAL",
        C64Model::C64Ntsc => "NTSC",
    };
    let sid_name = match sid {
        SidModel::Sid6581 => "6581",
        SidModel::Sid8580 => "8580",
    };
    format!("Commodore 64 ({region}, SID {sid_name})")
}

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

fn load_c64_config(cli: &CliArgs) -> C64Config {
    let roms_dir = find_roms_dir();

    let model = match cli.model.as_str() {
        "ntsc" => C64Model::C64Ntsc,
        _ => C64Model::C64Pal,
    };

    let sid_model = match cli.sid_model.as_str() {
        "8580" => SidModel::Sid8580,
        _ => SidModel::Sid6581,
    };

    // Load 1541 drive ROM if explicitly specified, or auto-detect from roms/
    let drive_rom = if let Some(ref path) = cli.drive_rom_path {
        Some(load_rom(path, "1541 Drive", 16384))
    } else {
        let auto_path = roms_dir.join("1541.rom");
        if auto_path.is_file() {
            Some(load_rom(&auto_path, "1541 Drive", 16384))
        } else {
            None
        }
    };

    C64Config {
        model,
        sid_model,
        kernal_rom: load_rom(&roms_dir.join("kernal.rom"), "Kernal", 8192),
        basic_rom: load_rom(&roms_dir.join("basic.rom"), "BASIC", 8192),
        char_rom: load_rom(&roms_dir.join("chargen.rom"), "Character", 4096),
        drive_rom,
        reu_size: cli.reu_size,
    }
}

fn make_c64(cli: &CliArgs) -> C64 {
    let config = load_c64_config(cli);
    make_c64_from_config(&config, cli)
}

fn make_c64_from_config(config: &C64Config, cli: &CliArgs) -> C64 {
    let mut c64 = C64::new(config);

    // Load D64 disk image if specified
    if let Some(ref path) = cli.d64_path {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read D64 file {}: {e}", path.display());
                process::exit(1);
            }
        };
        match c64.load_d64(&data) {
            Ok(()) => eprintln!("Inserted D64: {}", path.display()),
            Err(e) => {
                eprintln!("Failed to load D64: {e}");
                process::exit(1);
            }
        }
    }

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

    if let Some(ref path) = cli.bas_path {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read BAS file {}: {e}", path.display());
                process::exit(1);
            }
        };
        let program = match format_c64_bas::tokenise(&source) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to tokenise BASIC: {e}");
                process::exit(1);
            }
        };
        match c64.load_prg(&program.bytes) {
            Ok(addr) => eprintln!("Loaded BAS as PRG at ${addr:04X}: {}", path.display()),
            Err(e) => {
                eprintln!("Failed to load tokenised BASIC: {e}");
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
        let mut server = McpServer::new(C64Mcp::new());
        server.run();
        return;
    }

    if let Some(ref path) = cli.script_path {
        let mut server = McpServer::new(C64Mcp::new());
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

    let config = load_c64_config(&cli);
    let c64 = make_c64_from_config(&config, &cli);

    // Cache media data for model switching.
    let d64_data = cli.d64_path.as_ref().and_then(|p| std::fs::read(p).ok());
    let prg_data = cli.prg_path.as_ref().and_then(|p| std::fs::read(p).ok());

    let (menu, menu_ids) = build_menu();
    let mut app = App::new(c64, config, menu, menu_ids);
    app.d64_data = d64_data;
    app.prg_data = prg_data;

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
