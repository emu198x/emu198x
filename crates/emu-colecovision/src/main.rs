//! ColecoVision emulator binary.
//!
//! Runs the ColecoVision with a winit window and wgpu renderer, or in
//! headless mode for screenshots.

#![allow(clippy::cast_possible_truncation)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu_core::Cpu;
use emu_core::renderer::Renderer;
use emu_colecovision::{ColecoVision, CvRegion, KeypadKey};
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
// CLI argument parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CliArgs {
    bios_path: Option<PathBuf>,
    rom_path: Option<PathBuf>,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    region: CvRegion,
}

fn print_usage() {
    eprintln!("Usage: emu-colecovision [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --bios <file>        ColecoVision BIOS ROM (8 KB, required)");
    eprintln!("  --rom <file>         Cartridge ROM file");
    eprintln!("  --region <ntsc|pal>  Video region (default: ntsc)");
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
        bios_path: None,
        rom_path: None,
        headless: false,
        frames: 200,
        screenshot_path: None,
        region: CvRegion::Ntsc,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bios" => {
                cli.bios_path = Some(next_option_value(args, &mut i, "--bios")?);
            }
            "--rom" => {
                cli.rom_path = Some(next_option_value(args, &mut i, "--rom")?);
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
            "--region" => {
                i += 1;
                let value = args
                    .get(i)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--region requires a value".to_string())?;
                cli.region = match value.to_lowercase().as_str() {
                    "ntsc" => CvRegion::Ntsc,
                    "pal" => CvRegion::Pal,
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
    system: ColecoVision,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    last_frame_time: Instant,
    frame_duration: Duration,
    menu_ids: MenuIds,
    _menu: Menu,
}

impl App {
    fn new(system: ColecoVision, menu: Menu, menu_ids: MenuIds, region: CvRegion) -> Self {
        let frame_duration = match region {
            CvRegion::Ntsc => FRAME_DURATION_NTSC,
            CvRegion::Pal => FRAME_DURATION_PAL,
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
        let ctrl = self.system.controller1_mut();
        match keycode {
            KeyCode::ArrowUp => ctrl.up = pressed,
            KeyCode::ArrowDown => ctrl.down = pressed,
            KeyCode::ArrowLeft => ctrl.left = pressed,
            KeyCode::ArrowRight => ctrl.right = pressed,
            KeyCode::KeyZ => ctrl.left_button = pressed,
            KeyCode::KeyX => ctrl.right_button = pressed,
            // Number keys map to ColecoVision keypad
            KeyCode::Digit0 => ctrl.keypad = if pressed { Some(KeypadKey::K0) } else { None },
            KeyCode::Digit1 => ctrl.keypad = if pressed { Some(KeypadKey::K1) } else { None },
            KeyCode::Digit2 => ctrl.keypad = if pressed { Some(KeypadKey::K2) } else { None },
            KeyCode::Digit3 => ctrl.keypad = if pressed { Some(KeypadKey::K3) } else { None },
            KeyCode::Digit4 => ctrl.keypad = if pressed { Some(KeypadKey::K4) } else { None },
            KeyCode::Digit5 => ctrl.keypad = if pressed { Some(KeypadKey::K5) } else { None },
            KeyCode::Digit6 => ctrl.keypad = if pressed { Some(KeypadKey::K6) } else { None },
            KeyCode::Digit7 => ctrl.keypad = if pressed { Some(KeypadKey::K7) } else { None },
            KeyCode::Digit8 => ctrl.keypad = if pressed { Some(KeypadKey::K8) } else { None },
            KeyCode::Digit9 => ctrl.keypad = if pressed { Some(KeypadKey::K9) } else { None },
            KeyCode::Minus => ctrl.keypad = if pressed { Some(KeypadKey::Star) } else { None },
            KeyCode::Equal => ctrl.keypad = if pressed { Some(KeypadKey::Hash) } else { None },
            _ => {}
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
            .with_title("ColecoVision")
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

fn make_system(cli: &CliArgs) -> ColecoVision {
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

    let rom_path = match cli.rom_path.as_ref() {
        Some(p) => p,
        None => {
            eprintln!("No ROM file specified. Use --rom <file>");
            process::exit(1);
        }
    };

    let rom_data = match std::fs::read(rom_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to read ROM file {}: {e}", rom_path.display());
            process::exit(1);
        }
    };

    eprintln!("Loaded BIOS: {}", bios_path.display());
    eprintln!("Loaded ROM: {}", rom_path.display());
    ColecoVision::new(bios_data, rom_data, cli.region)
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
