//! The window: a GTK application window around the picture, driven by the
//! machine binary's messages.
//!
//! A thread reads the messages from standard input and wakes GTK's main
//! loop for each one; the main loop applies them below GTK's redraw priority,
//! so a machine sending frames as fast as it can never starves the redraws.
//! When several frames are waiting only the latest is drawn. Input, menu
//! commands and dialog answers go back through the [`Outbox`].

use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, BufReader, Stdin};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use emu198x_native_video::PresentationProfile;
use emu198x_shell::CapturedFrame;
use emu198x_ui::window_protocol::{
    self, FrameSettings, FromWindow, Level, MarkChange, PROTOCOL_VERSION, ToWindow, WindowSetup,
};
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use winit::keyboard::KeyCode;

use crate::outbox::Outbox;
use crate::picture::{Filters, nearest_into};
use crate::screen::{self, Screen};
use crate::{keys, menu};

type Shared = Rc<RefCell<Window>>;

thread_local! {
    /// The open window, for the wake-ups the reader thread queues.
    static WINDOW: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

/// Everything the window keeps between messages and events.
struct Window {
    app: gtk4::Application,
    window: gtk4::ApplicationWindow,
    screen: Screen,
    out: Outbox,
    /// The machine binary's messages; `None` means it has gone.
    inbox: Receiver<Option<ToWindow>>,
    filters: Filters,
    /// RGBA scratch for indexed frames.
    scratch: Vec<u8>,
    /// Physical keys held, so GTK's auto-repeat presses reach the machine as
    /// repeats, as winit reports them.
    held_keys: HashSet<KeyCode>,
    /// The file chooser on screen, kept alive until it answers.
    chooser: Option<gtk4::FileChooserNative>,
}

/// Run the window until the machine binary closes it or the user does.
pub(crate) fn main() -> ExitCode {
    let mut input = BufReader::new(io::stdin());
    let out = Outbox::new();
    let setup = match window_protocol::read_message::<ToWindow>(&mut input) {
        Ok(Some(ToWindow::Open(setup))) => setup,
        Ok(Some(_)) => {
            eprintln!("emu198x-gtk-window: the first message was not the window's setup");
            return ExitCode::FAILURE;
        }
        Ok(None) => return ExitCode::FAILURE,
        Err(err) => {
            eprintln!("emu198x-gtk-window: cannot read the window's setup: {err}");
            return ExitCode::FAILURE;
        }
    };
    if setup.version != PROTOCOL_VERSION {
        eprintln!(
            "emu198x-gtk-window: the machine speaks window protocol {}, this window {PROTOCOL_VERSION}",
            setup.version
        );
        out.send(&FromWindow::Incompatible {
            version: PROTOCOL_VERSION,
        });
        return ExitCode::FAILURE;
    }
    if gtk4::init().is_err() {
        eprintln!("emu198x-gtk-window: GTK could not start; is a display available?");
        return ExitCode::FAILURE;
    }

    let (inbox_tx, inbox) = channel();
    spawn_reader(input, inbox_tx);
    let app = gtk4::Application::builder()
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let pending = RefCell::new(Some((setup, out, inbox)));
    app.connect_activate(move |app| {
        if let Some((setup, out, inbox)) = pending.take() {
            open(app, &setup, out, inbox);
        }
    });
    app.run_with_args::<&str>(&[]);
    WINDOW.with_borrow_mut(Option::take);
    ExitCode::SUCCESS
}

/// Read the machine binary's messages on a thread of their own, waking the
/// main loop for each.
fn spawn_reader(mut input: BufReader<Stdin>, inbox: Sender<Option<ToWindow>>) {
    std::thread::spawn(move || {
        loop {
            let message = match window_protocol::read_message::<ToWindow>(&mut input) {
                Ok(message) => message,
                Err(err) => {
                    eprintln!("emu198x-gtk-window: {err}");
                    None
                }
            };
            let gone = message.is_none();
            if inbox.send(message).is_err() {
                return;
            }
            glib::idle_add_full(glib::Priority::DEFAULT_IDLE, || {
                drain();
                glib::ControlFlow::Break
            });
            if gone {
                return;
            }
        }
    });
}

/// Apply every message waiting, then draw the latest frame among them.
fn drain() {
    let Some(shared) = WINDOW.with_borrow(Clone::clone) else {
        return;
    };
    let Ok(mut window) = shared.try_borrow_mut() else {
        // An event handler has the window; come back once it is done.
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, || {
            drain();
            glib::ControlFlow::Break
        });
        return;
    };
    let mut latest = None;
    loop {
        match window.inbox.try_recv() {
            Ok(Some(ToWindow::Frame { frame, settings })) => latest = Some((frame, settings)),
            Ok(Some(message)) => window.apply(message, &shared),
            Ok(None) | Err(TryRecvError::Disconnected) => {
                window.quit();
                return;
            }
            Err(TryRecvError::Empty) => break,
        }
    }
    if let Some((frame, settings)) = latest {
        window.present(&frame, settings);
    }
}

impl Window {
    fn apply(&mut self, message: ToWindow, shared: &Shared) {
        match message {
            ToWindow::Open(_) | ToWindow::Frame { .. } => {}
            ToWindow::Title(title) => self.window.set_title(Some(&title)),
            ToWindow::PictureSize { width, height } => {
                // The window keeps the menu bar's height on top of the picture.
                let extra = self.window.height() - self.screen.height();
                self.window
                    .set_default_size(width.round() as i32, height.round() as i32 + extra.max(0));
            }
            ToWindow::ResetVideoHistory => {
                if let Some(video) = self.filters.presenter.as_mut() {
                    video.reset_temporal_history();
                }
            }
            ToWindow::Mark(change) => self.mark(change),
            ToWindow::CursorVisible(visible) => {
                self.screen
                    .set_cursor_from_name(if visible { None } else { Some("none") })
            }
            ToWindow::PickFile {
                title,
                filter_label,
                extensions,
                save_as,
                all_files,
            } => self.pick_file(
                &title,
                &filter_label,
                &extensions,
                save_as.as_deref(),
                all_files,
                shared,
            ),
            ToWindow::Message { title, level, text } => self.message(&title, level, &text),
            ToWindow::Close => self.quit(),
        }
    }

    /// Take the window off the screen at once, then end the main loop.
    /// Process teardown can take a moment in some graphics drivers, and the
    /// machine binary has already gone.
    fn quit(&self) {
        self.window.destroy();
        self.app.quit();
    }

    /// Move a menu mark the core has moved.
    fn mark(&self, change: MarkChange) {
        match change {
            MarkChange::Scale(scale) => {
                menu::set_state(&self.app, "scale", scale.to_string().to_variant());
            }
            MarkChange::Filter(filter) => {
                menu::set_state(&self.app, "filter", filter.to_string().to_variant());
            }
            MarkChange::Variant(id) => menu::set_state(&self.app, "variant", id.to_variant()),
            MarkChange::PortDrive { device, kind_id } => {
                menu::set_state(&self.app, &menu::drive_action(device), kind_id.to_variant());
            }
            MarkChange::Turbo(armed) => {
                menu::set_state(&self.app, "fast-forward", armed.to_variant());
            }
            MarkChange::HostKeyboard(host) => menu::set_state(
                &self.app,
                "keyboard",
                if host { "host" } else { "original" }.to_variant(),
            ),
        }
    }

    /// Show a file chooser; its answer goes back as
    /// [`FromWindow::FileChosen`].
    fn pick_file(
        &mut self,
        title: &str,
        filter_label: &str,
        extensions: &[String],
        save_as: Option<&str>,
        all_files: bool,
        shared: &Shared,
    ) {
        let action = if save_as.is_some() {
            gtk4::FileChooserAction::Save
        } else {
            gtk4::FileChooserAction::Open
        };
        let dialog =
            gtk4::FileChooserNative::new(Some(title), Some(&self.window), action, None, None);
        if let Some(name) = save_as {
            dialog.set_current_name(name);
        }
        let main = gtk4::FileFilter::new();
        main.set_name(Some(filter_label));
        for extension in extensions {
            main.add_suffix(extension);
        }
        dialog.add_filter(&main);
        if all_files {
            let all = gtk4::FileFilter::new();
            all.set_name(Some("All files"));
            all.add_pattern("*");
            dialog.add_filter(&all);
        }
        let out = self.out.clone();
        let shared = Rc::downgrade(shared);
        dialog.connect_response(move |dialog, response| {
            let path = (response == gtk4::ResponseType::Accept)
                .then(|| dialog.file().and_then(|file| file.path()))
                .flatten();
            out.send(&FromWindow::FileChosen(path));
            if let Some(shared) = shared.upgrade()
                && let Ok(mut window) = shared.try_borrow_mut()
            {
                window.chooser = None;
            }
        });
        dialog.show();
        self.chooser = Some(dialog);
    }

    /// Show a message box; its closing goes back as
    /// [`FromWindow::MessageClosed`].
    fn message(&self, title: &str, level: Level, text: &str) {
        let dialog = gtk4::MessageDialog::new(
            Some(&self.window),
            gtk4::DialogFlags::MODAL,
            match level {
                Level::Info => gtk4::MessageType::Info,
                Level::Warning => gtk4::MessageType::Warning,
                Level::Error => gtk4::MessageType::Error,
            },
            gtk4::ButtonsType::Ok,
            title,
        );
        dialog.set_secondary_text(Some(text));
        let out = self.out.clone();
        let answered = RefCell::new(false);
        dialog.connect_response(move |dialog, _| {
            dialog.close();
            if !answered.replace(true) {
                out.send(&FromWindow::MessageClosed);
            }
        });
        dialog.present();
    }

    /// Show `frame` in the picture widget.
    fn present(&mut self, frame: &CapturedFrame, settings: FrameSettings) {
        let profile = PresentationProfile {
            filter: settings.filter,
            pixel_aspect_ratio: settings.pixel_aspect_ratio,
            clock_hz: settings.clock_hz,
            phosphor_tau_ms: settings.phosphor_tau_ms,
            ..PresentationProfile::default()
        };
        if let Some(texture) = self.filters.compose(&self.screen, frame, &profile) {
            self.screen.show_composed(
                texture,
                (frame.width, frame.height),
                profile.pixel_aspect_ratio,
            );
            return;
        }
        // No GPU: scale the raw frame by nearest neighbour into a picture
        // the size of the widget, letterboxed in black.
        let pixels = match emu198x_native_video::frame_rgba_pixels(frame, &mut self.scratch) {
            Ok(pixels) => pixels,
            Err(err) => {
                eprintln!("video: cannot show this frame: {err}");
                return;
            }
        };
        let (width, height) = self.screen.device_size();
        let rect = screen::fit(
            i32::try_from(frame.width).unwrap_or(0),
            i32::try_from(frame.height).unwrap_or(0),
            profile.pixel_aspect_ratio,
            width as f32,
            height as f32,
        );
        let composed = nearest_into(pixels, frame.width, frame.height, width, height, &rect);
        let (Ok(texture_w), Ok(texture_h)) = (i32::try_from(width), i32::try_from(height)) else {
            return;
        };
        let texture = gdk::MemoryTexture::new(
            texture_w,
            texture_h,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(composed),
            width as usize * 4,
        );
        self.screen.show_composed(
            texture.upcast(),
            (frame.width, frame.height),
            profile.pixel_aspect_ratio,
        );
    }
}

/// Build the window and its menus from `setup`, then tell the machine binary
/// it is ready.
fn open(
    app: &gtk4::Application,
    setup: &WindowSetup,
    out: Outbox,
    inbox: Receiver<Option<ToWindow>>,
) {
    let bar = menu::build(app, setup, &out);
    app.set_menubar(Some(&bar));

    let screen = Screen::default();
    screen.set_hexpand(true);
    screen.set_vexpand(true);
    screen.set_focusable(true);
    screen.set_sizes(setup.minimum_size, setup.natural_size);
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title(setup.title.as_str())
        .show_menubar(true)
        .child(&screen)
        .build();

    let shared: Shared = Rc::new(RefCell::new(Window {
        app: app.clone(),
        window: window.clone(),
        screen: screen.clone(),
        out: out.clone(),
        inbox,
        filters: Filters::default(),
        scratch: Vec::new(),
        held_keys: HashSet::new(),
        chooser: None,
    }));
    connect_input(&window, &screen, &shared);
    {
        let app = app.clone();
        let out = out.clone();
        window.connect_close_request(move |_| {
            out.send(&FromWindow::Closed);
            app.quit();
            glib::Propagation::Proceed
        });
    }
    WINDOW.with_borrow_mut(|slot| *slot = Some(shared));
    window.present();
    screen.grab_focus();
    out.send(&FromWindow::Ready);
    // Anything that arrived while the window was being built.
    drain();
}

/// Send keyboard, focus and pointer events to the machine binary.
fn connect_input(window: &gtk4::ApplicationWindow, screen: &Screen, shared: &Shared) {
    // Keys, in the capture phase so the machine sees F10 and the rest before
    // GTK's own window shortcuts do.
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let shared = shared.clone();
        keys.connect_key_pressed(move |_, keyval, keycode, _| {
            key_event(&shared, keyval, keycode, true);
            glib::Propagation::Stop
        });
    }
    {
        let shared = shared.clone();
        keys.connect_key_released(move |_, keyval, keycode, _| {
            key_event(&shared, keyval, keycode, false);
        });
    }
    {
        let shared = shared.clone();
        keys.connect_modifiers(move |_, state| {
            if let Ok(window) = shared.try_borrow() {
                window
                    .out
                    .send(&FromWindow::Modifiers(keys::modifiers(state)));
            }
            glib::Propagation::Proceed
        });
    }
    window.add_controller(keys);

    {
        let shared = shared.clone();
        window.connect_is_active_notify(move |window| {
            if !window.is_active()
                && let Ok(mut host) = shared.try_borrow_mut()
            {
                host.held_keys.clear();
                host.out.send(&FromWindow::FocusLost);
            }
        });
    }

    let motion = gtk4::EventControllerMotion::new();
    {
        let shared = shared.clone();
        let screen = screen.clone();
        motion.connect_motion(move |_, x, y| {
            let Ok(window) = shared.try_borrow() else {
                return;
            };
            // The core maps window coordinates onto the framebuffer, so give
            // it the picture's own rectangle, not the letterbox.
            let (x, y, size) = match screen.picture_rect() {
                Some(rect) => (
                    x - f64::from(rect.x()),
                    y - f64::from(rect.y()),
                    Some((rect.width().round() as u32, rect.height().round() as u32)),
                ),
                None => (x, y, None),
            };
            window.out.send(&FromWindow::Cursor { x, y, size });
        });
    }
    {
        let shared = shared.clone();
        motion.connect_leave(move |_| {
            if let Ok(window) = shared.try_borrow() {
                window.out.send(&FromWindow::CursorLeft);
            }
        });
    }
    screen.add_controller(motion);

    let clicks = gtk4::GestureClick::new();
    clicks.set_button(0);
    {
        let shared = shared.clone();
        clicks.connect_pressed(move |gesture, _, _, _| {
            mouse_event(&shared, gesture.current_button(), true);
        });
    }
    {
        let shared = shared.clone();
        clicks.connect_released(move |gesture, _, _, _| {
            mouse_event(&shared, gesture.current_button(), false);
        });
    }
    screen.add_controller(clicks);
}

fn key_event(shared: &Shared, keyval: gdk::Key, keycode: u32, pressed: bool) {
    let Some(code) = keys::key_code(keycode) else {
        return;
    };
    let logical = keys::logical_key(keyval);
    if let Ok(mut window) = shared.try_borrow_mut() {
        // GTK sends auto-repeat as further presses of a key already down.
        let repeat = if pressed {
            !window.held_keys.insert(code)
        } else {
            window.held_keys.remove(&code);
            false
        };
        window.out.send(&FromWindow::Key {
            code,
            logical,
            pressed,
            repeat,
        });
    }
}

fn mouse_event(shared: &Shared, button: u32, pressed: bool) {
    if let Ok(window) = shared.try_borrow() {
        window.out.send(&FromWindow::MouseButton {
            button: keys::mouse_button(button),
            pressed,
        });
    }
}
