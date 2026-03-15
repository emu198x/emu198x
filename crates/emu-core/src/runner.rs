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

use muda::{
    AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use crate::Machine;
use crate::audio::AudioOutput;
use crate::capture::{AudioCapture, save_screenshot_argb32};
use crate::renderer::{FilterMode, Renderer};

/// Key handler function type: `(machine, keycode, pressed)`.
pub type KeyHandler<M> = Box<dyn FnMut(&mut M, KeyCode, bool)>;

/// Open handler: takes a file path, returns a new Machine (or None on error).
pub type OpenHandler<M> = Box<dyn FnMut(&PathBuf) -> Option<M>>;

struct ViewMenuIds {
    scale_native: MenuId,
    scale_2x: MenuId,
    scale_3x: MenuId,
    scale_4x: MenuId,
    filter_nearest: MenuId,
    filter_linear: MenuId,
    fullscreen: MenuId,
}

/// Menu IDs for the standard File/System menus.
struct MenuIds {
    open: MenuId,
    view: ViewMenuIds,
    screenshot: MenuId,
    start_audio_capture: MenuId,
    stop_audio_capture: MenuId,
    quit: Option<MenuId>,
    soft_reset: MenuId,
    hard_reset: MenuId,
}

#[derive(Clone)]
struct ViewMenuControls {
    scale_native: CheckMenuItem,
    scale_2x: CheckMenuItem,
    scale_3x: CheckMenuItem,
    scale_4x: CheckMenuItem,
    filter_nearest: CheckMenuItem,
    filter_linear: CheckMenuItem,
    fullscreen: CheckMenuItem,
}

#[derive(Clone)]
struct MenuControls {
    view: ViewMenuControls,
    start_audio_capture: MenuItem,
    stop_audio_capture: MenuItem,
}

fn build_menu(app_title: &str, extensions_label: &str) -> (Menu, MenuIds, MenuControls) {
    let menu = Menu::new();

    #[cfg(target_os = "macos")]
    {
        // On macOS the first top-level submenu becomes the app menu, so create
        // one explicitly to keep File/System in their expected positions.
        let app_menu = Submenu::new(app_title, true);
        app_menu
            .append(&PredefinedMenuItem::about(
                None,
                Some(default_about_metadata(app_title)),
            ))
            .ok();
        app_menu.append(&PredefinedMenuItem::separator()).ok();
        app_menu.append(&PredefinedMenuItem::services(None)).ok();
        app_menu.append(&PredefinedMenuItem::separator()).ok();
        app_menu.append(&PredefinedMenuItem::hide(None)).ok();
        app_menu.append(&PredefinedMenuItem::hide_others(None)).ok();
        app_menu.append(&PredefinedMenuItem::show_all(None)).ok();
        app_menu.append(&PredefinedMenuItem::separator()).ok();
        app_menu.append(&PredefinedMenuItem::quit(None)).ok();
        menu.append(&app_menu).ok();
    }

    let file_menu = Submenu::new("File", true);
    let open_label = if extensions_label.is_empty() {
        "Open ROM...\tCtrl+O".to_string()
    } else {
        format!("Open ROM ({extensions_label})...\tCtrl+O")
    };
    let open = MenuItem::new(&open_label, true, None);
    file_menu.append(&open).ok();

    #[cfg(not(target_os = "macos"))]
    let quit = {
        let quit = MenuItem::new("Quit\tCtrl+Q", true, None);
        file_menu.append(&PredefinedMenuItem::separator()).ok();
        file_menu.append(&quit).ok();
        Some(quit)
    };

    #[cfg(target_os = "macos")]
    let quit: Option<MenuItem> = None;

    let view_menu = Submenu::new("View", true);
    let scale_menu = Submenu::new("Scale", true);
    let scale_native = CheckMenuItem::new("Native (1x)", true, true, None);
    let scale_2x = CheckMenuItem::new("2x", true, false, None);
    let scale_3x = CheckMenuItem::new("3x", true, false, None);
    let scale_4x = CheckMenuItem::new("4x", true, false, None);
    scale_menu.append(&scale_native).ok();
    scale_menu.append(&scale_2x).ok();
    scale_menu.append(&scale_3x).ok();
    scale_menu.append(&scale_4x).ok();

    let filter_menu = Submenu::new("Filtering", true);
    let filter_nearest = CheckMenuItem::new("Sharp Pixels", true, true, None);
    let filter_linear = CheckMenuItem::new("Smooth Scaling", true, false, None);
    filter_menu.append(&filter_nearest).ok();
    filter_menu.append(&filter_linear).ok();

    let fullscreen = CheckMenuItem::new("Full Screen", true, false, None);
    view_menu.append(&scale_menu).ok();
    view_menu.append(&filter_menu).ok();
    view_menu.append(&PredefinedMenuItem::separator()).ok();
    view_menu.append(&fullscreen).ok();

    let capture_menu = Submenu::new("Capture", true);
    let screenshot = MenuItem::new("Save Screenshot...\tCtrl+P", true, None);
    let start_audio_capture = MenuItem::new("Start Audio Capture...", true, None);
    let stop_audio_capture = MenuItem::new("Stop Audio Capture", false, None);
    capture_menu.append(&screenshot).ok();
    capture_menu.append(&PredefinedMenuItem::separator()).ok();
    capture_menu.append(&start_audio_capture).ok();
    capture_menu.append(&stop_audio_capture).ok();

    let system_menu = Submenu::new("System", true);
    let soft_reset = MenuItem::new("Soft Reset\tCtrl+R", true, None);
    let hard_reset = MenuItem::new("Hard Reset\tCtrl+Shift+R", true, None);
    system_menu.append(&soft_reset).ok();
    system_menu.append(&hard_reset).ok();

    menu.append(&file_menu).ok();
    menu.append(&view_menu).ok();
    menu.append(&capture_menu).ok();
    menu.append(&system_menu).ok();

    let ids = MenuIds {
        open: open.id().clone(),
        view: ViewMenuIds {
            scale_native: scale_native.id().clone(),
            scale_2x: scale_2x.id().clone(),
            scale_3x: scale_3x.id().clone(),
            scale_4x: scale_4x.id().clone(),
            filter_nearest: filter_nearest.id().clone(),
            filter_linear: filter_linear.id().clone(),
            fullscreen: fullscreen.id().clone(),
        },
        screenshot: screenshot.id().clone(),
        start_audio_capture: start_audio_capture.id().clone(),
        stop_audio_capture: stop_audio_capture.id().clone(),
        quit: quit.as_ref().map(|item| item.id().clone()),
        soft_reset: soft_reset.id().clone(),
        hard_reset: hard_reset.id().clone(),
    };

    let controls = MenuControls {
        view: ViewMenuControls {
            scale_native,
            scale_2x,
            scale_3x,
            scale_4x,
            filter_nearest,
            filter_linear,
            fullscreen,
        },
        start_audio_capture,
        stop_audio_capture,
    };

    (menu, ids, controls)
}

fn default_about_metadata(app_title: &str) -> AboutMetadata {
    let mut metadata = AboutMetadata {
        name: Some(app_title.to_string()),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        copyright: Some("Emu198x / Code Like It's 198x".to_string()),
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    {
        metadata.credits = Some(
            "Part of Emu198x, a cycle-accurate emulator suite built for Code Like It's 198x.\n\n\
             The suite is designed for retro-computing lessons, scripted capture, observable state, \
             and browser-hosted learning tools."
                .to_string(),
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        metadata.comments = Some(
            "Part of Emu198x, a cycle-accurate emulator suite built for Code Like It's 198x.\n\n\
             The suite is designed for retro-computing lessons, scripted capture, observable state, \
             and browser-hosted learning tools."
                .to_string(),
        );
        metadata.license = Some(env!("CARGO_PKG_LICENSE").to_string());
    }

    metadata
}

/// Generic windowed runner for any `Machine`.
pub struct Runner<M: Machine> {
    machine: M,
    title: String,
    scale: u32,
    filter_mode: FilterMode,
    frame_duration: Duration,
    audio_enabled: bool,
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
            filter_mode: FilterMode::Nearest,
            frame_duration,
            audio_enabled: true,
            key_handler: None,
            open_handler: None,
            file_extensions: Vec::new(),
            quit_key: KeyCode::Escape,
        }
    }

    /// Set the key handler. Called for every key press/release.
    pub fn with_key_handler(
        mut self,
        handler: impl FnMut(&mut M, KeyCode, bool) + 'static,
    ) -> Self {
        self.key_handler = Some(Box::new(handler));
        self
    }

    /// Enable or disable host audio playback (default: enabled).
    pub fn with_audio_enabled(mut self, enabled: bool) -> Self {
        self.audio_enabled = enabled;
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
        let (menu, menu_ids, menu_controls) = build_menu(&self.title, &ext_label);
        let fb_width = self.machine.framebuffer_width();
        let fb_height = self.machine.framebuffer_height();
        let audio_sample_rate = self.machine.audio_sample_rate();
        let audio = if self.audio_enabled {
            match AudioOutput::new(audio_sample_rate, self.frame_duration) {
                Ok(audio) => Some(audio),
                Err(e) => {
                    eprintln!("Audio output disabled: {e}");
                    None
                }
            }
        } else {
            None
        };

        let mut app = App {
            machine: self.machine,
            renderer: None,
            window: None,
            last_frame_time: Instant::now(),
            frame_duration: self.frame_duration,
            audio,
            audio_enabled: self.audio_enabled,
            audio_sample_rate,
            audio_capture: None,
            title: self.title,
            scale: self.scale,
            filter_mode: self.filter_mode,
            fullscreen: false,
            fb_width,
            fb_height,
            menu_ids,
            menu_controls,
            _menu: menu,
            key_handler: self.key_handler,
            open_handler: self.open_handler,
            file_extensions: self.file_extensions,
            quit_key: self.quit_key,
            pending_windowed_resize: false,
        };

        let event_loop = match EventLoop::new() {
            Ok(el) => el,
            Err(e) => {
                eprintln!("Failed to create event loop: {e}");
                return;
            }
        };

        // WaitUntil is set per-frame in about_to_wait() for proper pacing.

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
    audio: Option<AudioOutput>,
    audio_enabled: bool,
    audio_sample_rate: u32,
    audio_capture: Option<AudioCapture>,
    title: String,
    scale: u32,
    filter_mode: FilterMode,
    fullscreen: bool,
    fb_width: u32,
    fb_height: u32,
    menu_ids: MenuIds,
    menu_controls: MenuControls,
    _menu: Menu,
    key_handler: Option<KeyHandler<M>>,
    open_handler: Option<OpenHandler<M>>,
    file_extensions: Vec<String>,
    quit_key: KeyCode,
    pending_windowed_resize: bool,
}

impl<M: Machine> App<M> {
    fn set_audio_capture_menu_state(&self, recording: bool) {
        self.menu_controls
            .start_audio_capture
            .set_enabled(!recording);
        self.menu_controls.stop_audio_capture.set_enabled(recording);
    }

    fn sync_view_menu_state(&self) {
        self.menu_controls
            .view
            .scale_native
            .set_checked(self.scale == 1);
        self.menu_controls
            .view
            .scale_2x
            .set_checked(self.scale == 2);
        self.menu_controls
            .view
            .scale_3x
            .set_checked(self.scale == 3);
        self.menu_controls
            .view
            .scale_4x
            .set_checked(self.scale == 4);
        self.menu_controls
            .view
            .filter_nearest
            .set_checked(self.filter_mode == FilterMode::Nearest);
        self.menu_controls
            .view
            .filter_linear
            .set_checked(self.filter_mode == FilterMode::Linear);
        self.menu_controls
            .view
            .fullscreen
            .set_checked(self.fullscreen);
    }

    fn default_capture_name(&self, filename: &str) -> String {
        let stem = self.title.to_lowercase().replace(' ', "-");
        format!("{stem}-{filename}")
    }

    fn save_screenshot_dialog(&self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name(&self.default_capture_name("screenshot.png"));

        if let Some(path) = dialog.save_file() {
            match save_screenshot_argb32(
                self.machine.framebuffer(),
                self.fb_width,
                self.fb_height,
                &path,
            ) {
                Ok(()) => eprintln!("Screenshot saved to {}", path.display()),
                Err(e) => eprintln!("Screenshot error: {e}"),
            }
        }
    }

    fn start_audio_capture_dialog(&mut self) {
        if self.audio_capture.is_some() {
            return;
        }

        let dialog = rfd::FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .set_file_name(&self.default_capture_name("audio.wav"));

        if let Some(path) = dialog.save_file() {
            match AudioCapture::start(path, self.audio_sample_rate) {
                Ok(capture) => {
                    eprintln!("Audio capture started: {}", capture.path().display());
                    self.audio_capture = Some(capture);
                    self.set_audio_capture_menu_state(true);
                }
                Err(e) => eprintln!("Audio capture error: {e}"),
            }
        }
    }

    fn stop_audio_capture(&mut self) {
        let Some(mut capture) = self.audio_capture.take() else {
            return;
        };

        let path = capture.path().to_path_buf();
        match capture.finish() {
            Ok(()) => eprintln!("Audio capture saved to {}", path.display()),
            Err(e) => eprintln!("Audio capture error: {e}"),
        }
        self.set_audio_capture_menu_state(false);
    }

    fn rebuild_audio_output(&mut self, sample_rate: u32) {
        self.audio_sample_rate = sample_rate;
        if !self.audio_enabled {
            self.audio = None;
            return;
        }

        self.audio = match AudioOutput::new(sample_rate, self.frame_duration) {
            Ok(audio) => Some(audio),
            Err(e) => {
                eprintln!("Audio output disabled: {e}");
                None
            }
        };
    }

    fn clear_audio(&self) {
        if let Some(audio) = &self.audio {
            audio.clear();
        }
    }

    fn request_windowed_size(&mut self) {
        if self.fullscreen {
            self.pending_windowed_resize = true;
            return;
        }

        let Some(window) = &self.window else {
            return;
        };

        let _ = window.request_inner_size(window_size_for_scale(
            self.fb_width,
            self.fb_height,
            self.scale,
        ));
        self.pending_windowed_resize = false;
    }

    fn set_scale(&mut self, scale: u32) {
        self.scale = scale;
        self.request_windowed_size();
        self.sync_view_menu_state();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn set_filter_mode(&mut self, filter_mode: FilterMode) {
        self.filter_mode = filter_mode;
        if let Some(renderer) = &mut self.renderer {
            renderer.set_filter_mode(filter_mode);
        }
        self.sync_view_menu_state();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;

        if let Some(window) = &self.window {
            if fullscreen {
                window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            } else {
                self.pending_windowed_resize = true;
                window.set_fullscreen(None);
            }
            window.request_redraw();
        }

        self.sync_view_menu_state();
    }

    fn sync_fullscreen_state_from_window(&mut self) {
        let Some(window) = &self.window else {
            return;
        };

        let fullscreen = window.fullscreen().is_some();
        if self.fullscreen != fullscreen {
            self.fullscreen = fullscreen;
            self.sync_view_menu_state();
        }
    }

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
                self.clear_audio();

                let new_sample_rate = self.machine.audio_sample_rate();
                if new_sample_rate != self.audio_sample_rate {
                    if self.audio_capture.is_some() {
                        eprintln!(
                            "Stopping audio capture before switching to a machine with a different sample rate",
                        );
                        self.stop_audio_capture();
                    }
                    self.rebuild_audio_output(new_sample_rate);
                }

                // Update framebuffer dimensions if they changed
                let new_w = self.machine.framebuffer_width();
                let new_h = self.machine.framebuffer_height();
                if new_w != self.fb_width || new_h != self.fb_height {
                    self.fb_width = new_w;
                    self.fb_height = new_h;
                    if let Some(window) = &self.window {
                        self.renderer = Some(Renderer::new(
                            window.clone(),
                            self.fb_width,
                            self.fb_height,
                            self.filter_mode,
                        ));
                    }
                    self.request_windowed_size();
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
        if self
            .menu_ids
            .quit
            .as_ref()
            .is_some_and(|quit_id| *id == *quit_id)
        {
            event_loop.exit();
        } else if *id == self.menu_ids.open {
            self.open_file_dialog();
        } else if *id == self.menu_ids.view.scale_native {
            self.set_scale(1);
        } else if *id == self.menu_ids.view.scale_2x {
            self.set_scale(2);
        } else if *id == self.menu_ids.view.scale_3x {
            self.set_scale(3);
        } else if *id == self.menu_ids.view.scale_4x {
            self.set_scale(4);
        } else if *id == self.menu_ids.view.filter_nearest {
            self.set_filter_mode(FilterMode::Nearest);
        } else if *id == self.menu_ids.view.filter_linear {
            self.set_filter_mode(FilterMode::Linear);
        } else if *id == self.menu_ids.view.fullscreen {
            self.set_fullscreen(!self.fullscreen);
        } else if *id == self.menu_ids.screenshot {
            self.save_screenshot_dialog();
        } else if *id == self.menu_ids.start_audio_capture {
            self.start_audio_capture_dialog();
        } else if *id == self.menu_ids.stop_audio_capture {
            self.stop_audio_capture();
        } else if *id == self.menu_ids.soft_reset {
            self.clear_audio();
            self.machine.reset();
            eprintln!("Soft reset");
        } else if *id == self.menu_ids.hard_reset {
            self.clear_audio();
            self.machine.reset();
            eprintln!("Hard reset");
        }
    }
}

fn window_size_for_scale(
    fb_width: u32,
    fb_height: u32,
    scale: u32,
) -> winit::dpi::LogicalSize<f64> {
    winit::dpi::LogicalSize::new(
        f64::from(fb_width.saturating_mul(scale)),
        f64::from(fb_height.saturating_mul(scale)),
    )
}

impl<M: Machine> ApplicationHandler for App<M> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_size = window_size_for_scale(self.fb_width, self.fb_height, self.scale);
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
                        if let winit::raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw()
                        {
                            unsafe {
                                self._menu.init_for_hwnd(h.hwnd.get() as _).ok();
                            }
                        }
                    }
                }

                let renderer = Renderer::new(
                    window.clone(),
                    self.fb_width,
                    self.fb_height,
                    self.filter_mode,
                );
                self.renderer = Some(renderer);
                self.window = Some(window);
                self.sync_fullscreen_state_from_window();
                self.sync_view_menu_state();
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
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
                self.sync_fullscreen_state_from_window();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let size = window.inner_size();
                    renderer.resize(size.width, size.height);
                }
                self.sync_fullscreen_state_from_window();
            }
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
                    let samples = self.machine.take_audio_buffer();
                    if let Some(capture) = &mut self.audio_capture
                        && let Err(e) = capture.append_frames(&samples)
                    {
                        eprintln!("Audio capture error: {e}");
                        self.audio_capture = None;
                        self.set_audio_capture_menu_state(false);
                    }
                    if let Some(audio) = &self.audio {
                        audio.push_frames(&samples);
                    }

                    if let Some(renderer) = &mut self.renderer {
                        renderer.upload_framebuffer(self.machine.framebuffer());
                    }

                    self.last_frame_time = now;
                }

                if let Some(renderer) = &self.renderer {
                    if let Err(e) = renderer.render() {
                        match e {
                            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                                if let (Some(renderer), Some(window)) =
                                    (&mut self.renderer, &self.window)
                                {
                                    let size = window.inner_size();
                                    renderer.resize(size.width, size.height);
                                }
                            }
                            wgpu::SurfaceError::Timeout => {}
                            wgpu::SurfaceError::OutOfMemory => {
                                eprintln!("Render error: out of memory");
                                event_loop.exit();
                            }
                            wgpu::SurfaceError::Other => {
                                eprintln!("Render error: surface unavailable");
                                event_loop.exit();
                            }
                        }
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

        self.sync_fullscreen_state_from_window();
        if self.pending_windowed_resize && !self.fullscreen {
            self.request_windowed_size();
        }

        // Schedule the next frame: sleep until frame_duration has elapsed,
        // then request a redraw. This keeps CPU usage low (~2-5%) instead
        // of busy-spinning at 100%.
        let next_frame = self.last_frame_time + self.frame_duration;
        let now = Instant::now();
        if now >= next_frame {
            // Already past due — request redraw immediately.
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        } else {
            // Sleep until the next frame is due.
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_frame));
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
