//! BBC Micro Model B emulator binary.
//!
//! Runs the BBC Micro with a winit window and wgpu renderer, or in
//! headless mode for screenshots.

#![allow(clippy::cast_possible_truncation)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu_core::Cpu;
use emu_core::renderer::Renderer;
use emu_bbc_micro::BbcMicro;
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Framebuffer dimensions.
const FB_WIDTH: u32 = 640;
const FB_HEIGHT: u32 = 256;

/// Window scale factor.
const SCALE: u32 = 2;

/// Frame duration for 50 Hz PAL.
const FRAME_DURATION: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// BBC Micro keyboard mapping
// ---------------------------------------------------------------------------

/// Map a host keycode to a BBC Micro keyboard matrix (column, row) pair.
/// The BBC keyboard has 10 columns x 8 rows, active-high (true = pressed).
fn bbc_key_mapping(keycode: KeyCode) -> Option<(usize, usize)> {
    // Simplified mapping covering essential keys.
    // BBC keyboard matrix: column 0-9, row 0-7.
    match keycode {
        // Row 0
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Some((0, 0)),
        KeyCode::KeyQ => Some((1, 0)),
        KeyCode::Digit3 => Some((2, 0)),
        KeyCode::Digit4 => Some((3, 0)),
        KeyCode::KeyE => Some((4, 0)),
        KeyCode::KeyT => Some((5, 0)),
        KeyCode::Digit7 => Some((6, 0)),
        KeyCode::KeyI => Some((7, 0)),
        KeyCode::Digit9 => Some((8, 0)),
        KeyCode::Minus => Some((9, 0)),
        // Row 1
        KeyCode::ControlLeft | KeyCode::ControlRight => Some((0, 1)),
        KeyCode::KeyW => Some((1, 1)),
        KeyCode::KeyR => Some((4, 1)),
        KeyCode::Digit6 => Some((5, 1)),
        KeyCode::Digit8 => Some((6, 1)),
        KeyCode::KeyO => Some((7, 1)),
        KeyCode::Digit0 => Some((8, 1)),
        // Row 2
        KeyCode::KeyA => Some((1, 2)),
        KeyCode::KeyX => Some((2, 2)),
        KeyCode::KeyD => Some((3, 2)),
        KeyCode::KeyU => Some((6, 2)),
        KeyCode::KeyP => Some((7, 2)),
        KeyCode::BracketLeft => Some((8, 2)),
        // Row 3
        KeyCode::CapsLock => Some((0, 3)),
        KeyCode::KeyS => Some((1, 3)),
        KeyCode::KeyC => Some((2, 3)),
        KeyCode::KeyF => Some((3, 3)),
        KeyCode::KeyY => Some((5, 3)),
        KeyCode::KeyJ => Some((6, 3)),
        KeyCode::KeyK => Some((7, 3)),
        KeyCode::Semicolon => Some((8, 3)),
        KeyCode::BracketRight => Some((9, 3)),
        // Row 4
        KeyCode::Tab => Some((0, 4)),
        KeyCode::KeyZ => Some((1, 4)),
        KeyCode::Space => Some((2, 4)),
        KeyCode::KeyV => Some((3, 4)),
        KeyCode::KeyG => Some((4, 4)),
        KeyCode::KeyH => Some((5, 4)),
        KeyCode::KeyN => Some((6, 4)),
        KeyCode::KeyL => Some((7, 4)),
        KeyCode::Quote => Some((8, 4)),
        // Row 5
        KeyCode::Escape => Some((0, 5)),
        KeyCode::Digit1 => Some((1, 5)),
        KeyCode::Digit2 => Some((2, 5)),
        KeyCode::KeyB => Some((3, 5)),
        KeyCode::KeyM => Some((4, 5)),
        KeyCode::Comma => Some((5, 5)),
        KeyCode::Period => Some((6, 5)),
        KeyCode::Slash => Some((7, 5)),
        // Row 6
        KeyCode::Enter => Some((9, 6)),
        KeyCode::Delete | KeyCode::Backspace => Some((9, 7)),
        // Cursor keys
        KeyCode::ArrowUp => Some((3, 7)),
        KeyCode::ArrowDown => Some((4, 7)),
        KeyCode::ArrowLeft => Some((1, 7)),
        KeyCode::ArrowRight => Some((7, 7)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CliArgs {
    mos_path: Option<PathBuf>,
    basic_path: Option<PathBuf>,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
}

fn print_usage() {
    eprintln!("Usage: emu-bbc-micro [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --mos <file>         MOS ROM file (16 KB, required)");
    eprintln!("  --basic <file>       BASIC ROM file (16 KB, optional)");
    eprintln!("  --headless           Run without a window");
    eprintln!("  --frames <n>         Number of frames in headless mode [default: 200]");
    eprintln!("  --screenshot <file>  Save a PNG screenshot (headless)");
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
        mos_path: None,
        basic_path: None,
        headless: false,
        frames: 200,
        screenshot_path: None,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mos" => {
                cli.mos_path = Some(next_option_value(args, &mut i, "--mos")?);
            }
            "--basic" => {
                cli.basic_path = Some(next_option_value(args, &mut i, "--basic")?);
            }
            "--headless" => {
                cli.headless = true;
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
            "--help" | "-h" => return Ok(None),
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
        i += 1;
    }

    if cli.screenshot_path.is_some() {
        cli.headless = true;
    }

    Ok(Some(cli))
}

// ---------------------------------------------------------------------------
// Screenshot
// ---------------------------------------------------------------------------

fn save_screenshot(fb: &[u32], width: u32, height: u32, path: &Path) -> Result<(), Box<dyn Error>> {
    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for &pixel in fb {
        let r = ((pixel >> 16) & 0xFF) as u8;
        let g = ((pixel >> 8) & 0xFF) as u8;
        let b = (pixel & 0xFF) as u8;
        let a = ((pixel >> 24) & 0xFF) as u8;
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    writer.write_image_data(&rgba)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Headless mode
// ---------------------------------------------------------------------------

fn run_headless(cli: &CliArgs) {
    let mut system = make_system(cli);

    for _ in 0..cli.frames {
        system.run_frame();
    }

    if let Some(ref path) = cli.screenshot_path {
        if let Err(e) = save_screenshot(system.framebuffer(), FB_WIDTH, FB_HEIGHT, path) {
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
    screenshot: MenuId,
    quit: MenuId,
    soft_reset: MenuId,
    hard_reset: MenuId,
}

fn build_menu() -> (Menu, MenuIds) {
    let menu = Menu::new();

    let file_menu = Submenu::new("File", true);
    let screenshot = MenuItem::new("Screenshot\tCtrl+P", true, None);
    let quit = MenuItem::new("Quit\tCtrl+Q", true, None);
    file_menu.append(&screenshot).ok();
    file_menu.append(&PredefinedMenuItem::separator()).ok();
    file_menu.append(&quit).ok();

    let system_menu = Submenu::new("System", true);
    let soft_reset = MenuItem::new("Soft Reset\tCtrl+R", true, None);
    let hard_reset = MenuItem::new("Hard Reset\tCtrl+Shift+R", true, None);
    system_menu.append(&soft_reset).ok();
    system_menu.append(&hard_reset).ok();

    menu.append(&file_menu).ok();
    menu.append(&system_menu).ok();

    let ids = MenuIds {
        screenshot: screenshot.id().clone(),
        quit: quit.id().clone(),
        soft_reset: soft_reset.id().clone(),
        hard_reset: hard_reset.id().clone(),
    };

    (menu, ids)
}

// ---------------------------------------------------------------------------
// Windowed mode (winit + wgpu + muda)
// ---------------------------------------------------------------------------

struct App {
    system: BbcMicro,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    last_frame_time: Instant,
    menu_ids: MenuIds,
    _menu: Menu,
}

impl App {
    fn new(system: BbcMicro, menu: Menu, menu_ids: MenuIds) -> Self {
        Self {
            system,
            window: None,
            renderer: None,
            last_frame_time: Instant::now(),
            menu_ids,
            _menu: menu,
        }
    }

    fn handle_key(&mut self, keycode: KeyCode, pressed: bool) {
        if let Some((col, row)) = bbc_key_mapping(keycode) {
            if pressed {
                self.system.press_key(col, row);
            } else {
                self.system.release_key(col, row);
            }
        }
    }

    fn handle_menu_event(&mut self, id: &MenuId, event_loop: &ActiveEventLoop) {
        if *id == self.menu_ids.quit {
            event_loop.exit();
        } else if *id == self.menu_ids.soft_reset {
            self.system.cpu_mut().reset();
            eprintln!("Soft reset");
        } else if *id == self.menu_ids.hard_reset {
            self.system.cpu_mut().reset();
            eprintln!("Hard reset");
        } else if *id == self.menu_ids.screenshot {
            let path = PathBuf::from("screenshot.png");
            if let Err(e) = save_screenshot(self.system.framebuffer(), FB_WIDTH, FB_HEIGHT, &path) {
                eprintln!("Screenshot error: {e}");
            } else {
                eprintln!("Screenshot saved to {}", path.display());
            }
        }
    }
}

#[allow(clippy::used_underscore_binding)]
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_size = winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, FB_HEIGHT * SCALE);
        let attrs = WindowAttributes::default()
            .with_title("BBC Micro Model B")
            .with_inner_size(window_size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);

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
                    if keycode == KeyCode::F12 && event.state == ElementState::Pressed {
                        event_loop.exit();
                        return;
                    }
                    self.handle_key(keycode, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.system.run_frame();

                    if let Some(renderer) = &mut self.renderer {
                        renderer.upload_framebuffer(self.system.framebuffer());
                    }

                    self.last_frame_time = now;
                }

                if let Some(renderer) = &self.renderer
                    && let Err(e) = renderer.render()
                {
                    eprintln!("Render error: {e}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
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

fn make_system(cli: &CliArgs) -> BbcMicro {
    let mos_path = match cli.mos_path.as_ref() {
        Some(p) => p,
        None => {
            eprintln!("No MOS ROM file specified. Use --mos <file>");
            process::exit(1);
        }
    };

    let mos_data = match std::fs::read(mos_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to read MOS ROM file {}: {e}", mos_path.display());
            process::exit(1);
        }
    };

    eprintln!("Loaded MOS ROM: {}", mos_path.display());

    let mut system = BbcMicro::new(mos_data);

    if let Some(ref basic_path) = cli.basic_path {
        let basic_data = match std::fs::read(basic_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read BASIC ROM file {}: {e}", basic_path.display());
                process::exit(1);
            }
        };
        eprintln!("Loaded BASIC ROM: {}", basic_path.display());
        // BASIC lives in sideways ROM bank 15 on the BBC Micro.
        system.insert_rom(15, basic_data);
    }

    system
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

    let system = make_system(&cli);
    let (menu, menu_ids) = build_menu();
    let mut app = App::new(system, menu, menu_ids);

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
