//! The window-free half of a native host.
//!
//! [`HostCore`] holds everything a window needs to host one machine except
//! the window itself: pacing, the input queues, the command channel, media,
//! save states and variant switches. A host shell (the winit window today,
//! the GTK4 window next) owns the window, the presenter and the menu bar,
//! feeds the core its events, and implements [`Shell`] so the core can ask
//! for the few things only a window can do. See
//! `knowledge/decisions/host-ui-model.md` § 12.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use emu198x_native_video::PresentationProfile;
use emu198x_shell::{
    CapturedFrame, ControlCommand, HostIo, InputEvent, LatestFrameCapture, MachineCore, MediaKind,
    MediaTransportAction, MediaTransportCommand, NativeAudioOutput, NativeGamepadInput,
    NullTraceSink, ResetKind, RunResult,
};
use winit::keyboard::{Key, KeyCode, ModifiersState};

use crate::host_audio::HostAudio;
use crate::menu::AppCommand;
use crate::pointer;
use crate::{HostControl, MouseButton, UiError, UiSystem, VideoFilter, export, keyboard};
use crate::{media_open, quick_load};

const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;
/// Frames per tick while fast-loading a tape (turbo): run unthrottled in
/// bounded bursts so the loader races ahead but the window stays responsive.
const MAX_TURBO_FRAMES: u32 = 32;

/// A menu check mark the core has changed, for the shell to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuSync<'a> {
    Scale(u32),
    Filter(VideoFilter),
    Variant(&'a str),
    PortDrive { device: u8, kind_id: &'a str },
    Turbo(bool),
    HostKeyboard(bool),
}

/// How serious a message box is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Notice {
    Info,
    Warning,
    Error,
}

/// A file-picker filter: a label and its extensions.
pub(crate) type FileFilter<'a> = (&'a str, &'a [&'a str]);

/// What the core asks of the window that hosts it.
pub(crate) trait Shell {
    /// Show `title` as the window title.
    fn set_title(&mut self, title: &str);
    /// Draw the latest frame soon.
    fn request_redraw(&mut self);
    /// Ask for a picture area of `width` × `height` logical pixels.
    fn request_picture_size(&mut self, width: f64, height: f64);
    /// The framebuffer is now `width` × `height`: rebuild what presents it.
    fn framebuffer_resized(&mut self, width: u32, height: u32) -> Result<(), UiError>;
    /// Forget earlier frames (phosphor and other temporal filters).
    fn reset_video_history(&mut self);
    /// Show a changed menu check mark.
    fn sync_menu(&mut self, change: MenuSync<'_>);
    /// Ask for a file to open, offering `filter` and "All files".
    fn pick_file(&mut self, title: &str, filter: FileFilter<'_>) -> Option<PathBuf>;
    /// Ask where to save, suggesting `file_name`. `all_files` adds an
    /// "All files" filter after `filter`.
    fn pick_save_file(
        &mut self,
        title: &str,
        file_name: &str,
        filter: FileFilter<'_>,
        all_files: bool,
    ) -> Option<PathBuf>;
    /// Show a message box.
    fn message(&mut self, title: &str, level: Notice, text: &str);
}

/// When the core next wants a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wake {
    /// Straight away: a fast-forward burst is running.
    Now,
    /// At this instant: the next paced slice.
    At(Instant),
}

/// Drives a [`MachineCore`] runtime a frame (or sub-frame) at a time, capturing
/// the latest frame and feeding framed audio.
pub(crate) struct Runner<R: MachineCore> {
    pub(crate) runtime: R,
    frame_capture: LatestFrameCapture,
    audio_output: HostAudio,
    last_run_result: Option<RunResult>,
}

impl<R: MachineCore> Runner<R> {
    /// A runner playing through the host output device when `audio` is set
    /// and the device opens; silent otherwise (see [`HostAudio::open`]).
    pub(crate) fn new(runtime: R, audio: bool) -> Self {
        Self {
            runtime,
            frame_capture: LatestFrameCapture::default(),
            audio_output: HostAudio::open(audio, || NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)),
            last_run_result: None,
        }
    }

    pub(crate) fn run_ticks(
        &mut self,
        input_events: &[InputEvent],
        ticks: u64,
    ) -> Result<bool, UiError> {
        let previous = self.frame().map(|frame| frame.timestamp);
        let target = self.runtime.time().saturating_add(ticks);
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events,
            frame_sink: &mut self.frame_capture,
            audio_sink: &mut self.audio_output,
            trace_sink: &mut trace_sink,
        };
        self.last_run_result = Some(self.runtime.run_until(target, &mut host)?);
        Ok(self.frame().map(|frame| frame.timestamp) != previous)
    }

    pub(crate) fn frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
    }

    /// Drop the captured frame, queued audio and the last run result, so
    /// nothing from before a reset, load or switch is shown or heard.
    fn clear_capture(&mut self) {
        self.frame_capture = LatestFrameCapture::default();
        self.audio_output.clear();
        self.last_run_result = None;
    }
}

/// Everything a window hosts for one [`UiSystem`], except the window.
pub(crate) struct HostCore<S: UiSystem> {
    pub(crate) system: S,
    pub(crate) runner: Runner<S::Runtime>,
    pub(crate) scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    full_frame_ticks: u64,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, HostControl>,
    held_target_keys: keyboard::HeldKeys,
    host_keyboard: bool,
    keyboard_notice: Option<String>,
    /// Cursor movement in framebuffer coordinates, for relative mouse deltas
    /// on a system that cannot say where its pointer is. Only used when the
    /// system has a mouse.
    relative_motion: pointer::RelativeMotion,
    /// Closed-loop pointer control for a system that can say where its
    /// pointer is ([`UiSystem::guest_pointer`]).
    pointer_sync: pointer::PointerSync,
    /// Mouse buttons currently held, so they can be released on focus loss.
    pressed_mouse_buttons: HashMap<MouseButton, &'static str>,
    /// Host buttons held on an aiming device's port, so focus loss can let
    /// go of a trigger the game would otherwise see stuck down.
    pressed_aim_buttons: HashSet<MouseButton>,
    /// Held modifier keys, tracked so the save/load chords (Cmd/Ctrl+S / +L)
    /// can be distinguished from the bare keys the machine keyboard uses.
    modifiers: ModifiersState,
    gamepads: NativeGamepadInput,
    pub(crate) presentation: PresentationProfile,
    pub(crate) halt_message: Option<String>,
    /// Tape fast-load (turbo) armed by the user (F11 / Tape → Fast-Forward
    /// While Playing). Only races ahead while [`UiSystem::tape_playing`] is
    /// also true.
    turbo_armed: bool,
    /// Command channel shared by menu clicks and keyboard shortcuts; drained at
    /// the frame boundary so an action never tears down state mid-frame.
    command_tx: Sender<AppCommand>,
    command_rx: Receiver<AppCommand>,
}

impl<S: UiSystem> HostCore<S> {
    pub(crate) fn new(
        system: S,
        runner: Runner<S::Runtime>,
        scale: u32,
        video: VideoFilter,
    ) -> Self {
        let slices = system.input_slices_per_frame().max(1);
        let frame_ticks = system.frame_ticks(&runner.runtime);
        let frame_duration = system.frame_duration(&runner.runtime);
        let slice_ticks = frame_ticks.div_ceil(u64::from(slices));
        let slice_duration =
            Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(slices));
        // The pixel aspect ratio is derived from the system's target display
        // aspect and the cropped framebuffer size once the window opens (see
        // `update_pixel_aspect`); square pixels until then.
        let presentation = PresentationProfile::for_filter(video);
        let (command_tx, command_rx) = channel();
        let host_keyboard = system.supports_host_keyboard();
        Self {
            system,
            runner,
            scale,
            slice_ticks,
            slice_duration,
            full_frame_ticks: frame_ticks,
            next_slice_at: Instant::now(),
            pending_inputs: Vec::new(),
            pressed_keys: HashMap::new(),
            held_target_keys: keyboard::HeldKeys::default(),
            host_keyboard,
            keyboard_notice: None,
            relative_motion: pointer::RelativeMotion::default(),
            pointer_sync: pointer::PointerSync::default(),
            pressed_mouse_buttons: HashMap::new(),
            pressed_aim_buttons: HashSet::new(),
            modifiers: ModifiersState::empty(),
            gamepads: NativeGamepadInput::new(),
            presentation,
            halt_message: None,
            turbo_armed: false,
            command_tx,
            command_rx,
        }
    }

    /// A sender for commands from the shell's menu.
    pub(crate) fn command_sender(&self) -> Sender<AppCommand> {
        self.command_tx.clone()
    }

    /// Take the picture's aspect from the machine as it is now. Call before
    /// sizing a new window.
    pub(crate) fn update_pixel_aspect(&mut self) {
        self.presentation.pixel_aspect_ratio = self.current_pixel_aspect_ratio();
    }

    /// Restart pacing from now, so time spent opening a window is not run as
    /// catch-up frames.
    pub(crate) fn restart_pacing(&mut self) {
        self.next_slice_at = Instant::now();
    }

    /// One turn of the host loop: apply queued commands, advance the machine
    /// to the wall clock, and report a halt. Returns when to tick next.
    ///
    /// # Errors
    ///
    /// Returns the first fatal error from a command or the machine; the shell
    /// should then end the session.
    pub(crate) fn tick(&mut self, shell: &mut impl Shell) -> Result<Wake, UiError> {
        // Drain queued commands (menu + shortcuts) at the frame boundary, so an
        // action never tears down state the current frame is using.
        while let Ok(command) = self.command_rx.try_recv() {
            self.handle_command(shell, command)?;
        }
        if self.advance_machine()? {
            shell.request_redraw();
        }
        self.update_halt_status(shell);
        // While turbo-loading, tick back-to-back so the fast-load bursts run
        // with no wait between them; otherwise wake at the next paced slice.
        Ok(if self.turbo_active() {
            Wake::Now
        } else {
            Wake::At(self.next_slice_at)
        })
    }

    /// Surface a machine halt (e.g. a CPU JAM). On the transition into a halt:
    /// log it, append it to the window title, and (via the shell's render)
    /// draw the on-screen overlay. Clears again — restoring the title — when
    /// the machine resumes, e.g. after a reset. Idempotent per state.
    fn update_halt_status(&mut self, shell: &mut impl Shell) {
        let status = self.system.halt_status(&self.runner.runtime);
        if status == self.halt_message {
            return;
        }
        match &status {
            Some(message) => {
                eprintln!("warning: {message}");
                shell.set_title(&format!("{} — {message}", self.window_title()));
            }
            None => shell.set_title(&self.window_title()),
        }
        self.halt_message = status;
        shell.request_redraw();
    }

    /// Pixel aspect for the picture the machine is showing *now*.
    ///
    /// Prefer the raster-derived value, which does not depend on how much of
    /// the signal this core crops to. Cores not yet migrated fall back to the
    /// older "fill this shape" hook, which does. The presenter and the window
    /// size then stretch width to match.
    fn current_pixel_aspect_ratio(&self) -> f32 {
        let (fb_width, fb_height) = self.system.framebuffer_size(&self.runner.runtime);
        self.system
            .display(&self.runner.runtime)
            .map_or(1.0, |display| {
                display.pixel_aspect_ratio(fb_width, fb_height)
            })
    }

    /// The framebuffer's width stretched by the pixel aspect, and its height:
    /// the picture at scale 1, in logical pixels.
    pub(crate) fn display_size(&self) -> (f64, f64) {
        let (fb_width, fb_height) = self.system.framebuffer_size(&self.runner.runtime);
        let par = f64::from(self.presentation.pixel_aspect_ratio).max(f64::MIN_POSITIVE);
        (f64::from(fb_width) * par, f64::from(fb_height))
    }

    /// The logical picture size for an integer `scale`: the cropped framebuffer
    /// stretched by the derived pixel-aspect ratio (so the picture keeps its
    /// 4:3-or-whatever shape) and then by `scale`. Used both at window creation
    /// and by the View → Window Scale menu.
    pub(crate) fn picture_size(&self, scale: u32) -> (f64, f64) {
        let (_, fb_height) = self.system.framebuffer_size(&self.runner.runtime);
        let (display_width, _) = self.display_size();
        (
            display_width * f64::from(scale),
            f64::from(fb_height.saturating_mul(scale)),
        )
    }

    fn advance_machine(&mut self) -> Result<bool, UiError> {
        self.gamepads.drain_events_with_axes(
            self.system.button_map(),
            self.system.axis_map(),
            &mut self.pending_inputs,
        );

        // Turbo (fast-load): while a tape is playing and the user armed it, run
        // unthrottled in a bounded burst so the loader races ahead. `tick`
        // returns `Wake::Now` in this state so bursts run back-to-back.
        if self.turbo_active() {
            let mut frame_completed = false;
            for _ in 0..MAX_TURBO_FRAMES {
                let inputs = std::mem::take(&mut self.pending_inputs);
                if self.runner.run_ticks(&inputs, self.full_frame_ticks)? {
                    frame_completed = true;
                    self.queue_pointer_correction();
                }
            }
            self.next_slice_at = Instant::now();
            return Ok(frame_completed);
        }

        if Instant::now() < self.next_slice_at {
            return Ok(false);
        }

        let mut ran = 0u32;
        let max_slices =
            MAX_CATCH_UP_FRAMES.saturating_mul(self.system.input_slices_per_frame().max(1));
        let mut frame_completed = false;
        while Instant::now() >= self.next_slice_at && ran < max_slices {
            let inputs = std::mem::take(&mut self.pending_inputs);
            if self.runner.run_ticks(&inputs, self.slice_ticks)? {
                frame_completed = true;
                self.queue_pointer_correction();
            }
            self.next_slice_at += self.slice_duration;
            ran += 1;
        }
        if ran == max_slices && Instant::now() >= self.next_slice_at {
            self.next_slice_at = Instant::now() + self.slice_duration;
        }
        Ok(frame_completed)
    }

    /// The window title: the system's, plus any keyboard notice or mode.
    pub(crate) fn window_title(&self) -> String {
        let title = self.system.window_title();
        if let Some(notice) = &self.keyboard_notice {
            return format!("{title} — {notice}");
        }
        if self.system.supports_host_keyboard() {
            format!(
                "{title} — {} keyboard",
                if self.host_keyboard {
                    "Host"
                } else {
                    "Original"
                }
            )
        } else {
            title
        }
    }

    /// The host's modifier keys changed.
    pub(crate) fn modifiers_changed(&mut self, modifiers: ModifiersState) {
        self.modifiers = modifiers;
        if self.host_keyboard
            && let Some(keyword) = self.system.host_keyword_modifier()
        {
            self.pending_inputs.extend(
                self.held_target_keys
                    .refresh_keyword(keyword, self.modifiers.shift_key()),
            );
            self.next_slice_at = Instant::now();
        }
    }

    /// The window lost focus: let go of everything held, since the key-up
    /// and button-up events will go elsewhere.
    pub(crate) fn focus_lost(&mut self) {
        self.modifiers = ModifiersState::empty();
        self.release_all_keys();
        self.release_all_mouse_buttons();
        self.queue_aim_away();
    }

    /// A physical key went down or up. `logical` is what the host layout
    /// makes of it, for host-layout typing.
    pub(crate) fn key_event(
        &mut self,
        shell: &mut impl Shell,
        code: KeyCode,
        logical: &Key,
        pressed: bool,
        repeat: bool,
    ) {
        if repeat {
            return;
        }
        if self.handle_shortcut(code, pressed) {
            return;
        }
        if self.host_keyboard {
            self.queue_host_key(shell, code, logical, pressed);
        } else {
            self.queue_key_state(code, pressed);
        }
    }

    fn queue_key_state(&mut self, code: KeyCode, pressed: bool) {
        // Keyboard path (home computers): one host key → one or more machine key
        // names, emitted as InputEvent::Key. Takes precedence over the joystick
        // button map.
        if let Some(names) = self.system.map_keys(code) {
            let keys = pressed.then(|| names.iter().map(|name| (*name).to_owned()).collect());
            self.pending_inputs
                .extend(self.held_target_keys.update(code, keys));
            self.next_slice_at = Instant::now();
            return;
        }

        // Joystick / d-pad path (consoles): host key → HostControl → button map.
        let map = self.system.button_map();
        if pressed {
            let Some(control) = self.system.map_key(code) else {
                return;
            };
            if self.pressed_keys.contains_key(&code) {
                return;
            }
            self.pressed_keys.insert(code, control);
            if let Some(input) = map.event(control, true) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        } else if let Some(control) = self.pressed_keys.remove(&code) {
            if let Some(input) = map.event(control, false) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        }
    }

    fn queue_host_key(
        &mut self,
        shell: &mut impl Shell,
        code: KeyCode,
        logical: &Key,
        pressed: bool,
    ) {
        let keys = match keyboard::route_host_key(
            code,
            logical,
            pressed,
            self.modifiers,
            self.system.host_keyword_modifier(),
            &self.held_target_keys,
        ) {
            keyboard::HostKey::Release => None,
            keyboard::HostKey::Ignore => return,
            keyboard::HostKey::Keys(keys) => Some(keys),
            keyboard::HostKey::Physical => self
                .system
                .map_keys(code)
                .map(|names| names.iter().map(|name| (*name).to_owned()).collect()),
            keyboard::HostKey::Control => self
                .system
                .host_control_keys(code)
                .map(|names| names.iter().map(|name| (*name).to_owned()).collect()),
            keyboard::HostKey::Character(ch) => {
                match self.system.host_character_keys(&self.runner.runtime, ch) {
                    Some(keys) => Some(keys),
                    None => {
                        self.keyboard_notice = Some(format!("Unsupported character: {ch}"));
                        shell.set_title(&self.window_title());
                        eprintln!(
                            "keyboard: character {ch:?} is not supported by this target mapping"
                        );
                        return;
                    }
                }
            }
        };
        if pressed && self.keyboard_notice.take().is_some() {
            shell.set_title(&self.window_title());
        }
        self.pending_inputs
            .extend(self.held_target_keys.update(code, keys));
        self.next_slice_at = Instant::now();
    }

    fn release_all_keys(&mut self) {
        let map = self.system.button_map();
        for control in std::mem::take(&mut self.pressed_keys).into_values() {
            if let Some(input) = map.event(control, false) {
                self.pending_inputs.push(input);
            }
        }
        self.pending_inputs
            .extend(self.held_target_keys.release_all());
        self.next_slice_at = Instant::now();
    }

    /// Convert a window cursor position to framebuffer coordinates, so mouse
    /// deltas track the emulated screen resolution rather than the window
    /// scale. `window` is the window's inner size, when it has one.
    fn cursor_to_frame_position(&self, x: f64, y: f64, window: Option<(u32, u32)>) -> (f64, f64) {
        let Some((width, height)) = window else {
            return (x, y);
        };
        let (fb_w, fb_h) = self.system.framebuffer_size(&self.runner.runtime);
        let width = f64::from(width.max(1));
        let height = f64::from(height.max(1));
        (x * f64::from(fb_w) / width, y * f64::from(fb_h) / height)
    }

    /// The cursor moved to `(x, y)` in a window whose inner size is `window`.
    pub(crate) fn cursor_moved(&mut self, x: f64, y: f64, window: Option<(u32, u32)>) {
        self.queue_mouse_motion(x, y, window);
        self.queue_aim(x, y, window);
    }

    /// Emit an absolute `Aim` for a system with a pointing device.
    ///
    /// Sent on every move rather than on change, because the machine may have
    /// resized its display since the last one — the Master System switches
    /// between 192, 224 and 240 lines at runtime — and the same cursor
    /// position then means a different pixel.
    fn queue_aim(&mut self, x: f64, y: f64, window: Option<(u32, u32)>) {
        let Some(port) = self.system.aim_port() else {
            return;
        };
        let (fb_w, fb_h) = self.system.framebuffer_size(&self.runner.runtime);
        let (fx, fy) = self.cursor_to_frame_position(x, y, window);
        let at = emu198x_shell::aim_from_pixels(fx as i32, fy as i32, fb_w, fb_h);
        self.pending_inputs.push(InputEvent::Aim { port, at });
        self.next_slice_at = Instant::now();
    }

    /// Tell a pointing device the cursor has left the window, which is a
    /// gesture and not an absence: light-gun games read "aimed off the
    /// screen" as the reload.
    pub(crate) fn queue_aim_away(&mut self) {
        if let Some(port) = self.system.aim_port() {
            self.pending_inputs.push(InputEvent::Aim { port, at: None });
            self.next_slice_at = Instant::now();
        }
    }

    /// Follow the host cursor with the guest's mouse. A system that can say
    /// where its pointer is ([`UiSystem::guest_pointer`]) has the cursor
    /// recorded as the target for [`Self::queue_pointer_correction`]; any
    /// other gets the cursor's movement as relative `PointerMotion`, in the
    /// guest's counts, the first event after focus only setting the baseline. No-op for a system with no
    /// mouse.
    fn queue_mouse_motion(&mut self, x: f64, y: f64, window: Option<(u32, u32)>) {
        let Some(device) = self.system.mouse_device() else {
            return;
        };
        let position = self.cursor_to_frame_position(x, y, window);
        if self.system.guest_pointer(&self.runner.runtime).is_some() {
            self.pointer_sync.set_target(Some(position));
            self.relative_motion.reset();
            return;
        }
        self.pointer_sync.reset();
        let scale = self.system.pointer_counts_per_pixel(&self.runner.runtime);
        let Some((dx, dy)) = self.relative_motion.motion_to(position, scale) else {
            return;
        };
        self.pending_inputs.push(InputEvent::PointerMotion {
            device: device.into(),
            dx,
            dy,
        });
        self.next_slice_at = Instant::now();
    }

    /// After a frame, send the counts that move the guest pointer toward the
    /// host cursor. No-op unless the cursor is over the window and the system
    /// can locate its pointer.
    fn queue_pointer_correction(&mut self) {
        let Some(device) = self.system.mouse_device() else {
            return;
        };
        if !self.pointer_sync.is_active() {
            return;
        }
        let Some(guest) = self.system.guest_pointer(&self.runner.runtime) else {
            // The pointer has gone — a game took the screen — so hand the
            // cursor back until it reappears.
            self.pointer_sync.reset();
            return;
        };
        let scale = self.system.pointer_counts_per_pixel(&self.runner.runtime);
        if let Some((dx, dy)) = self.pointer_sync.correction(guest, scale) {
            self.pending_inputs.push(InputEvent::PointerMotion {
                device: device.into(),
                dx,
                dy,
            });
        }
    }

    /// Whether the window should show the host cursor. It is hidden while the
    /// guest's own pointer stands in for it.
    pub(crate) fn host_cursor_visible(&self) -> bool {
        !self.pointer_sync.is_active()
    }

    /// The cursor left the window: aim away, and leave the guest pointer
    /// where it is.
    pub(crate) fn cursor_left(&mut self) {
        self.queue_aim_away();
        self.pointer_sync.reset();
        self.relative_motion.reset();
    }

    /// Emit a `PointerButton` press/release, tracking held buttons so they can be
    /// released on focus loss. No-op for a system with no mouse or an unmapped
    /// button.
    pub(crate) fn mouse_button(&mut self, button: MouseButton, pressed: bool) {
        if let (Some(port), Some(name)) =
            (self.system.aim_port(), self.system.map_aim_button(button))
        {
            let held = self.pressed_aim_buttons.contains(&button);
            if pressed != held {
                if pressed {
                    self.pressed_aim_buttons.insert(button);
                } else {
                    self.pressed_aim_buttons.remove(&button);
                }
                self.pending_inputs.push(InputEvent::Button {
                    port,
                    name: name.into(),
                    pressed,
                });
                self.next_slice_at = Instant::now();
            }
        }
        let Some(device) = self.system.mouse_device() else {
            return;
        };
        if pressed {
            let Some(name) = self.system.map_mouse_button(button) else {
                return;
            };
            if self.pressed_mouse_buttons.insert(button, name).is_some() {
                return; // already held
            }
            self.pending_inputs.push(InputEvent::PointerButton {
                device: device.into(),
                button: name.into(),
                pressed: true,
            });
            self.next_slice_at = Instant::now();
        } else if let Some(name) = self.pressed_mouse_buttons.remove(&button) {
            self.pending_inputs.push(InputEvent::PointerButton {
                device: device.into(),
                button: name.into(),
                pressed: false,
            });
            self.next_slice_at = Instant::now();
        }
    }

    fn release_all_mouse_buttons(&mut self) {
        if let Some(port) = self.system.aim_port() {
            for button in std::mem::take(&mut self.pressed_aim_buttons) {
                if let Some(name) = self.system.map_aim_button(button) {
                    self.pending_inputs.push(InputEvent::Button {
                        port,
                        name: name.into(),
                        pressed: false,
                    });
                }
            }
            self.next_slice_at = Instant::now();
        }
        let Some(device) = self.system.mouse_device() else {
            return;
        };
        for name in std::mem::take(&mut self.pressed_mouse_buttons).into_values() {
            self.pending_inputs.push(InputEvent::PointerButton {
                device: device.into(),
                button: name.into(),
                pressed: false,
            });
        }
        self.relative_motion.reset();
        self.pointer_sync.reset();
        self.next_slice_at = Instant::now();
    }

    /// Run one whole frame from a clean capture, so a picture from after a
    /// reset, load or switch is ready to show.
    fn refresh_frame(&mut self) -> Result<(), UiError> {
        self.runner.clear_capture();
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        Ok(())
    }

    /// Hard-reset the machine: reset the runtime, run the system's re-init
    /// hook, clear capture/audio, and run one frame so a picture is ready.
    fn reset_machine(&mut self, shell: &mut impl Shell) -> Result<(), UiError> {
        self.release_all_keys();
        self.runner.runtime.reset(ResetKind::Hard);
        self.system.after_reset(&mut self.runner.runtime)?;
        shell.reset_video_history();
        self.refresh_frame()
    }

    /// The quick-save slot file for the running machine: one file per concrete
    /// profile (`<profile_id>.state`) under the state directory, so e.g. the
    /// NTSC and PAL variants of a console don't share a slot. `None` if no
    /// state directory can be resolved.
    fn state_slot_path(&self) -> Option<PathBuf> {
        let root = state_root()?;
        let id = self.runner.runtime.profile().profile_id.as_str();
        Some(root.join(slot_file_name(id)))
    }

    /// Quick-save: serialise the runtime via [`MachineCore::snapshot`] and write
    /// it to the machine's slot file. Failures (a runtime that can't snapshot,
    /// or an I/O error) are reported and otherwise ignored — a missing
    /// save-state must never take the window down.
    fn quick_save(&mut self) {
        let bytes = match self.runner.runtime.snapshot() {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("save-state: this machine can't snapshot yet: {err}");
                return;
            }
        };
        let Some(path) = self.state_slot_path() else {
            eprintln!("save-state: no state directory (set EMU198X_STATE_DIR or HOME)");
            return;
        };
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            eprintln!("save-state: cannot create {}: {err}", parent.display());
            return;
        }
        match std::fs::write(&path, &bytes) {
            Ok(()) => println!(
                "save-state: wrote {} ({} bytes)",
                path.display(),
                bytes.len()
            ),
            Err(err) => eprintln!("save-state: cannot write {}: {err}", path.display()),
        }
    }

    /// Quick-load: read the machine's slot file and restore it via
    /// [`MachineCore::restore`], then refresh the picture. A missing slot or a
    /// rejected restore is reported, not fatal; only a genuine emulation error
    /// from running the refresh frame propagates.
    fn quick_load(&mut self, shell: &mut impl Shell) -> Result<(), UiError> {
        let path = self.state_slot_path();
        if let Err(message) =
            quick_load::restore(path.as_deref(), |bytes| self.runner.runtime.restore(bytes))
        {
            // A native modal can consume key-up events while it owns focus.
            self.release_all_keys();
            shell.message("Load State", Notice::Warning, &message);
            self.next_slice_at = Instant::now();
            return Ok(());
        }
        // Drop held keys and stale capture/audio, then run one frame so the
        // restored picture is on screen immediately.
        self.release_all_keys();
        shell.reset_video_history();
        self.refresh_frame()?;
        // A restored snapshot can carry a different drive layout.
        self.refresh_all_port_drives(shell);
        if let Some(path) = path {
            println!("load-state: restored {}", path.display());
        }
        Ok(())
    }

    /// Returns `true` if the key was consumed as a UI shortcut (harness F12,
    /// the Cmd/Ctrl+S / +L save-state chords, or a per-system
    /// [`UiSystem::handle_key`] shortcut), so it isn't also routed to the
    /// machine.
    fn handle_shortcut(&mut self, code: KeyCode, pressed: bool) -> bool {
        if self.system.supports_host_keyboard()
            && code == KeyCode::KeyK
            && !self.modifiers.alt_key()
            && self.modifiers.shift_key()
            && (self.modifiers.super_key() || self.modifiers.control_key())
        {
            if pressed {
                let _ = self
                    .command_tx
                    .send(AppCommand::SetHostKeyboard(!self.host_keyboard));
            }
            return true;
        }
        if self.system.tape_export_filter().is_some() && tape_export_chord(self.modifiers, code) {
            if pressed {
                let _ = self.command_tx.send(AppCommand::ExportTape);
            }
            return true;
        }
        // Save-state chords. Gated on a host modifier (Cmd on macOS, Ctrl
        // elsewhere) so they never shadow the bare S / L keys the machine
        // keyboard uses, nor the F1-F10 function keys the home computers map.
        // Routed through the command channel so the chord and the State menu
        // run the identical handler.
        match state_chord_action(self.modifiers, code, pressed) {
            Some(StateAction::Save) => {
                let _ = self.command_tx.send(AppCommand::QuickSave);
                return true;
            }
            Some(StateAction::Load) => {
                let _ = self.command_tx.send(AppCommand::QuickLoad);
                return true;
            }
            None => {}
        }
        // Tape transport: F9 play / F10 stop / F11 toggle fast-load — but only
        // for a system that has a tape slot, and only for a key the system
        // doesn't itself map (so non-tape machines, and tape machines that use
        // F9-F11 as keys, keep them). Routed through the command channel so the
        // Tape menu and these shortcuts share one handler.
        if let Some(shortcut) = tape_transport_shortcut(code)
            && self.system.map_keys(code).is_none()
            && let Some(slot) = self.tape_slot()
        {
            if pressed {
                let _ = self.command_tx.send(match shortcut {
                    TapeShortcut::Play => AppCommand::MediaTransport {
                        slot,
                        action: MediaTransportAction::Start,
                    },
                    TapeShortcut::Stop => AppCommand::MediaTransport {
                        slot,
                        action: MediaTransportAction::Stop,
                    },
                    TapeShortcut::ToggleTurbo => AppCommand::ToggleTurbo,
                });
            }
            return true;
        }
        match code {
            // Reset goes through the same `AppCommand::Reset` the Machine menu
            // emits, so menu and shortcut share one path.
            KeyCode::F12 => {
                if pressed {
                    let _ = self.command_tx.send(AppCommand::Reset);
                }
                true
            }
            _ => self
                .system
                .handle_key(&mut self.runner.runtime, code, pressed),
        }
    }

    /// Process one queued [`AppCommand`] at the frame boundary, whatever its
    /// source (menu click or keyboard shortcut).
    fn handle_command(
        &mut self,
        shell: &mut impl Shell,
        command: AppCommand,
    ) -> Result<(), UiError> {
        match command {
            AppCommand::Reset => self.reset_machine(shell)?,
            AppCommand::QuickSave => self.quick_save(),
            AppCommand::QuickLoad => self.quick_load(shell)?,
            AppCommand::SetScale(scale) => self.set_window_scale(shell, scale),
            AppCommand::SetFilter(filter) => self.set_video_filter(shell, filter),
            AppCommand::OpenMedia { slot, kind } => self.open_media(shell, &slot, kind)?,
            AppCommand::Eject { slot } => self.eject_media(shell, &slot)?,
            AppCommand::SwitchVariant(id) => self.switch_to_variant(shell, &id)?,
            AppCommand::SetPortDrive { device, kind_id } => {
                self.set_port_drive(shell, device, &kind_id);
            }
            AppCommand::MediaTransport { slot, action } => self.media_transport(&slot, action),
            AppCommand::ToggleTurbo => self.toggle_turbo(shell),
            AppCommand::OpenState => self.open_state(shell)?,
            AppCommand::SaveState => self.save_state(shell),
            AppCommand::ExportTape => self.export_tape(shell),
            AppCommand::SetHostKeyboard(enabled) => {
                self.release_all_keys();
                self.keyboard_notice = None;
                self.host_keyboard = enabled && self.system.supports_host_keyboard();
                shell.sync_menu(MenuSync::HostKeyboard(self.host_keyboard));
                shell.set_title(&self.window_title());
            }
        }
        Ok(())
    }

    /// File → Open State…: ask for a file in the system's state formats, hand
    /// it to [`UiSystem::load_state_file`], and refresh the picture. A
    /// cancelled dialog or a rejected file is reported and ignored; only a
    /// genuine emulation error from the refresh frame propagates.
    fn open_state(&mut self, shell: &mut impl Shell) -> Result<(), UiError> {
        let Some(filter) = self.system.state_open_filter() else {
            return Ok(());
        };
        let Some(path) = shell.pick_file("Open State", filter) else {
            return Ok(()); // user cancelled
        };
        if let Err(err) = self.system.load_state_file(&mut self.runner.runtime, &path) {
            eprintln!("state: cannot load {}: {err}", path.display());
            return Ok(());
        }
        self.release_all_keys();
        shell.reset_video_history();
        self.refresh_frame()?;
        // A loaded state can carry a different drive layout.
        self.refresh_all_port_drives(shell);
        println!("state: loaded {}", path.display());
        shell.request_redraw();
        Ok(())
    }

    /// State → Save State As…: serialise via [`MachineCore::snapshot`], then
    /// ask where to save, suggesting the machine's slot name, and write the
    /// bytes there. The counterpart to [`Self::open_state`] /
    /// [`Self::quick_save`] (which writes the fixed slot). Failures — a runtime
    /// that can't snapshot, a cancelled dialog, an I/O error — are reported and
    /// otherwise ignored, never fatal. The snapshot is taken before the dialog,
    /// so a machine that can't snapshot reports immediately instead of popping
    /// a picker that can't be honoured.
    fn save_state(&mut self, shell: &mut impl Shell) {
        let bytes = match self.runner.runtime.snapshot() {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("save-state: this machine can't snapshot yet: {err}");
                return;
            }
        };
        let default_name = slot_file_name(self.runner.runtime.profile().profile_id.as_str());
        let Some(path) = shell.pick_save_file(
            "Save State",
            &default_name,
            ("Save state", &["state"]),
            true,
        ) else {
            return; // user cancelled
        };
        match std::fs::write(&path, &bytes) {
            Ok(()) => println!(
                "save-state: wrote {} ({} bytes)",
                path.display(),
                bytes.len()
            ),
            Err(err) => eprintln!("save-state: cannot write {}: {err}", path.display()),
        }
    }

    /// Export the recording to a new host file; cancellation and write failures
    /// retain the recording so the user can retry. Dialogs run at a frame boundary.
    fn export_tape(&mut self, shell: &mut impl Shell) {
        const TITLE: &str = "Export Tape Recording";
        let Some((label, extension)) = self.system.tape_export_filter() else {
            return;
        };
        self.release_all_keys();
        let bytes = match self.system.export_tape(&self.runner.runtime) {
            Ok(bytes) => bytes,
            Err(message) => {
                shell.message(TITLE, Notice::Warning, &message);
                return;
            }
        };
        let Some(path) = shell.pick_save_file(
            "Export Tape Recording — choose a new file",
            &format!("recording.{extension}"),
            (label, &[extension]),
            false,
        ) else {
            return;
        };
        match export::write_new_file(&path, &bytes) {
            Ok(()) => shell.message(
                TITLE,
                Notice::Info,
                &format!(
                    "Exported {} bytes to {}.\nThe recording remains available in this session.",
                    bytes.len(),
                    path.display()
                ),
            ),
            Err(err) => shell.message(
                TITLE,
                Notice::Error,
                &format!(
                    "Could not export to {}: {err}\nChoose a new filename and try again. The recording has been kept.",
                    path.display()
                ),
            ),
        }
    }

    /// The id of the machine's tape slot (the first `Tape`-kind media slot in
    /// its profile), if any. Drives the transport shortcuts/menu and gates the
    /// turbo pacing.
    fn tape_slot(&self) -> Option<Cow<'static, str>> {
        self.runner
            .runtime
            .profile()
            .media_slots
            .iter()
            .find(|slot| slot.kind == MediaKind::Tape)
            .map(|slot| slot.id.clone())
    }

    /// Whether to race the machine (turbo): the user armed fast-load and a tape
    /// is actually playing.
    fn turbo_active(&self) -> bool {
        self.turbo_armed && self.system.tape_playing(&self.runner.runtime)
    }

    /// Tape → Play / Stop (or F9 / F10): issue a transport command to the slot.
    /// A rejected command is logged, never fatal; the running pacing loop picks
    /// up the new tape state on the next tick.
    fn media_transport(&mut self, slot: &str, action: MediaTransportAction) {
        let command =
            ControlCommand::MediaTransport(MediaTransportCommand::new(slot.to_owned(), action));
        if let Err(err) = self.runner.runtime.command(&command) {
            eprintln!("transport: {action:?} on slot \"{slot}\" rejected: {err}");
        }
    }

    /// Tape → Fast-Forward While Playing (or F11): arm/disarm turbo. It only
    /// races while a tape is playing ([`Self::turbo_active`]); a fresh
    /// baseline keeps pacing smooth when it turns off mid-load.
    fn toggle_turbo(&mut self, shell: &mut impl Shell) {
        self.turbo_armed = !self.turbo_armed;
        self.next_slice_at = Instant::now();
        shell.sync_menu(MenuSync::Turbo(self.turbo_armed));
        println!(
            "tape fast-load {}",
            if self.turbo_armed { "armed" } else { "off" }
        );
    }

    /// Machine → Drives radio: set the drive model on `device` via the system's
    /// [`UiSystem::set_port_drive`], then re-check the radio from the runtime's
    /// actual post-change state. Unlike a variant switch this swaps a drive on
    /// the bus without rebuilding the machine, so there is no re-pacing or
    /// framebuffer change — a non-fatal action, run between frames like the
    /// others. On error the running machine is untouched and the re-read pins the
    /// radio back to the unchanged selection.
    fn set_port_drive(&mut self, shell: &mut impl Shell, device: u8, kind_id: &str) {
        if let Err(err) = self
            .system
            .set_port_drive(&mut self.runner.runtime, device, kind_id)
        {
            eprintln!("machine: cannot set device {device} drive to {kind_id}: {err}");
        }
        self.refresh_port_drive(shell, device);
    }

    /// Re-check `device`'s Drives radio from the runtime's live selection.
    fn refresh_port_drive(&mut self, shell: &mut impl Shell, device: u8) {
        if let Some(port) = self
            .system
            .drive_ports(&self.runner.runtime)
            .into_iter()
            .find(|port| port.device == device)
        {
            shell.sync_menu(MenuSync::PortDrive {
                device,
                kind_id: &port.current,
            });
        }
    }

    /// Re-check every Drives radio from the runtime — used after a state load,
    /// which can change the whole port layout.
    fn refresh_all_port_drives(&mut self, shell: &mut impl Shell) {
        for port in self.system.drive_ports(&self.runner.runtime) {
            shell.sync_menu(MenuSync::PortDrive {
                device: port.device,
                kind_id: &port.current,
            });
        }
    }

    /// Recompute the per-frame pacing (slice ticks + wall-clock) from the current
    /// runtime's timing. Called after a variant switch, since frame length can
    /// differ between variants (e.g. the Spectrum 48K's 69888 T-states vs the
    /// 128K's 70908).
    fn recompute_pacing(&mut self) {
        let slices = self.system.input_slices_per_frame().max(1);
        let frame_ticks = self.system.frame_ticks(&self.runner.runtime);
        let frame_duration = self.system.frame_duration(&self.runner.runtime);
        self.full_frame_ticks = frame_ticks;
        self.slice_ticks = frame_ticks.div_ceil(u64::from(slices));
        self.slice_duration =
            Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(slices));
        self.next_slice_at = Instant::now();
    }

    /// Machine → variant radio: rebuild the live machine as `variant` via the
    /// system's [`UiSystem::switch_variant`], then re-pace and refresh the
    /// picture. Runs between frames (the single-command-channel invariant), so it
    /// never tears down a machine a frame is mid-render on. On failure the
    /// running machine is untouched and the radio is pinned back to it; only a
    /// genuine emulation error from the refresh frame is fatal.
    fn switch_to_variant(&mut self, shell: &mut impl Shell, variant: &str) -> Result<(), UiError> {
        let old_size = self.system.framebuffer_size(&self.runner.runtime);
        if let Err(err) = self
            .system
            .switch_variant(&mut self.runner.runtime, variant)
        {
            eprintln!("machine: cannot switch to variant {variant}: {err}");
            if let Some(current) = self.system.current_variant() {
                shell.sync_menu(MenuSync::Variant(&current));
            }
            return Ok(());
        }
        // Re-pace from the new machine's timing and drop stale input + capture.
        self.recompute_pacing();
        self.release_all_keys();
        shell.reset_video_history();
        self.runner.clear_capture();
        // The framebuffer size can differ between variants; rebuild the presenter
        // and resize the window if so.
        // A variant can move a machine between PAL and NTSC, which changes the
        // pixel aspect even when the framebuffer keeps its dimensions.
        self.presentation.pixel_aspect_ratio = self.current_pixel_aspect_ratio();
        let new_size = self.system.framebuffer_size(&self.runner.runtime);
        if new_size != old_size {
            let (w, h) = new_size;
            shell.framebuffer_resized(w, h)?;
            let (width, height) = self.picture_size(self.scale);
            shell.request_picture_size(width, height);
        }
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        if let Some(current) = self.system.current_variant() {
            shell.sync_menu(MenuSync::Variant(&current));
        }
        shell.set_title(&self.window_title());
        shell.request_redraw();
        Ok(())
    }

    /// File → Open: ask for a file of the slot's media kind, load it into the
    /// named slot via [`MachineCore::load_media`], and refresh the picture.
    /// Cancellation is quiet; unreadable or rejected media gets a nonfatal
    /// warning. Only a genuine emulation error from the refresh frame
    /// propagates. The dialog blocks the loop while open — fine, since it's
    /// user-initiated and the machine simply pauses.
    fn open_media(
        &mut self,
        shell: &mut impl Shell,
        slot: &str,
        kind: MediaKind,
    ) -> Result<(), UiError> {
        let (label, extensions) = media_filter(kind);
        let Some(path) = shell.pick_file(&format!("Open {label}"), (label, extensions)) else {
            return Ok(()); // user cancelled
        };
        self.load_media_file(shell, &path, slot, kind)
    }

    /// Load the file at `path` into `slot`, as File → Open does once a file is
    /// chosen.
    pub(crate) fn load_media_file(
        &mut self,
        shell: &mut impl Shell,
        path: &Path,
        slot: &str,
        kind: MediaKind,
    ) -> Result<(), UiError> {
        if let Err(message) =
            media_open::load(path, slot, kind, |set| self.runner.runtime.load_media(set))
        {
            // A native modal can consume key-up events while it owns focus.
            self.release_all_keys();
            shell.message("Open Media", Notice::Warning, &message);
            self.next_slice_at = Instant::now();
            return Ok(());
        }
        // Fresh capture/audio, then one frame so the inserted media shows. Some
        // cartridge machines need a Reset (Machine → Reset) to pick it up.
        self.release_all_keys();
        self.refresh_frame()?;
        println!("media: loaded {} into slot \"{slot}\"", path.display());
        shell.request_redraw();
        Ok(())
    }

    /// File → Eject: eject whatever media occupies `slot` via
    /// [`MachineCore::eject_media`], then refresh the picture. A runtime that
    /// rejects the eject (empty slot, no eject wired for this slot, or an
    /// unsupported machine) is reported and ignored — never fatal, mirroring
    /// [`Self::media_transport`]. Only a genuine emulation error from the
    /// refresh frame propagates.
    fn eject_media(&mut self, shell: &mut impl Shell, slot: &str) -> Result<(), UiError> {
        if let Err(err) = self.runner.runtime.eject_media(slot) {
            eprintln!("media: eject of slot \"{slot}\" rejected: {err}");
            return Ok(());
        }
        // Fresh capture/audio, then one frame so the now-empty slot shows.
        self.release_all_keys();
        self.refresh_frame()?;
        println!("media: ejected slot \"{slot}\"");
        shell.request_redraw();
        Ok(())
    }

    /// Resize the picture to an integer multiple of the native frame.
    pub(crate) fn set_window_scale(&mut self, shell: &mut impl Shell, scale: u32) {
        self.scale = scale;
        let (width, height) = self.picture_size(scale);
        shell.request_picture_size(width, height);
        shell.request_redraw();
        shell.sync_menu(MenuSync::Scale(scale));
    }

    /// Switch display mode while preserving geometry, timebase and phosphor settings.
    fn set_video_filter(&mut self, shell: &mut impl Shell, filter: VideoFilter) {
        if filter.uses_signal() {
            let supported = self
                .runner
                .frame()
                .and_then(|frame| frame.signal.as_ref())
                .is_some_and(|signal| {
                    !filter.uses_monitor_connection()
                        || matches!(
                            signal.encoding,
                            emu198x_shell::SignalEncoding::Rgb
                                | emu198x_shell::SignalEncoding::Yuv {
                                    separate_chroma: true,
                                    ..
                                }
                        )
                });
            if !supported {
                eprintln!(
                    "the selected video connection is unavailable for this machine or variant"
                );
                return;
            }
        }
        // Display geometry, timebase and phosphor choice survive mode changes.
        self.presentation.filter = filter;
        shell.sync_menu(MenuSync::Filter(filter));
        shell.request_redraw();
    }

    /// Settle the presentation for the frame about to be drawn, and return
    /// the filter it fell back to, if it had to, for the shell's menu.
    ///
    /// Debug stepping and a switch to a variant without an electrical source
    /// must still display their raw state rather than end the UI, so a frame
    /// without a signal drops an electrical filter. The phosphor clock is
    /// taken from the running machine.
    pub(crate) fn settle_presentation(&mut self) -> Option<VideoFilter> {
        let mut fallback = None;
        if let Some(frame) = self.runner.frame()
            && frame.signal.is_none()
            && self.presentation.filter.uses_signal()
        {
            let filter = if self.presentation.filter.is_modern() {
                VideoFilter::Raw
            } else {
                VideoFilter::Crt
            };
            self.presentation.filter = filter;
            eprintln!("electrical video is unavailable for this frame; using {filter} display");
            fallback = Some(filter);
        }
        let clock = self.runner.runtime.profile().clock.rate;
        self.presentation.clock_hz = clock.numerator_hz as f64 / clock.denominator_hz as f64;
        fallback
    }

    /// End the session: run the system's teardown hook.
    pub(crate) fn on_exit(&mut self) -> Result<(), String> {
        self.system.on_exit(&mut self.runner.runtime)
    }
}

pub(crate) fn tape_export_chord(modifiers: ModifiersState, code: KeyCode) -> bool {
    code == KeyCode::KeyE
        && modifiers.shift_key()
        && (modifiers.super_key() || modifiers.control_key())
        && !modifiers.alt_key()
}

/// A tape-transport action a function key maps to (only honoured on a system
/// that has a tape slot and doesn't itself use the key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TapeShortcut {
    Play,
    Stop,
    ToggleTurbo,
}

/// The tape-transport shortcut for a key, if any: F9 play, F10 stop, F11
/// toggle fast-load — matching the Spectrum/C64 bespoke runners.
pub(crate) fn tape_transport_shortcut(code: KeyCode) -> Option<TapeShortcut> {
    match code {
        KeyCode::F9 => Some(TapeShortcut::Play),
        KeyCode::F10 => Some(TapeShortcut::Stop),
        KeyCode::F11 => Some(TapeShortcut::ToggleTurbo),
        _ => None,
    }
}

/// A save-state action a key event maps to, once the modifier gating is
/// applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateAction {
    Save,
    Load,
}

/// Decide whether a key event is a save-state chord. The chord fires only on
/// key-*down* with Cmd (macOS) or Ctrl (elsewhere) held — never on a bare key,
/// so the machine keyboard keeps its S / L and F-keys, and never on release.
pub(crate) fn state_chord_action(
    modifiers: ModifiersState,
    code: KeyCode,
    pressed: bool,
) -> Option<StateAction> {
    if !pressed || modifiers.alt_key() || !(modifiers.super_key() || modifiers.control_key()) {
        return None;
    }
    match code {
        KeyCode::KeyS => Some(StateAction::Save),
        KeyCode::KeyL => Some(StateAction::Load),
        _ => None,
    }
}

/// File-dialog filter (label + extensions) for a media kind. Extensions are a
/// generous superset across the family's machines; `MediaKind` is
/// `#[non_exhaustive]`, so an unknown kind falls back to no extension filter.
pub(crate) fn media_filter(kind: MediaKind) -> (&'static str, &'static [&'static str]) {
    match kind {
        MediaKind::Tape => ("Tape images", &["tap", "tzx", "cas", "t64", "tsx", "cdt"]),
        MediaKind::Disk => (
            "Disk images",
            &[
                "dsk", "adf", "d64", "trd", "img", "scl", "fdi", "do", "po", "st",
            ],
        ),
        MediaKind::Cartridge => (
            "Cartridge ROMs",
            &[
                "bin", "rom", "a26", "a52", "a78", "col", "sg", "nes", "crt", "car", "cart",
            ],
        ),
        MediaKind::Optical => ("Disc images", &["iso", "cue", "chd"]),
        MediaKind::Snapshot => ("Snapshots", &["sna", "z80", "szx", "sav", "vsf"]),
        MediaKind::Program => ("Programs", &["prg", "bas", "p", "o", "com"]),
        _ => ("Files", &[]),
    }
}

/// The slot file name for a profile id: the id with any character that isn't
/// ASCII-alphanumeric, `-`, or `_` replaced by `-`, plus a `.state` extension.
/// Keeps the per-profile slot on a single predictable, filesystem-safe path.
pub(crate) fn slot_file_name(profile_id: &str) -> String {
    let mut name: String = profile_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    name.push_str(".state");
    name
}

/// Directory the quick-save slots live in: `$EMU198X_STATE_DIR` if set,
/// otherwise `<home>/.emu198x/state`. `None` when neither the override nor a
/// home directory can be resolved (in which case save-states are unavailable).
fn state_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("EMU198X_STATE_DIR")
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".emu198x/state"))
}
