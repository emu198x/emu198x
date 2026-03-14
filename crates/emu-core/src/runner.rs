//! Generic windowed runner for any `Machine` implementation.
//!
//! Provides the full winit + wgpu + muda application loop. Each system's
//! `main.rs` reduces to constructing its `Machine`, providing a key mapping
//! function, and calling `Runner::run()`.
//!
//! ```ignore
//! let machine = MySystem::new(rom);
//! Runner::new(machine, "My System", 3, Duration::from_millis(20))
//!     .with_key_handler(|machine, keycode, pressed| { ... })
//!     .with_open_handler(&["sg", "bin"], |path| { ... })
//!     .run();
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::Machine;
use crate::renderer::Renderer;

/// Key handler function type: `(machine, keycode, pressed)`.
pub type KeyHandler<M> = Box<dyn FnMut(&mut M, KeyCode, bool)>;

/// Open handler: takes a file path, returns a new Machine (or None on error).
pub type OpenHandler<M> = Box<dyn FnMut(&PathBuf) -> Option<M>>;

/// Menu IDs for the standard File/System menus.
struct MenuIds {
    open: MenuId,
    screenshot: MenuId,
    quit: MenuId,
    soft_reset: MenuId,
    hard_reset: MenuId,
}

fn build_menu(extensions_label: &str) -> (Menu, MenuIds) {
    let menu = Menu::new();

    let file_menu = Submenu::new("File", true);
    let open_label = if extensions_label.is_empty() {
        "Open ROM...\tCtrl+O".to_string()
    } else {
        format!("Open ROM ({extensions_label})...\tCtrl+O")
    };
    let open = MenuItem::new(&open_label, true, None);
    let screenshot = MenuItem::new("Screenshot\tCtrl+P", true, None);
    let quit = MenuItem::new("Quit\tCtrl+Q", true, None);
    file_menu.append(&open).ok();
    file_menu.append(&PredefinedMenuItem::separator()).ok();
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
        open: open.id().clone(),
        screenshot: screenshot.id().clone(),
        quit: quit.id().clone(),
        soft_reset: soft_reset.id().clone(),
        hard_reset: hard_reset.id().clone(),
    };

    (menu, ids)
}

/// Generic windowed runner for any `Machine`.
pub struct Runner<M: Machine> {
    machine: M,
    title: String,
    scale: u32,
    frame_duration: Duration,
    key_handler: Option<KeyHandler<M>>,
    open_handler: Option<OpenHandler<M>>,
    file_extensions: Vec<String>,
    quit_key: KeyCode,
}

impl<M: Machine> Runner<M> {
    /// Create a new runner.
    pub fn new(machine: M, title: &str, scale: u32, frame_duration: Duration) -> Self {
        Self {
            machine,
            title: title.to_string(),
            scale,
            frame_duration,
            key_handler: None,
            open_handler: None,
            file_extensions: Vec::new(),
            quit_key: KeyCode::Escape,
        }
    }

    /// Set the key handler. Called for every key press/release.
    pub fn with_key_handler(mut self, handler: impl FnMut(&mut M, KeyCode, bool) + 'static) -> Self {
        self.key_handler = Some(Box::new(handler));
        self
    }

    /// Set the File > Open handler. The callback receives the chosen file path
    /// and returns a new Machine, or None if loading failed. The file dialog
    /// will filter by the given extensions (e.g., `&["sg", "bin"]`).
    pub fn with_open_handler(
        mut self,
        extensions: &[&str],
        handler: impl FnMut(&PathBuf) -> Option<M> + 'static,
    ) -> Self {
        self.file_extensions = extensions.iter().map(|s| (*s).to_string()).collect();
        self.open_handler = Some(Box::new(handler));
        self
    }

    /// Set the quit key (default: Escape).
    pub fn with_quit_key(mut self, key: KeyCode) -> Self {
        self.quit_key = key;
        self
    }

    /// Run the windowed application. Blocks until the window is closed.
    pub fn run(self) {
        let ext_label = self.file_extensions.join(", ");
        let (menu, menu_ids) = build_menu(&ext_label);
        let fb_width = self.machine.framebuffer_width();
        let fb_height = self.machine.framebuffer_height();

        let mut app = App {
            machine: self.machine,
            renderer: None,
            window: None,
            last_frame_time: Instant::now(),
            frame_duration: self.frame_duration,
            title: self.title,
            scale: self.scale,
            fb_width,
            fb_height,
            menu_ids,
            _menu: menu,
            key_handler: self.key_handler,
            open_handler: self.open_handler,
            file_extensions: self.file_extensions,
            quit_key: self.quit_key,
        };

        let event_loop = match EventLoop::new() {
            Ok(el) => el,
            Err(e) => {
                eprintln!("Failed to create event loop: {e}");
                return;
            }
        };

        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        if let Err(e) = event_loop.run_app(&mut app) {
            eprintln!("Event loop error: {e}");
        }
    }
}

/// Internal application state.
struct App<M: Machine> {
    machine: M,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    last_frame_time: Instant,
    frame_duration: Duration,
    title: String,
    scale: u32,
    fb_width: u32,
    fb_height: u32,
    menu_ids: MenuIds,
    _menu: Menu,
    key_handler: Option<KeyHandler<M>>,
    open_handler: Option<OpenHandler<M>>,
    file_extensions: Vec<String>,
    quit_key: KeyCode,
}

impl<M: Machine> App<M> {
    fn open_file_dialog(&mut self) {
        let Some(handler) = &mut self.open_handler else {
            return;
        };

        let mut dialog = rfd::FileDialog::new();
        if !self.file_extensions.is_empty() {
            let ext_refs: Vec<&str> = self.file_extensions.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter("ROM files", &ext_refs);
        }

        if let Some(path) = dialog.pick_file() {
            if let Some(new_machine) = handler(&path) {
                self.machine = new_machine;

                // Update framebuffer dimensions if they changed
                let new_w = self.machine.framebuffer_width();
                let new_h = self.machine.framebuffer_height();
                if new_w != self.fb_width || new_h != self.fb_height {
                    self.fb_width = new_w;
                    self.fb_height = new_h;
                    if let Some(window) = &self.window {
                        self.renderer = Some(Renderer::new(
                            window.clone(), self.fb_width, self.fb_height,
                        ));
                    }
                }

                // Update window title with filename
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let title = format!("{} — {name}", self.title);
                    if let Some(window) = &self.window {
                        window.set_title(&title);
                    }
                }
                eprintln!("Loaded: {}", path.display());
            }
        }
    }

    fn handle_menu_event(&mut self, id: &MenuId, event_loop: &ActiveEventLoop) {
        if *id == self.menu_ids.quit {
            event_loop.exit();
        } else if *id == self.menu_ids.open {
            self.open_file_dialog();
        } else if *id == self.menu_ids.soft_reset {
            self.machine.reset();
            eprintln!("Soft reset");
        } else if *id == self.menu_ids.hard_reset {
            self.machine.reset();
            eprintln!("Hard reset");
        } else if *id == self.menu_ids.screenshot {
            eprintln!("Screenshot (not implemented in generic runner)");
        }
    }
}

impl<M: Machine> ApplicationHandler for App<M> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_size = winit::dpi::LogicalSize::new(
            self.fb_width * self.scale,
            self.fb_height * self.scale,
        );
        let attrs = WindowAttributes::default()
            .with_title(&self.title)
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

                let renderer = Renderer::new(window.clone(), self.fb_width, self.fb_height);
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    if keycode == self.quit_key && pressed {
                        event_loop.exit();
                        return;
                    }
                    if let Some(handler) = &mut self.key_handler {
                        handler(&mut self.machine, keycode, pressed);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= self.frame_duration {
                    self.machine.run_frame();
                    let _ = self.machine.take_audio_buffer();

                    if let Some(renderer) = &mut self.renderer {
                        renderer.upload_framebuffer(self.machine.framebuffer());
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
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle_menu_event(event.id(), event_loop);
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
