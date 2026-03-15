//! Atari 7800 emulator binary.
//!
//! Runs the 7800 with a winit window and wgpu renderer, or in
//! headless mode for screenshots.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu_core::Cpu;
use emu_core::renderer::Renderer;
use emu_atari_7800::{
    Atari7800, Atari7800Config, Atari7800Region, capture,
    controller_map::{self, Atari7800Input},
    maria,
};
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Framebuffer dimensions.
const FB_WIDTH: u32 = maria::FB_WIDTH;

/// Window scale factor.
const SCALE: u32 = 2;

/// Frame duration for ~60 Hz NTSC.
const FRAME_DURATION_NTSC: Duration = Duration::from_micros(16_639);
/// Frame duration for ~50 Hz PAL.
const FRAME_DURATION_PAL: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CliArgs {
    rom_path: Option<PathBuf>,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    region: Atari7800Region,
}

fn print_usage() {
    eprintln!("Usage: emu-atari-7800 [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --rom <file>         Atari 7800 cartridge ROM file");
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
        rom_path: None,
        headless: false,
        frames: 200,
        screenshot_path: None,
        region: Atari7800Region::Ntsc,
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
                    "ntsc" => Atari7800Region::Ntsc,
                    "pal" => Atari7800Region::Pal,
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
// Headless mode
// ---------------------------------------------------------------------------

fn run_headless(cli: &CliArgs) {
    let mut system = make_system(cli);

    for _ in 0..cli.frames {
        system.run_frame();
    }

    if let Some(ref path) = cli.screenshot_path {
        if let Err(e) = capture::save_screenshot(&system, path) {
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
    system: Atari7800,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    last_frame_time: Instant,
    frame_duration: Duration,
    menu_ids: MenuIds,
    _menu: Menu,
    /// Joystick direction state.
    joy_up: bool,
    joy_down: bool,
    joy_left: bool,
    joy_right: bool,
}

impl App {
    fn new(system: Atari7800, menu: Menu, menu_ids: MenuIds) -> Self {
        let frame_duration = match system.region() {
            Atari7800Region::Ntsc => FRAME_DURATION_NTSC,
            Atari7800Region::Pal => FRAME_DURATION_PAL,
        };
        Self {
            system,
            window: None,
            renderer: None,
            last_frame_time: Instant::now(),
            frame_duration,
            menu_ids,
            _menu: menu,
            joy_up: false,
            joy_down: false,
            joy_left: false,
            joy_right: false,
        }
    }

    fn handle_key(&mut self, keycode: KeyCode, pressed: bool) {
        if let Some(input) = controller_map::map_keycode(keycode) {
            match input {
                Atari7800Input::P0Up => {
                    self.joy_up = pressed;
                    self.system.set_joystick(
                        self.joy_up, self.joy_down, self.joy_left, self.joy_right,
                    );
                }
                Atari7800Input::P0Down => {
                    self.joy_down = pressed;
                    self.system.set_joystick(
                        self.joy_up, self.joy_down, self.joy_left, self.joy_right,
                    );
                }
                Atari7800Input::P0Left => {
                    self.joy_left = pressed;
                    self.system.set_joystick(
                        self.joy_up, self.joy_down, self.joy_left, self.joy_right,
                    );
                }
                Atari7800Input::P0Right => {
                    self.joy_right = pressed;
                    self.system.set_joystick(
                        self.joy_up, self.joy_down, self.joy_left, self.joy_right,
                    );
                }
                Atari7800Input::P0Fire => {
                    self.system.set_fire(pressed);
                }
                Atari7800Input::P0Fire2
                | Atari7800Input::Pause
                | Atari7800Input::Reset
                | Atari7800Input::Select => {
                    // Not yet wired.
                }
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
            let path = std::path::PathBuf::from("screenshot.png");
            if let Err(e) = capture::save_screenshot(&self.system, &path) {
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

        let fb_height = self.system.framebuffer_height();
        let window_size =
            winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, fb_height * SCALE);
        let attrs = WindowAttributes::default()
            .with_title("Atari 7800")
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

                let renderer = Renderer::new(window.clone(), FB_WIDTH, fb_height, emu_core::renderer::FilterMode::Nearest);
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

fn make_system_result(cli: &CliArgs) -> Result<Atari7800, String> {
    let rom_path = cli
        .rom_path
        .as_ref()
        .ok_or_else(|| "No ROM file specified. Use --rom <file>".to_string())?;

    let rom_data = std::fs::read(rom_path)
        .map_err(|e| format!("Failed to read ROM file {}: {e}", rom_path.display()))?;

    let config = Atari7800Config {
        rom_data,
        region: cli.region,
    };
    Atari7800::new(&config)
}

fn make_system(cli: &CliArgs) -> Atari7800 {
    match make_system_result(cli) {
        Ok(system) => {
            if let Some(ref rom_path) = cli.rom_path {
                eprintln!("Loaded ROM: {}", rom_path.display());
            }
            system
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_cli(args: &[&str]) -> Result<Option<CliArgs>, String> {
        let args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        parse_args_from(&args)
    }

    #[test]
    fn cli_parser_reads_basic_options() {
        let cli = parse_cli(&[
            "emu-atari-7800",
            "--rom",
            "game.a78",
            "--headless",
            "--region",
            "pal",
            "--frames",
            "42",
            "--screenshot",
            "out.png",
        ])
        .expect("parse should succeed")
        .expect("help was not requested");

        assert_eq!(cli.rom_path, Some(PathBuf::from("game.a78")));
        assert!(cli.headless);
        assert_eq!(cli.region, Atari7800Region::Pal);
        assert_eq!(cli.frames, 42);
        assert_eq!(cli.screenshot_path, Some(PathBuf::from("out.png")));
    }

    #[test]
    fn cli_parser_promotes_screenshot_to_headless() {
        let cli = parse_cli(&[
            "emu-atari-7800",
            "--rom",
            "game.a78",
            "--screenshot",
            "out.png",
        ])
        .expect("parse should succeed")
        .expect("help was not requested");

        assert!(cli.headless);
    }

    #[test]
    fn cli_parser_rejects_unknown_args() {
        let result = parse_cli(&["emu-atari-7800", "--bogus"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_parser_help_returns_none() {
        assert!(
            parse_cli(&["emu-atari-7800", "--help"])
                .expect("help parse should succeed")
                .is_none()
        );
    }
}
