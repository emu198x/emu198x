//! Running a machine with its window in the GTK window process.
//!
//! The machine binary keeps the [`HostCore`]: pacing, input, commands,
//! media and save states all stay here. The window process, started as a
//! child, only draws and reports input, so this binary never links GTK. If
//! the window process is missing, cannot start, or speaks another protocol
//! version, [`run`] hands the core back and the caller opens the winit
//! window instead. See `knowledge/decisions/gtk-window-process.md`.

use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use emu198x_shell::{CapturedFrame, MachineCore, MachineTime, PixelFormat};

use crate::host_core::{FileFilter, HostCore, MenuSync, Notice, Shell, Wake};
use crate::menu::AppCommand;
use crate::window_protocol::{
    self, DrivePortEntry, FrameSettings, FromWindow, Level, MarkChange, PROTOCOL_VERSION, SlotInfo,
    ToWindow, VariantEntry, WindowSetup,
};
use crate::{UiError, UiSystem, VideoFilter, overlay};

/// The window program's name, looked for beside the running binary.
const WINDOW_PROGRAM: &str = "emu198x-gtk-window";

/// How long the window may take to open before this binary gives up on it.
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

/// What became of a session.
pub(crate) enum Outcome<S: UiSystem> {
    /// The window ran and has closed: the core, for teardown, and the error
    /// that ended the session, if any.
    Finished(HostCore<S>, Option<UiError>),
    /// There is no window process to use; open another window instead.
    Unavailable(HostCore<S>),
}

/// Where the window program is: `$EMU198X_GTK_WINDOW`, or beside this
/// binary. `None` when it is not there, or `EMU198X_UI=winit` asks for the
/// winit window.
fn window_program() -> Option<PathBuf> {
    if std::env::var("EMU198X_UI").is_ok_and(|ui| ui == "winit") {
        return None;
    }
    if let Some(path) = std::env::var_os("EMU198X_GTK_WINDOW").filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let path = std::env::current_exe().ok()?.parent()?.join(WINDOW_PROGRAM);
    path.is_file().then_some(path)
}

/// Run `core` in the GTK window process until the window closes.
pub(crate) fn run<S: UiSystem>(mut core: HostCore<S>, video: VideoFilter) -> Outcome<S> {
    let Some(program) = window_program() else {
        return Outcome::Unavailable(core);
    };
    let Ok(mut child) = Command::new(&program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    else {
        eprintln!(
            "window: cannot start {}; using the built-in window",
            program.display()
        );
        return Outcome::Unavailable(core);
    };
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        stop(&mut child);
        return Outcome::Unavailable(core);
    };

    let (events_tx, events) = channel();
    spawn_reader(stdout, events_tx);
    let mut shell = RemoteShell {
        writer: BufWriter::new(stdin),
        events,
        deferred: Vec::new(),
        redraw: false,
        closed: false,
    };

    core.update_pixel_aspect();
    let opened = shell.send(&ToWindow::Open(Box::new(setup(&core, video))))
        && matches!(
            shell.events.recv_timeout(OPEN_TIMEOUT),
            Ok(Some(FromWindow::Ready))
        );
    if !opened {
        eprintln!(
            "window: {} did not open; using the built-in window",
            program.display()
        );
        stop(&mut child);
        return Outcome::Unavailable(core);
    }

    core.restart_pacing();
    let commands = core.command_sender();
    send_frame(&mut core, &mut shell);
    let error = drive(&mut core, &mut shell, &commands).err();
    let _ = shell.send(&ToWindow::Close);
    drop(shell);
    let _ = child.wait();
    Outcome::Finished(core, error)
}

/// The session loop: apply window events, tick the core, send frames, and
/// sleep until the next tick or event.
fn drive<S: UiSystem>(
    core: &mut HostCore<S>,
    shell: &mut RemoteShell,
    commands: &Sender<AppCommand>,
) -> Result<(), UiError> {
    let mut wake = Wake::Now;
    let mut cursor_visible = true;
    loop {
        // Events that arrived while a dialog was open come first.
        for event in std::mem::take(&mut shell.deferred) {
            handle_event(core, shell, commands, event);
        }
        if shell.closed {
            return Ok(());
        }
        let timeout = match wake {
            Wake::Now => Duration::ZERO,
            Wake::At(at) => at.saturating_duration_since(Instant::now()),
        };
        match shell.events.recv_timeout(timeout) {
            Ok(Some(event)) => {
                handle_event(core, shell, commands, event);
                // Take every event already waiting before the next tick.
                while let Ok(next) = shell.events.try_recv() {
                    match next {
                        Some(event) => handle_event(core, shell, commands, event),
                        None => shell.closed = true,
                    }
                }
            }
            Ok(None) | Err(RecvTimeoutError::Disconnected) => shell.closed = true,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if shell.closed {
            return Ok(());
        }
        wake = core.tick(shell)?;
        if std::mem::take(&mut shell.redraw) {
            send_frame(core, shell);
        }
        // Pointer events and ticks both move this; tell the window only on
        // a change.
        let visible = core.host_cursor_visible();
        if visible != cursor_visible {
            shell.send(&ToWindow::CursorVisible(visible));
            cursor_visible = visible;
        }
    }
}

fn handle_event<S: UiSystem>(
    core: &mut HostCore<S>,
    shell: &mut RemoteShell,
    commands: &Sender<AppCommand>,
    event: FromWindow,
) {
    match event {
        FromWindow::Key {
            code,
            logical,
            pressed,
            repeat,
        } => core.key_event(shell, code, &logical, pressed, repeat),
        FromWindow::Modifiers(modifiers) => core.modifiers_changed(modifiers),
        FromWindow::FocusLost => core.focus_lost(),
        FromWindow::Cursor { x, y, size } => core.cursor_moved(x, y, size),
        FromWindow::CursorLeft => core.cursor_left(),
        FromWindow::MouseButton { button, pressed } => core.mouse_button(button, pressed),
        FromWindow::Command(command) => {
            let _ = commands.send(command);
        }
        FromWindow::Closed => shell.closed = true,
        // Replies arrive only while a dialog is waiting for them, and the
        // handshake is over.
        FromWindow::FileChosen(_)
        | FromWindow::MessageClosed
        | FromWindow::Ready
        | FromWindow::Incompatible { .. } => {}
    }
}

/// Send the latest frame, or the halt overlay, with its presentation.
fn send_frame<S: UiSystem>(core: &mut HostCore<S>, shell: &mut RemoteShell) {
    if let Some(fallback) = core.settle_presentation() {
        shell.send(&ToWindow::Mark(MarkChange::Filter(fallback)));
    }
    let mut settings = FrameSettings {
        filter: core.presentation.filter,
        pixel_aspect_ratio: core.presentation.pixel_aspect_ratio,
        clock_hz: core.presentation.clock_hz,
        phosphor_tau_ms: core.presentation.phosphor_tau_ms,
    };
    let frame = if let Some(message) = &core.halt_message {
        let (width, height) = core.system.framebuffer_size(&core.runner.runtime);
        settings.filter = VideoFilter::Raw;
        CapturedFrame {
            signal: None,
            timestamp: MachineTime::new(0),
            format: PixelFormat::Rgba8888,
            width,
            height,
            palette: None,
            pixels: overlay::build_halt_overlay(width, height, message),
        }
    } else if let Some(frame) = core.runner.frame() {
        frame.clone()
    } else {
        return;
    };
    shell.send(&ToWindow::Frame {
        frame: Box::new(frame),
        settings,
    });
}

/// The window's opening message, from the machine as it is now.
fn setup<S: UiSystem>(core: &HostCore<S>, video: VideoFilter) -> WindowSetup {
    let system = &core.system;
    let runtime = &core.runner.runtime;
    WindowSetup {
        version: PROTOCOL_VERSION,
        title: core.window_title(),
        media_slots: runtime
            .profile()
            .media_slots
            .iter()
            .map(|slot| SlotInfo {
                id: slot.id.to_string(),
                display_name: slot.display_name.to_string(),
                kind: slot.kind,
            })
            .collect(),
        variants: system.variants().iter().map(VariantEntry::from).collect(),
        current_variant: system.current_variant().map(|id| id.to_string()),
        drive_ports: system
            .drive_ports(runtime)
            .iter()
            .map(DrivePortEntry::from)
            .collect(),
        host_keyboard: system
            .supports_host_keyboard()
            .then_some(system.supports_host_keyboard()),
        state_open: system.state_open_filter().is_some(),
        tape_export: system.tape_export_filter().is_some(),
        scale: core.scale,
        filter: video,
        minimum_size: core.display_size(),
        natural_size: core.picture_size(core.scale),
    }
}

/// Read the window's messages on a thread of their own. `None` on the
/// channel means the window has gone.
fn spawn_reader(stdout: std::process::ChildStdout, events: Sender<Option<FromWindow>>) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match window_protocol::read_message::<FromWindow>(&mut reader) {
                Ok(Some(message)) => {
                    if events.send(Some(message)).is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    let _ = events.send(None);
                    return;
                }
                Err(err) => {
                    eprintln!("window: {err}");
                    let _ = events.send(None);
                    return;
                }
            }
        }
    });
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The window process, as the core's [`Shell`].
struct RemoteShell {
    writer: BufWriter<ChildStdin>,
    events: Receiver<Option<FromWindow>>,
    /// Events that arrived while a dialog was waiting for its answer.
    deferred: Vec<FromWindow>,
    /// Set when the core asks for a redraw; the frame goes after the tick.
    redraw: bool,
    /// The window has closed, or its pipe has.
    closed: bool,
}

impl RemoteShell {
    /// Send one message. `false`, and the session is over, if the pipe has
    /// closed.
    fn send(&mut self, message: &ToWindow) -> bool {
        if self.closed {
            return false;
        }
        if window_protocol::write_message(&mut self.writer, message).is_err() {
            self.closed = true;
        }
        !self.closed
    }

    /// Wait for the reply a dialog request asked for, keeping any other
    /// event for later. `None` if the window went away first.
    fn wait_for<T>(&mut self, mut reply: impl FnMut(&FromWindow) -> Option<T>) -> Option<T> {
        loop {
            match self.events.recv() {
                Ok(Some(event)) => {
                    if let Some(value) = reply(&event) {
                        return Some(value);
                    }
                    if event == FromWindow::Closed {
                        self.closed = true;
                        return None;
                    }
                    self.deferred.push(event);
                }
                Ok(None) | Err(_) => {
                    self.closed = true;
                    return None;
                }
            }
        }
    }

    fn pick(
        &mut self,
        title: &str,
        (label, extensions): FileFilter<'_>,
        save_as: Option<&str>,
        all_files: bool,
    ) -> Option<PathBuf> {
        let request = ToWindow::PickFile {
            title: title.to_owned(),
            filter_label: label.to_owned(),
            extensions: extensions.iter().map(|ext| (*ext).to_owned()).collect(),
            save_as: save_as.map(ToOwned::to_owned),
            all_files,
        };
        if !self.send(&request) {
            return None;
        }
        self.wait_for(|event| match event {
            FromWindow::FileChosen(path) => Some(path.clone()),
            _ => None,
        })
        .flatten()
    }
}

impl Shell for RemoteShell {
    fn set_title(&mut self, title: &str) {
        self.send(&ToWindow::Title(title.to_owned()));
    }

    fn request_redraw(&mut self) {
        self.redraw = true;
    }

    fn request_picture_size(&mut self, width: f64, height: f64) {
        self.send(&ToWindow::PictureSize { width, height });
    }

    fn framebuffer_resized(&mut self, _width: u32, _height: u32) -> Result<(), UiError> {
        // The window sizes its picture from each frame.
        Ok(())
    }

    fn reset_video_history(&mut self) {
        self.send(&ToWindow::ResetVideoHistory);
    }

    fn sync_menu(&mut self, change: MenuSync<'_>) {
        let mark = match change {
            MenuSync::Scale(scale) => MarkChange::Scale(scale),
            MenuSync::Filter(filter) => MarkChange::Filter(filter),
            MenuSync::Variant(id) => MarkChange::Variant(id.to_owned()),
            MenuSync::PortDrive { device, kind_id } => MarkChange::PortDrive {
                device,
                kind_id: kind_id.to_owned(),
            },
            MenuSync::Turbo(armed) => MarkChange::Turbo(armed),
            MenuSync::HostKeyboard(host) => MarkChange::HostKeyboard(host),
        };
        self.send(&ToWindow::Mark(mark));
    }

    fn pick_file(&mut self, title: &str, filter: FileFilter<'_>) -> Option<PathBuf> {
        self.pick(title, filter, None, true)
    }

    fn pick_save_file(
        &mut self,
        title: &str,
        file_name: &str,
        filter: FileFilter<'_>,
        all_files: bool,
    ) -> Option<PathBuf> {
        self.pick(title, filter, Some(file_name), all_files)
    }

    fn message(&mut self, title: &str, level: Notice, text: &str) {
        let request = ToWindow::Message {
            title: title.to_owned(),
            level: match level {
                Notice::Info => Level::Info,
                Notice::Warning => Level::Warning,
                Notice::Error => Level::Error,
            },
            text: text.to_owned(),
        };
        if self.send(&request) {
            self.wait_for(|event| (*event == FromWindow::MessageClosed).then_some(()));
        }
    }
}
