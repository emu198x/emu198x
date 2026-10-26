//! The GTK4 Linux host: a native GTK window around the shared [`HostCore`].
//!
//! The core does the machine; this module owns the window, the picture
//! widget and the menu bar, turns GTK events into core calls, and implements
//! [`Shell`] with GTK's own dialogs. See
//! `knowledge/decisions/host-ui-model.md` § 12.
//!
//! The raw picture is drawn by GTK. Every other video filter is drawn by
//! the same wgpu presenter the winit window uses, rendering offscreen at the
//! picture's device-pixel size; the host shows the result one to one.

mod keys;
mod menu;
mod screen;

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use emu198x_native_video::{PresentationProfile, WgpuVideoPresenter};
use emu198x_shell::{CapturedFrame, MachineCore, MachineTime, PixelFormat};
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use winit::keyboard::KeyCode;

use crate::host_core::{FileFilter, HostCore, MenuSync, Notice, Shell, Wake};
use crate::{UiError, UiSystem, VideoFilter, overlay};
use screen::Screen;

/// The window, its picture and the application, lent to the core.
struct GtkShell<'a> {
    app: &'a gtk4::Application,
    window: &'a gtk4::ApplicationWindow,
    screen: &'a Screen,
    filters: &'a mut Filters,
    /// Set when the core asks for a redraw; the frame is uploaded once the
    /// tick is over.
    redraw: bool,
}

impl GtkShell<'_> {
    /// Run a nested main loop until `start` has called its completion.
    fn wait_for<T: 'static>(start: impl FnOnce(Box<dyn FnOnce(T)>)) -> Option<T> {
        let main_loop = glib::MainLoop::new(None, false);
        let result = Rc::new(RefCell::new(None));
        let done = {
            let main_loop = main_loop.clone();
            let result = result.clone();
            Box::new(move |value: T| {
                result.replace(Some(value));
                main_loop.quit();
            })
        };
        start(done);
        main_loop.run();
        result.take()
    }

    fn filters(filter: FileFilter<'_>, all_files: bool) -> gio::ListStore {
        let store = gio::ListStore::new::<gtk4::FileFilter>();
        let (label, extensions) = filter;
        let main = gtk4::FileFilter::new();
        main.set_name(Some(label));
        for extension in extensions {
            main.add_suffix(extension);
        }
        store.append(&main);
        if all_files {
            let all = gtk4::FileFilter::new();
            all.set_name(Some("All files"));
            all.add_pattern("*");
            store.append(&all);
        }
        store
    }
}

impl Shell for GtkShell<'_> {
    fn set_title(&mut self, title: &str) {
        self.window.set_title(Some(title));
    }

    fn request_redraw(&mut self) {
        self.redraw = true;
    }

    fn request_picture_size(&mut self, width: f64, height: f64) {
        // The window keeps the menu bar's height on top of the picture.
        let extra = self.window.height() - self.screen.height();
        self.window
            .set_default_size(width.round() as i32, height.round() as i32 + extra.max(0));
    }

    fn framebuffer_resized(&mut self, _width: u32, _height: u32) -> Result<(), UiError> {
        // The picture widget takes its size from each frame's texture.
        Ok(())
    }

    fn reset_video_history(&mut self) {
        if let Some(video) = self.filters.presenter.as_mut() {
            video.reset_temporal_history();
        }
    }

    fn sync_menu(&mut self, change: MenuSync<'_>) {
        match change {
            MenuSync::Scale(scale) => {
                menu::set_state(self.app, "scale", scale.to_string().to_variant());
            }
            MenuSync::Filter(filter) => {
                menu::set_state(self.app, "filter", filter.to_string().to_variant());
            }
            MenuSync::Variant(id) => menu::set_state(self.app, "variant", id.to_variant()),
            MenuSync::PortDrive { device, kind_id } => {
                menu::set_state(self.app, &menu::drive_action(device), kind_id.to_variant());
            }
            MenuSync::Turbo(armed) => menu::set_state(self.app, "fast-forward", armed.to_variant()),
            MenuSync::HostKeyboard(host) => menu::set_state(
                self.app,
                "keyboard",
                if host { "host" } else { "original" }.to_variant(),
            ),
        }
    }

    fn pick_file(&mut self, title: &str, filter: FileFilter<'_>) -> Option<PathBuf> {
        let dialog = gtk4::FileDialog::builder()
            .title(title)
            .modal(true)
            .filters(&Self::filters(filter, true))
            .build();
        let window = self.window.clone();
        Self::wait_for(move |done| {
            dialog.open(Some(&window), None::<&gio::Cancellable>, move |result| {
                done(result.ok().and_then(|file| file.path()));
            });
        })
        .flatten()
    }

    fn pick_save_file(
        &mut self,
        title: &str,
        file_name: &str,
        filter: FileFilter<'_>,
        all_files: bool,
    ) -> Option<PathBuf> {
        let dialog = gtk4::FileDialog::builder()
            .title(title)
            .modal(true)
            .initial_name(file_name)
            .filters(&Self::filters(filter, all_files))
            .build();
        let window = self.window.clone();
        Self::wait_for(move |done| {
            dialog.save(Some(&window), None::<&gio::Cancellable>, move |result| {
                done(result.ok().and_then(|file| file.path()));
            });
        })
        .flatten()
    }

    fn message(&mut self, title: &str, level: Notice, text: &str) {
        let heading = match level {
            Notice::Info => title.to_owned(),
            Notice::Warning => format!("{title}: warning"),
            Notice::Error => format!("{title}: error"),
        };
        let dialog = gtk4::AlertDialog::builder()
            .message(heading)
            .detail(text)
            .modal(true)
            .build();
        let window = self.window.clone();
        Self::wait_for(move |done| {
            dialog.choose(Some(&window), None::<&gio::Cancellable>, move |_| done(()));
        });
    }
}

/// Everything the GTK host keeps between events.
struct GtkHost<S: UiSystem> {
    core: HostCore<S>,
    app: gtk4::Application,
    window: gtk4::ApplicationWindow,
    screen: Screen,
    /// RGBA scratch for indexed frames.
    scratch: Vec<u8>,
    filters: Filters,
    /// Physical keys held, so GTK's auto-repeat presses reach the core as
    /// repeats, as winit reports them.
    held_keys: HashSet<KeyCode>,
    /// Whether the cursor is currently hidden over the picture, so
    /// [`Self::sync_host_cursor`] only touches the widget on a change.
    cursor_hidden: bool,
    fatal_error: Option<UiError>,
}

impl<S: UiSystem> GtkHost<S> {
    /// Lend the window to the core, run `f`, then upload the frame if the
    /// core asked for a redraw.
    fn with_shell<T>(&mut self, f: impl FnOnce(&mut HostCore<S>, &mut GtkShell<'_>) -> T) -> T {
        let mut shell = GtkShell {
            app: &self.app,
            window: &self.window,
            screen: &self.screen,
            filters: &mut self.filters,
            redraw: false,
        };
        let result = f(&mut self.core, &mut shell);
        if shell.redraw {
            self.present();
        }
        result
    }

    /// One turn of the core's loop. `None` means the session is over.
    fn tick(&mut self) -> Option<Wake> {
        let result = self.with_shell(|core, shell| core.tick(shell));
        self.sync_host_cursor();
        match result {
            Ok(wake) => Some(wake),
            Err(err) => {
                self.fail(err);
                None
            }
        }
    }

    /// Show or hide the cursor over the picture as the core asks, touching
    /// the widget only on a change.
    fn sync_host_cursor(&mut self) {
        let hidden = !self.core.host_cursor_visible();
        if hidden == self.cursor_hidden {
            return;
        }
        self.screen
            .set_cursor_from_name(if hidden { Some("none") } else { None });
        self.cursor_hidden = hidden;
    }

    fn fail(&mut self, err: UiError) {
        eprintln!("error: {err}");
        self.fatal_error = Some(err);
        self.app.quit();
    }

    /// Upload the latest frame, or the halt overlay, to the picture widget.
    fn present(&mut self) {
        if let Some(fallback) = self.core.settle_presentation() {
            menu::set_state(&self.app, "filter", fallback.to_string().to_variant());
        }
        let core = &self.core;
        let overlay_frame;
        let (frame, profile) = if let Some(message) = &core.halt_message {
            let (width, height) = core.system.framebuffer_size(&core.runner.runtime);
            overlay_frame = CapturedFrame {
                signal: None,
                timestamp: MachineTime::new(0),
                format: PixelFormat::Rgba8888,
                width,
                height,
                palette: None,
                pixels: overlay::build_halt_overlay(width, height, message),
            };
            let mut profile = core.presentation;
            profile.filter = VideoFilter::Raw;
            (&overlay_frame, profile)
        } else if let Some(frame) = core.runner.frame() {
            (frame, core.presentation)
        } else {
            return;
        };
        // Raw is drawn by GTK itself, which is exact at whole scales and
        // costs nothing. Every other filter needs the wgpu presenter.
        if profile.filter != VideoFilter::Raw
            && let Some(texture) = self.filters.compose(&self.screen, frame, &profile)
        {
            self.screen.show_composed(
                texture,
                (frame.width, frame.height),
                profile.pixel_aspect_ratio,
            );
            return;
        }
        let pixels = match emu198x_native_video::frame_rgba_pixels(frame, &mut self.scratch) {
            Ok(pixels) => pixels,
            Err(err) => {
                eprintln!("video: cannot show this frame: {err}");
                return;
            }
        };
        let (Ok(width), Ok(height)) = (i32::try_from(frame.width), i32::try_from(frame.height))
        else {
            return;
        };
        let texture = gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from(pixels),
            frame.width as usize * 4,
        );
        self.screen
            .show_frame(texture.upcast(), profile.pixel_aspect_ratio);
    }
}

/// The video filters: the wgpu presenter, rendering offscreen at the picture
/// widget's size, made the first time a filter other than raw is chosen.
#[derive(Default)]
struct Filters {
    presenter: Option<WgpuVideoPresenter>,
    /// Set once the presenter could not be made, so it is not tried on every
    /// frame. The host then shows the raw picture.
    unavailable: bool,
    /// The last error a frame hit, reported once until it changes.
    last_error: Option<String>,
}

impl Filters {
    /// Compose `frame` for the whole widget, or `None` to fall back to raw.
    fn compose(
        &mut self,
        screen: &Screen,
        frame: &CapturedFrame,
        profile: &PresentationProfile,
    ) -> Option<gdk::Texture> {
        if self.unavailable {
            return None;
        }
        let (width, height) = screen.device_size();
        if self.presenter.is_none() {
            match WgpuVideoPresenter::new_offscreen(width, height, frame.width, frame.height) {
                Ok(presenter) => self.presenter = Some(presenter),
                Err(err) => {
                    eprintln!("video: filters are unavailable, showing the raw picture: {err}");
                    self.unavailable = true;
                    return None;
                }
            }
        }
        let presenter = self.presenter.as_mut()?;
        if presenter.offscreen_pixels().map(|(_, w, h)| (w, h)) != Some((width, height)) {
            presenter.resize_surface(width, height);
        }
        match presenter.present_with_status(frame, profile) {
            Ok(true) => self.last_error = None,
            Ok(false) => return None,
            Err(err) => {
                let message = err.to_string();
                if self.last_error.as_deref() != Some(message.as_str()) {
                    eprintln!("video: {message}; showing the raw picture");
                    self.last_error = Some(message);
                }
                return None;
            }
        }
        let (pixels, width, height) = presenter.offscreen_pixels()?;
        let texture = gdk::MemoryTexture::new(
            i32::try_from(width).ok()?,
            i32::try_from(height).ok()?,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from(pixels),
            width as usize * 4,
        );
        Some(texture.upcast())
    }
}

type SharedHost<S> = Rc<RefCell<Option<GtkHost<S>>>>;

/// Tick the core, then schedule the next tick when it asks for one.
///
/// Ticks run below GTK's redraw priority. At default priority a tick that is
/// always due (the machine catching up, or fast-forward) would starve the
/// redraws, and the window would never show a frame.
fn schedule_tick<S: UiSystem + 'static>(host: SharedHost<S>, after: Duration) {
    let mut host = Some(host);
    glib::timeout_add_local_full(after, glib::Priority::DEFAULT_IDLE, move || {
        let Some(host) = host.take() else {
            return glib::ControlFlow::Break;
        };
        // A dialog's nested main loop can fire this while the host is
        // already borrowed; try again shortly instead of re-entering.
        let wake = match host.try_borrow_mut() {
            Ok(mut guard) => match guard.as_mut() {
                Some(host) => host.tick(),
                None => None,
            },
            Err(_) => Some(Wake::At(Instant::now() + Duration::from_millis(5))),
        };
        match wake {
            Some(Wake::Now) => schedule_tick(host, Duration::ZERO),
            Some(Wake::At(at)) => {
                schedule_tick(host, at.saturating_duration_since(Instant::now()));
            }
            None => {}
        }
        glib::ControlFlow::Break
    });
}

/// Run `core` in a GTK window until it closes. Returns the core, for the
/// system's teardown, and the error that ended the session, if any.
pub(crate) fn run<S: UiSystem + 'static>(
    core: HostCore<S>,
    _video: VideoFilter,
) -> Result<(HostCore<S>, Option<UiError>), UiError> {
    if gtk4::init().is_err() {
        return Err(UiError::Host(
            "GTK could not start; is a display available?".to_owned(),
        ));
    }
    let app = gtk4::Application::builder()
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let host: SharedHost<S> = Rc::new(RefCell::new(None));
    let pending = Rc::new(RefCell::new(Some(core)));
    {
        let host = host.clone();
        app.connect_activate(move |app| {
            let Some(core) = pending.take() else {
                return;
            };
            open_window(app, core, &host);
            schedule_tick(host.clone(), Duration::ZERO);
        });
    }
    app.run_with_args::<&str>(&[]);
    let mut finished = host
        .take()
        .ok_or_else(|| UiError::Host("the GTK window never opened".to_owned()))?;
    let error = finished.fatal_error.take();
    Ok((finished.core, error))
}

fn open_window<S: UiSystem + 'static>(
    app: &gtk4::Application,
    mut core: HostCore<S>,
    host: &SharedHost<S>,
) {
    let commands = core.command_sender();
    let system = &core.system;
    let runtime = &core.runner.runtime;
    let variants = system.variants();
    let current_variant = system.current_variant();
    let drive_ports = system.drive_ports(runtime);
    let title = system.window_title();
    let bar = menu::build(
        app,
        &menu::MachineInfo {
            title: &title,
            media_slots: &runtime.profile().media_slots,
            variants: &variants,
            current_variant: current_variant.as_deref(),
            drive_ports: &drive_ports,
            host_keyboard: system
                .supports_host_keyboard()
                .then_some(system.supports_host_keyboard()),
            state_open: system.state_open_filter().is_some(),
            tape_export: system.tape_export_filter().is_some(),
            scale: core.scale,
            filter: core.presentation.filter,
        },
        &commands,
    );
    app.set_menubar(Some(&bar));

    core.update_pixel_aspect();
    let screen = Screen::default();
    screen.set_hexpand(true);
    screen.set_vexpand(true);
    screen.set_focusable(true);
    screen.set_sizes(core.display_size(), core.picture_size(core.scale));
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title(core.window_title())
        .show_menubar(true)
        .child(&screen)
        .build();

    connect_input(&window, &screen, host);
    {
        let app = app.clone();
        window.connect_close_request(move |_| {
            app.quit();
            glib::Propagation::Proceed
        });
    }

    core.restart_pacing();
    let mut new_host = GtkHost {
        core,
        app: app.clone(),
        window: window.clone(),
        screen: screen.clone(),
        scratch: Vec::new(),
        filters: Filters::default(),
        held_keys: HashSet::new(),
        cursor_hidden: false,
        fatal_error: None,
    };
    new_host.present();
    host.replace(Some(new_host));
    window.present();
    screen.grab_focus();
}

/// Route keyboard, focus and pointer events into the core.
fn connect_input<S: UiSystem + 'static>(
    window: &gtk4::ApplicationWindow,
    screen: &Screen,
    host: &SharedHost<S>,
) {
    // Keys, in the capture phase so the machine sees F10 and the rest before
    // GTK's own window shortcuts do.
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let host = host.clone();
        keys.connect_key_pressed(move |_, keyval, keycode, _| {
            key_event(&host, keyval, keycode, true);
            glib::Propagation::Stop
        });
    }
    {
        let host = host.clone();
        keys.connect_key_released(move |_, keyval, keycode, _| {
            key_event(&host, keyval, keycode, false);
        });
    }
    {
        let host = host.clone();
        keys.connect_modifiers(move |_, state| {
            if let Ok(mut guard) = host.try_borrow_mut()
                && let Some(host) = guard.as_mut()
            {
                host.core.modifiers_changed(keys::modifiers(state));
            }
            glib::Propagation::Proceed
        });
    }
    window.add_controller(keys);

    {
        let host = host.clone();
        window.connect_is_active_notify(move |window| {
            if !window.is_active()
                && let Ok(mut guard) = host.try_borrow_mut()
                && let Some(host) = guard.as_mut()
            {
                host.held_keys.clear();
                host.core.focus_lost();
                host.sync_host_cursor();
            }
        });
    }

    let motion = gtk4::EventControllerMotion::new();
    {
        let host = host.clone();
        let screen = screen.clone();
        motion.connect_motion(move |_, x, y| {
            if let Ok(mut guard) = host.try_borrow_mut()
                && let Some(host) = guard.as_mut()
            {
                // The core maps window coordinates onto the framebuffer, so
                // give it the picture's own rectangle, not the letterbox.
                let (x, y, size) = match screen.picture_rect() {
                    Some(rect) => (
                        x - f64::from(rect.x()),
                        y - f64::from(rect.y()),
                        Some((rect.width().round() as u32, rect.height().round() as u32)),
                    ),
                    None => (x, y, None),
                };
                host.core.cursor_moved(x, y, size);
                host.sync_host_cursor();
            }
        });
    }
    {
        let host = host.clone();
        motion.connect_leave(move |_| {
            if let Ok(mut guard) = host.try_borrow_mut()
                && let Some(host) = guard.as_mut()
            {
                host.core.cursor_left();
                host.sync_host_cursor();
            }
        });
    }
    screen.add_controller(motion);

    let clicks = gtk4::GestureClick::new();
    clicks.set_button(0);
    {
        let host = host.clone();
        clicks.connect_pressed(move |gesture, _, _, _| {
            mouse_event(&host, gesture.current_button(), true);
        });
    }
    {
        let host = host.clone();
        clicks.connect_released(move |gesture, _, _, _| {
            mouse_event(&host, gesture.current_button(), false);
        });
    }
    screen.add_controller(clicks);
}

fn key_event<S: UiSystem + 'static>(
    host: &SharedHost<S>,
    keyval: gdk::Key,
    keycode: u32,
    pressed: bool,
) {
    let Some(code) = keys::key_code(keycode) else {
        return;
    };
    let logical = keys::logical_key(keyval);
    if let Ok(mut guard) = host.try_borrow_mut()
        && let Some(host) = guard.as_mut()
    {
        // GTK sends auto-repeat as further presses of a key already down.
        let repeat = if pressed {
            !host.held_keys.insert(code)
        } else {
            host.held_keys.remove(&code);
            false
        };
        host.with_shell(|core, shell| core.key_event(shell, code, &logical, pressed, repeat));
    }
}

fn mouse_event<S: UiSystem + 'static>(host: &SharedHost<S>, button: u32, pressed: bool) {
    if let Ok(mut guard) = host.try_borrow_mut()
        && let Some(host) = guard.as_mut()
    {
        host.core.mouse_button(keys::mouse_button(button), pressed);
    }
}

/// Whether to open the GTK window rather than the winit one. GTK is the
/// default when built in; `EMU198X_UI=winit` picks the winit window.
pub(crate) fn chosen() -> bool {
    std::env::var("EMU198X_UI").map_or(true, |value| value != "winit")
}
