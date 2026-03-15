//! MSX1 emulator binary.
//!
//! Runs the MSX with a winit window and wgpu renderer, or in
//! headless mode for screenshots.

#![allow(clippy::cast_possible_truncation)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu_core::Cpu;
use emu_core::renderer::Renderer;
use emu_msx::{MapperType, Msx, MsxRegion};
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Framebuffer dimensions.
const FB_WIDTH: u32 = 256;
const FB_HEIGHT: u32 = 192;

/// Window scale factor.
const SCALE: u32 = 3;

/// Frame duration for ~60 Hz NTSC.
const FRAME_DURATION_NTSC: Duration = Duration::from_micros(16_639);
/// Frame duration for ~50 Hz PAL.
const FRAME_DURATION_PAL: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// MSX keyboard matrix mapping
// ---------------------------------------------------------------------------

/// Map a host keycode to an MSX keyboard matrix (row, bit) pair.
fn msx_key_mapping(keycode: KeyCode) -> Option<(usize, u8)> {
    // MSX1 keyboard matrix: 11 rows x 8 columns.
    // Row 0: 0-7, Row 1: 8-), Row 2: -/./,/`/'/[/]/backslash
    // Row 3: unassigned, Row 4: unassigned, Row 5: unassigned
    // Row 6: F1-F5/ESC/TAB/STOP, Row 7: cursor/space/home/ins/del
    // Row 8: shift/ctrl/graph/caps/code, Row 9-10: unused on MSX1
    //
    // Basic mapping for arrows, space, return, and alphanumeric keys.
    match keycode {
        // Row 0: 0 1 2 3 4 5 6 7
        KeyCode::Digit0 => Some((0, 0)),
        KeyCode::Digit1 => Some((0, 1)),
        KeyCode::Digit2 => Some((0, 2)),
        KeyCode::Digit3 => Some((0, 3)),
        KeyCode::Digit4 => Some((0, 4)),
        KeyCode::Digit5 => Some((0, 5)),
        KeyCode::Digit6 => Some((0, 6)),
        KeyCode::Digit7 => Some((0, 7)),
        // Row 1: 8 9 - = \ [ ] ; ' `
        KeyCode::Digit8 => Some((1, 0)),
        KeyCode::Digit9 => Some((1, 1)),
        KeyCode::Minus => Some((1, 2)),
        KeyCode::Equal => Some((1, 3)),
        KeyCode::Backslash => Some((1, 4)),
        KeyCode::BracketLeft => Some((1, 5)),
        KeyCode::BracketRight => Some((1, 6)),
        KeyCode::Semicolon => Some((1, 7)),
        // Row 2: ' ` , . / dead dead dead
        KeyCode::Quote => Some((2, 0)),
        KeyCode::Backquote => Some((2, 1)),
        KeyCode::Comma => Some((2, 2)),
        KeyCode::Period => Some((2, 3)),
        KeyCode::Slash => Some((2, 4)),
        // Row 3: A-H
        KeyCode::KeyA => Some((3, 0)),
        KeyCode::KeyB => Some((3, 1)),
        KeyCode::KeyC => Some((3, 2)),
        KeyCode::KeyD => Some((3, 3)),
        KeyCode::KeyE => Some((3, 4)),
        KeyCode::KeyF => Some((3, 5)),
        KeyCode::KeyG => Some((3, 6)),
        KeyCode::KeyH => Some((3, 7)),
        // Row 4: I-P
        KeyCode::KeyI => Some((4, 0)),
        KeyCode::KeyJ => Some((4, 1)),
        KeyCode::KeyK => Some((4, 2)),
        KeyCode::KeyL => Some((4, 3)),
        KeyCode::KeyM => Some((4, 4)),
        KeyCode::KeyN => Some((4, 5)),
        KeyCode::KeyO => Some((4, 6)),
        KeyCode::KeyP => Some((4, 7)),
        // Row 5: Q-X
        KeyCode::KeyQ => Some((5, 0)),
        KeyCode::KeyR => Some((5, 1)),
        KeyCode::KeyS => Some((5, 2)),
        KeyCode::KeyT => Some((5, 3)),
        KeyCode::KeyU => Some((5, 4)),
        KeyCode::KeyV => Some((5, 5)),
        KeyCode::KeyW => Some((5, 6)),
        KeyCode::KeyX => Some((5, 7)),
        // Row 6: Y Z
        KeyCode::KeyY => Some((6, 0)),
        KeyCode::KeyZ => Some((6, 1)),
        // Row 6 continued: F1-F5, ESC, TAB, STOP(F8)
        KeyCode::F1 => Some((6, 5)),
        KeyCode::F2 => Some((6, 6)),
        KeyCode::F3 => Some((6, 7)),
        // Row 7: F4, F5, ESC, TAB, STOP
        KeyCode::F4 => Some((7, 0)),
        KeyCode::F5 => Some((7, 1)),
        KeyCode::Escape => Some((7, 2)),
        KeyCode::Tab => Some((7, 3)),
        // Row 8: Space, Home, Ins, Del, cursor keys, Return, Backspace
        KeyCode::Space => Some((8, 0)),
        KeyCode::Home => Some((8, 1)),
        KeyCode::Insert => Some((8, 2)),
        KeyCode::Delete => Some((8, 3)),
        KeyCode::ArrowLeft => Some((8, 4)),
        KeyCode::ArrowUp => Some((8, 5)),
        KeyCode::ArrowDown => Some((8, 6)),
        KeyCode::ArrowRight => Some((8, 7)),
        // Row 9: Return, Select(F6), Backspace
        KeyCode::Enter => Some((9, 0)),
        KeyCode::F6 => Some((9, 1)),
        KeyCode::Backspace => Some((9, 2)),
        // Row 10: Shift, Ctrl
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Some((10, 0)),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some((10, 1)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CliArgs {
    bios_path: Option<PathBuf>,
    cart_path: Option<PathBuf>,
    mapper: MapperType,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    region: MsxRegion,
}

fn print_usage() {
    eprintln!("Usage: emu-msx [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --bios <file>                  MSX BIOS ROM (32 KB, required)");
    eprintln!("  --cart <file>                  Cartridge ROM file");
    eprintln!("  --mapper <type>                Cartridge mapper (plain|konami|konamiscc|ascii8|ascii16)");
    eprintln!("  --region <ntsc|pal>            Video region (default: ntsc)");
    eprintln!("  --headless                     Run without a window");
    eprintln!("  --frames <n>                   Number of frames in headless mode [default: 200]");
    eprintln!("  --screenshot <file>            Save a PNG screenshot (headless)");
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

fn next_option_str<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .filter(|value| !value.starts_with("--"))
        .map(|s| s.as_str())
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_args_from(args: &[String]) -> Result<Option<CliArgs>, String> {
    let mut cli = CliArgs {
        bios_path: None,
        cart_path: None,
        mapper: MapperType::Plain,
        headless: false,
        frames: 200,
        screenshot_path: None,
        region: MsxRegion::Ntsc,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bios" => {
                cli.bios_path = Some(next_option_value(args, &mut i, "--bios")?);
            }
            "--cart" => {
                cli.cart_path = Some(next_option_value(args, &mut i, "--cart")?);
            }
            "--mapper" => {
                let value = next_option_str(args, &mut i, "--mapper")?;
                cli.mapper = match value.to_lowercase().as_str() {
                    "plain" => MapperType::Plain,
                    "konami" => MapperType::Konami,
                    "konamiscc" => MapperType::KonamiScc,
                    "ascii8" => MapperType::Ascii8,
                    "ascii16" => MapperType::Ascii16,
                    _ => return Err(format!("Invalid mapper type: {value}")),
                };
            }
            "--headless" => {
                cli.headless = true;
            }
            "--frames" => {
                let value = next_option_str(args, &mut i, "--frames")?;
                cli.frames = value
                    .parse()
                    .map_err(|_| format!("Invalid value for --frames: {value}"))?;
            }
            "--screenshot" => {
                cli.screenshot_path = Some(next_option_value(args, &mut i, "--screenshot")?);
            }
            "--region" => {
                let value = next_option_str(args, &mut i, "--region")?;
                cli.region = match value.to_lowercase().as_str() {
                    "ntsc" => MsxRegion::Ntsc,
                    "pal" => MsxRegion::Pal,
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
    system: Msx,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    last_frame_time: Instant,
    frame_duration: Duration,
    menu_ids: MenuIds,
    _menu: Menu,
}

impl App {
    fn new(system: Msx, menu: Menu, menu_ids: MenuIds, region: MsxRegion) -> Self {
        let frame_duration = match region {
            MsxRegion::Ntsc => FRAME_DURATION_NTSC,
            MsxRegion::Pal => FRAME_DURATION_PAL,
        };
        Self {
            system,
            window: None,
            renderer: None,
            last_frame_time: Instant::now(),
            frame_duration,
            menu_ids,
            _menu: menu,
        }
    }

    fn handle_key(&mut self, keycode: KeyCode, pressed: bool) {
        if let Some((row, bit)) = msx_key_mapping(keycode) {
            if pressed {
                self.system.press_key(row, bit);
            } else {
                self.system.release_key(row, bit);
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
            .with_title("MSX")
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
                    // Escape handled by keyboard matrix (row 7, bit 2)
                    self.handle_key(keycode, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= self.frame_duration {
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

fn make_system(cli: &CliArgs) -> Msx {
    let bios_path = match cli.bios_path.as_ref() {
        Some(p) => p,
        None => {
            eprintln!("No BIOS file specified. Use --bios <file>");
            process::exit(1);
        }
    };

    let bios_data = match std::fs::read(bios_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to read BIOS file {}: {e}", bios_path.display());
            process::exit(1);
        }
    };

    eprintln!("Loaded BIOS: {}", bios_path.display());

    let mut system = Msx::new(bios_data, cli.region);

    if let Some(ref cart_path) = cli.cart_path {
        let cart_data = match std::fs::read(cart_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read cartridge file {}: {e}", cart_path.display());
                process::exit(1);
            }
        };
        eprintln!("Loaded cartridge: {} (mapper: {:?})", cart_path.display(), cli.mapper);
        system.insert_cart1(cart_data, cli.mapper);
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

    let region = cli.region;
    let system = make_system(&cli);
    let (menu, menu_ids) = build_menu();
    let mut app = App::new(system, menu, menu_ids, region);

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
