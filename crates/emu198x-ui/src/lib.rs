//! Shared native-UI harness for Emu198x runners.
//!
//! Six runners (NES, Game Boy, Amiga, C64, Dragon, Spectrum) each carried a
//! near-identical winit `ApplicationHandler` + wgpu video + framed audio +
//! keyboard/gamepad-input loop. This crate factors that common spine out: a
//! runner supplies a small [`UiSystem`] descriptor (its runtime, framebuffer
//! size, frame timing, button map, and key map) and calls [`run`] to get a
//! native window with `raw`/`lcd`/`crt` video filters, framed audio, gamepad
//! input, Esc-quit / F12-reset, Cmd/Ctrl+S / +L quick save-states, and a native
//! menu bar (App / File / Machine / State / View; see [`menu`]).
//!
//! The File menu's media open is built from each machine's declared media
//! slots, so it needs no per-system code. Still to come for the menu: machine
//! variant switching (a live-runtime trait) and multi-slot save-states.

mod menu;
mod overlay;

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use menu::{AppCommand, AppMenu};

use emu198x_native_video::{PresentationProfile, VideoPresenterError, WgpuVideoPresenter};
use emu198x_shell::{
    CapturedFrame, ControlCommand, HostIo, InputEvent, LatestFrameCapture, MachineCore,
    MachineError, MachineTime, MediaImage, MediaKind, MediaSet, MediaTransportAction,
    MediaTransportCommand, NativeAudioError, NativeAudioOutput, NativeGamepadInput, NullTraceSink,
    PixelFormat, ResetKind, RunResult, read_media_asset,
};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{ModifiersState, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

// Re-exported so a `UiSystem` impl can describe itself without depending on
// winit / native-video / shell input types directly.
pub use emu198x_native_video::VideoFilter;
pub use emu198x_shell::{
    AxisInputMap, AxisTarget, ButtonInputMap, ButtonTarget, HostAxis, HostControl,
};
pub use winit::keyboard::KeyCode;

/// The empty axis map returned by [`UiSystem::axis_map`]'s default — a system
/// with no analogue inputs gets no axis routing.
static EMPTY_AXIS_MAP: AxisInputMap = AxisInputMap::new(&[]);

const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;
/// Frames per `about_to_wait` while fast-loading a tape (turbo): run unthrottled
/// in bounded bursts so the loader races ahead but the window stays responsive.
const MAX_TURBO_FRAMES: u32 = 32;

/// A switchable machine variant for the Machine menu's variant radio. `id` is a
/// stable string the system maps back to its model — the same id its script/MCP
/// `set_machine` accepts — and `label` is the human-readable menu text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantInfo {
    /// Stable variant identifier (round-trips through [`UiSystem::switch_variant`]).
    pub id: Cow<'static, str>,
    /// Human-readable menu label.
    pub label: Cow<'static, str>,
}

impl VariantInfo {
    /// Build a [`VariantInfo`] from anything that converts into a `Cow`.
    pub fn new(id: impl Into<Cow<'static, str>>, label: impl Into<Cow<'static, str>>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// Per-system configuration the harness needs to host a machine in a window.
///
/// The runner builds its runtime (CLI parsing, cartridge/media loading) and
/// hands it to [`run`] alongside an implementor of this trait.
pub trait UiSystem {
    /// The machine runtime this system drives.
    type Runtime: MachineCore;

    /// Window title.
    fn window_title(&self) -> String;

    /// Default integer window scale when the CLI doesn't override it.
    fn default_scale(&self) -> u32 {
        3
    }

    /// Target display aspect ratio (picture width ÷ height) the framebuffer
    /// should fill — e.g. `Some(4.0 / 3.0)` for a system that drove a 4:3 TV.
    /// The harness derives the pixel-stretch from this and the cropped
    /// framebuffer dimensions, so the picture keeps its true proportions
    /// whatever the scanline count. `None` ⇒ square pixels (no stretch).
    fn display_aspect_ratio(&self) -> Option<f32> {
        None
    }

    /// Input sub-slices per frame. Use `1` for runtimes that advance in whole
    /// frames (so a sub-frame target would overshoot); higher values give
    /// finer input latency on runtimes that honour sub-frame targets.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    /// Framebuffer dimensions (pixels) the presenter renders at.
    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32);

    /// Machine ticks in one displayed frame.
    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64;

    /// Wall-clock duration of one displayed frame.
    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration;

    /// The host-control → (port, input-name) map shared by keyboard and gamepad.
    /// A multi-variant system may return a different map per variant by matching
    /// on its own current-variant state (e.g. the Spectrum's Kempston vs IF2
    /// routing) — the harness calls this on the live system each drain.
    fn button_map(&self) -> &'static ButtonInputMap;

    /// The gamepad analogue-axis → (port, axis-name) map (e.g. a stick driving
    /// a joystick's horizontal/vertical). Default: no axes. Like
    /// [`Self::button_map`], it may vary by the system's current variant.
    fn axis_map(&self) -> &'static AxisInputMap {
        &EMPTY_AXIS_MAP
    }

    /// Translate a physical key to a host control (joystick / d-pad), or `None`
    /// to ignore it. Used by consoles; home computers use [`Self::map_keys`].
    fn map_key(&self, _code: KeyCode) -> Option<HostControl> {
        None
    }

    /// Translate a physical key to one or more machine key *names* — the
    /// keyboard path for home computers. Returning several names produces a
    /// hardware combo from one host key (e.g. the Spectrum's cursor keys are
    /// the membrane-wired `Caps`+`5`/`6`/`7`/`8`). When this returns `Some`, the
    /// harness emits an `InputEvent::Key` for each name and does **not** route
    /// the key through the button map. Default: no keyboard (a console — use
    /// [`Self::map_key`] / [`Self::button_map`]).
    fn map_keys(&self, _code: KeyCode) -> Option<&'static [&'static str]> {
        None
    }

    /// Hook run after a hard reset re-initialises the runtime (e.g. re-insert
    /// media for runtimes that drop it on reset). Default: nothing.
    fn after_reset(&mut self, _runtime: &mut Self::Runtime) -> Result<(), MachineError> {
        Ok(())
    }

    /// Hook for per-system key shortcuts beyond the harness's Esc/F12 — e.g.
    /// audio-channel debug toggles. Called on key-down and key-up for any key
    /// the harness doesn't itself own, *before* it's treated as a controller
    /// button; return `true` to consume it (so it isn't also routed through the
    /// button map). Default: owns no extra keys.
    fn handle_key(&mut self, _runtime: &mut Self::Runtime, _code: KeyCode, _pressed: bool) -> bool {
        false
    }

    /// Hook run once when the session ends (window closed or fatal error), for
    /// teardown such as flushing battery-backed RAM to disk. An `Err` becomes a
    /// [`UiError::Teardown`]. Default: nothing.
    fn on_exit(&mut self, _runtime: &mut Self::Runtime) -> Result<(), String> {
        Ok(())
    }

    /// Optional human-readable status when the machine has wedged — e.g. the
    /// CPU executed a JAM/stop-code (usually a bad ROM dump). Returned every
    /// frame; the harness logs it once and appends it to the window title so a
    /// hang reads as "CPU halted" instead of a silent grey screen. `None` means
    /// running normally. Default: never reports a halt.
    fn halt_status(&self, _runtime: &Self::Runtime) -> Option<String> {
        None
    }

    /// The machine variants this system can switch between live (the Machine
    /// menu's variant radio). Empty (the default) ⇒ a single-variant system with
    /// no variant menu. Each `id` is a stable string [`Self::switch_variant`]
    /// accepts (matching the system's script/MCP `set_machine` id).
    fn variants(&self) -> Vec<VariantInfo> {
        Vec::new()
    }

    /// The id of the currently-running variant, for the menu radio check. `None`
    /// ⇒ no variant is distinguished (the default, single-variant case).
    fn current_variant(&self) -> Option<Cow<'static, str>> {
        None
    }

    /// Whether a tape is currently playing — gates the harness's turbo
    /// (fast-load) pacing, which only races ahead while a tape is actually
    /// loading. A system with a tape queries its runtime here (e.g. the
    /// `tape.playing` query or a machine accessor). Default: no tape, never
    /// turbos.
    fn tape_playing(&self, _runtime: &Self::Runtime) -> bool {
        false
    }

    /// File-picker filter for "Open State…", as `(label, extensions)`. `Some`
    /// adds a File → Open State… item (loading an arbitrary state/snapshot file,
    /// distinct from the fixed-slot quick-save); `None` (default) hides it. This
    /// is for *foreign* formats a system parses itself (e.g. the Spectrum's
    /// `.sna`/`.z80`), which `load_media`'s tape/disk contract doesn't cover.
    fn state_open_filter(&self) -> Option<(&'static str, &'static [&'static str])> {
        None
    }

    /// Load an arbitrary state/snapshot file chosen via Open State…. The system
    /// parses it (by extension) and applies it to the runtime. Only called when
    /// [`Self::state_open_filter`] is `Some`. Default: unsupported.
    fn load_state_file(
        &mut self,
        _runtime: &mut Self::Runtime,
        _path: &Path,
    ) -> Result<(), String> {
        Err("this system cannot open state files".to_owned())
    }

    /// Switch the live machine to `variant`, rebuilding it in place — the system
    /// owns firmware loading and the rebuild (e.g. `HeadlessSession::swap_machine`).
    /// The harness then re-paces from the new machine's timing and resets the
    /// frame/audio capture; machine state and loaded media are not preserved
    /// (matching a hardware variant change). Only ids from [`Self::variants`] are
    /// passed. Default: unsupported (single-variant systems never reach here, as
    /// they expose no variant menu).
    fn switch_variant(
        &mut self,
        _runtime: &mut Self::Runtime,
        _variant: &str,
    ) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "switch machine variant",
        })
    }
}

/// Errors that can end a UI session.
#[derive(Debug, Error)]
pub enum UiError {
    #[error(transparent)]
    Machine(#[from] MachineError),
    #[error(transparent)]
    Video(#[from] VideoPresenterError),
    #[error(transparent)]
    EventLoop(#[from] EventLoopError),
    #[error(transparent)]
    Os(#[from] OsError),
    #[error(transparent)]
    Audio(#[from] NativeAudioError),
    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },
    #[error("teardown failed: {0}")]
    Teardown(String),
}

/// Drives a [`MachineCore`] runtime a frame (or sub-frame) at a time, capturing
/// the latest frame and feeding framed audio.
struct Runner<R: MachineCore> {
    runtime: R,
    frame_capture: LatestFrameCapture,
    audio_output: NativeAudioOutput,
    last_run_result: Option<RunResult>,
}

impl<R: MachineCore> Runner<R> {
    fn new(runtime: R) -> Result<Self, UiError> {
        Ok(Self {
            runtime,
            frame_capture: LatestFrameCapture::default(),
            audio_output: NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?,
            last_run_result: None,
        })
    }

    fn run_ticks(&mut self, input_events: &[InputEvent], ticks: u64) -> Result<bool, UiError> {
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

    fn frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
    }
}

/// The winit application: hosts the window, the wgpu presenter, the pacing
/// loop, and keyboard/gamepad input for one [`UiSystem`].
struct App<S: UiSystem> {
    system: S,
    runner: Runner<S::Runtime>,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    full_frame_ticks: u64,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, HostControl>,
    pressed_key_names: HashMap<KeyCode, &'static [&'static str]>,
    /// Held modifier keys, tracked so the save/load chords (Cmd/Ctrl+S / +L)
    /// can be distinguished from the bare keys the machine keyboard uses.
    modifiers: ModifiersState,
    gamepads: NativeGamepadInput,
    window: Option<Arc<Window>>,
    video: Option<WgpuVideoPresenter>,
    presentation: PresentationProfile,
    fatal_error: Option<UiError>,
    halt_message: Option<String>,
    /// Tape fast-load (turbo) armed by the user (F11 / Tape → Fast Load). Only
    /// races ahead while [`UiSystem::tape_playing`] is also true.
    turbo_armed: bool,
    /// Native menu bar (no-op stub on Linux). Owns the muda menu tree.
    app_menu: AppMenu,
    /// Set once the menu has been attached to the OS (in `resumed`).
    menu_installed: bool,
    /// Command channel shared by menu clicks and keyboard shortcuts; drained at
    /// the frame boundary so an action never tears down state mid-frame.
    command_tx: Sender<AppCommand>,
    command_rx: Receiver<AppCommand>,
}

impl<S: UiSystem> App<S> {
    fn new(system: S, runner: Runner<S::Runtime>, scale: u32, video: VideoFilter) -> Self {
        let slices = system.input_slices_per_frame().max(1);
        let frame_ticks = system.frame_ticks(&runner.runtime);
        let frame_duration = system.frame_duration(&runner.runtime);
        let slice_ticks = frame_ticks.div_ceil(u64::from(slices));
        let slice_duration =
            Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(slices));
        // The pixel aspect ratio is derived from the system's target display
        // aspect and the cropped framebuffer size once the window opens (see
        // `create_window`); square pixels until then.
        let presentation = PresentationProfile::for_filter(video);
        let variants = system.variants();
        let current_variant = system.current_variant();
        let state_open = system.state_open_filter().is_some();
        let app_menu = AppMenu::new(
            &system.window_title(),
            scale,
            video,
            &runner.runtime.profile().media_slots,
            &variants,
            current_variant.as_deref(),
            state_open,
        );
        let (command_tx, command_rx) = channel();
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
            pressed_key_names: HashMap::new(),
            modifiers: ModifiersState::empty(),
            gamepads: NativeGamepadInput::new(),
            window: None,
            video: None,
            presentation,
            fatal_error: None,
            halt_message: None,
            turbo_armed: false,
            app_menu,
            menu_installed: false,
            command_tx,
            command_rx,
        }
    }

    /// Surface a machine halt (e.g. a CPU JAM). On the transition into a halt:
    /// log it, append it to the window title, and (via [`Self::render`]) draw
    /// the on-screen overlay. Clears again — restoring the title — when the
    /// machine resumes, e.g. after a reset. Idempotent per state.
    fn update_halt_status(&mut self) {
        let status = self.system.halt_status(&self.runner.runtime);
        if status == self.halt_message {
            return;
        }
        match &status {
            Some(message) => {
                eprintln!("warning: {message}");
                if let Some(window) = &self.window {
                    window.set_title(&format!("{} — {message}", self.system.window_title()));
                }
            }
            None => {
                if let Some(window) = &self.window {
                    window.set_title(&self.system.window_title());
                }
            }
        }
        self.halt_message = status;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn take_error(&mut self) -> Option<UiError> {
        self.fatal_error.take()
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: UiError) {
        eprintln!("error: {err}");
        self.fatal_error = Some(err);
        event_loop.exit();
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), UiError> {
        if self.window.is_some() {
            return Ok(());
        }
        let (fb_width, fb_height) = self.system.framebuffer_size(&self.runner.runtime);
        // Derive the pixel aspect ratio from the system's target display aspect
        // and the cropped framebuffer dimensions, so the picture fills its
        // intended shape (e.g. 4:3) for whatever scanline count is shown. The
        // presenter and the window size then stretch width to match.
        self.presentation.pixel_aspect_ratio = match self.system.display_aspect_ratio() {
            Some(aspect) if fb_width > 0 && fb_height > 0 => {
                aspect * fb_height as f32 / fb_width as f32
            }
            _ => 1.0,
        };
        let par = f64::from(self.presentation.pixel_aspect_ratio).max(f64::MIN_POSITIVE);
        let display_width = f64::from(fb_width) * par;
        let attributes = WindowAttributes::default()
            .with_title(self.system.window_title())
            .with_inner_size(self.window_logical_size(self.scale))
            .with_min_inner_size(LogicalSize::new(display_width, f64::from(fb_height)));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let video = WgpuVideoPresenter::new(window.clone(), fb_width, fb_height)?;
        self.window = Some(window);
        self.video = Some(video);
        self.next_slice_at = Instant::now();
        Ok(())
    }

    /// The logical window size for an integer `scale`: the cropped framebuffer
    /// stretched by the derived pixel-aspect ratio (so the picture keeps its
    /// 4:3-or-whatever shape) and then by `scale`. Used both at window creation
    /// and by the View → Window Scale menu.
    fn window_logical_size(&self, scale: u32) -> LogicalSize<f64> {
        let (fb_width, fb_height) = self.system.framebuffer_size(&self.runner.runtime);
        let par = f64::from(self.presentation.pixel_aspect_ratio).max(f64::MIN_POSITIVE);
        let display_width = f64::from(fb_width) * par;
        LogicalSize::new(
            display_width * f64::from(scale),
            f64::from(fb_height.saturating_mul(scale)),
        )
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn advance_machine(&mut self) -> Result<bool, UiError> {
        self.gamepads.drain_events_with_axes(
            self.system.button_map(),
            self.system.axis_map(),
            &mut self.pending_inputs,
        );

        // Turbo (fast-load): while a tape is playing and the user armed it, run
        // unthrottled in a bounded burst so the loader races ahead. `about_to_wait`
        // sets `Poll` in this state so bursts run back-to-back.
        if self.turbo_active() {
            let mut frame_completed = false;
            for _ in 0..MAX_TURBO_FRAMES {
                let inputs = std::mem::take(&mut self.pending_inputs);
                frame_completed |= self.runner.run_ticks(&inputs, self.full_frame_ticks)?;
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
            frame_completed |= self.runner.run_ticks(&inputs, self.slice_ticks)?;
            self.next_slice_at += self.slice_duration;
            ran += 1;
        }
        if ran == max_slices && Instant::now() >= self.next_slice_at {
            self.next_slice_at = Instant::now() + self.slice_duration;
        }
        Ok(frame_completed)
    }

    fn render(&mut self) -> Result<(), UiError> {
        // A halted machine draws the diagnostic overlay over the frozen frame
        // instead of presenting it — so the cause is on-screen, not a mystery.
        if let Some(message) = &self.halt_message {
            let (width, height) = self.system.framebuffer_size(&self.runner.runtime);
            let pixels = overlay::build_halt_overlay(width, height, message);
            let frame = CapturedFrame {
                timestamp: MachineTime::new(0),
                format: PixelFormat::Rgba8888,
                width,
                height,
                palette: None,
                pixels,
            };
            if let Some(video) = self.video.as_mut() {
                video.present(&frame, &self.presentation)?;
            }
            return Ok(());
        }

        let (Some(frame), Some(video)) = (self.runner.frame(), self.video.as_mut()) else {
            return Ok(());
        };
        video.present(frame, &self.presentation)?;
        Ok(())
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        if let Some(video) = self.video.as_mut() {
            video.resize_surface(width, height);
        }
    }

    fn queue_key_state(&mut self, code: KeyCode, pressed: bool) {
        // Keyboard path (home computers): one host key → one or more machine key
        // names, emitted as InputEvent::Key. Takes precedence over the joystick
        // button map.
        if let Some(names) = self.system.map_keys(code) {
            if pressed {
                if self.pressed_key_names.contains_key(&code) {
                    return;
                }
                self.pressed_key_names.insert(code, names);
                self.push_key_events(names, true);
            } else if let Some(names) = self.pressed_key_names.remove(&code) {
                self.push_key_events(names, false);
            }
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

    /// Emit an `InputEvent::Key` for each machine key name in a host key's
    /// mapping (multiple names form a hardware combo).
    fn push_key_events(&mut self, names: &'static [&'static str], pressed: bool) {
        for name in names {
            self.pending_inputs.push(InputEvent::Key {
                name: (*name).into(),
                pressed,
            });
        }
        self.next_slice_at = Instant::now();
    }

    fn release_all_keys(&mut self) {
        let map = self.system.button_map();
        for control in std::mem::take(&mut self.pressed_keys).into_values() {
            if let Some(input) = map.event(control, false) {
                self.pending_inputs.push(input);
            }
        }
        for names in std::mem::take(&mut self.pressed_key_names).into_values() {
            self.push_key_events(names, false);
        }
        self.next_slice_at = Instant::now();
    }

    /// Hard-reset the machine: reset the runtime, run the system's re-init
    /// hook, clear capture/audio, and run one frame so a picture is ready.
    fn reset_machine(&mut self) -> Result<(), UiError> {
        self.release_all_keys();
        self.runner.runtime.reset(ResetKind::Hard);
        self.system.after_reset(&mut self.runner.runtime)?;
        self.runner.frame_capture = LatestFrameCapture::default();
        self.runner.audio_output.clear();
        self.runner.last_run_result = None;
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        Ok(())
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
    fn quick_load(&mut self) -> Result<(), UiError> {
        let Some(path) = self.state_slot_path() else {
            eprintln!("load-state: no state directory (set EMU198X_STATE_DIR or HOME)");
            return Ok(());
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("load-state: no saved state at {} ({err})", path.display());
                return Ok(());
            }
        };
        if let Err(err) = self.runner.runtime.restore(&bytes) {
            eprintln!(
                "load-state: {} could not be restored: {err}",
                path.display()
            );
            return Ok(());
        }
        // Drop held keys and stale capture/audio, then run one frame so the
        // restored picture is on screen immediately.
        self.release_all_keys();
        self.runner.frame_capture = LatestFrameCapture::default();
        self.runner.audio_output.clear();
        self.runner.last_run_result = None;
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        println!("load-state: restored {}", path.display());
        Ok(())
    }

    /// Returns `true` if the key was consumed as a UI shortcut (harness Esc/F12,
    /// the Cmd/Ctrl+S / +L save-state chords, or a per-system
    /// [`UiSystem::handle_key`] shortcut), so it isn't also routed to the
    /// machine.
    fn handle_shortcut(
        &mut self,
        event_loop: &ActiveEventLoop,
        code: KeyCode,
        pressed: bool,
    ) -> bool {
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
            KeyCode::Escape => {
                if pressed {
                    event_loop.exit();
                }
                true
            }
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
    fn handle_command(&mut self, event_loop: &ActiveEventLoop, command: AppCommand) {
        match command {
            AppCommand::Reset => {
                if let Err(err) = self.reset_machine() {
                    self.fail(event_loop, err);
                }
            }
            AppCommand::QuickSave => self.quick_save(),
            AppCommand::QuickLoad => {
                if let Err(err) = self.quick_load() {
                    self.fail(event_loop, err);
                }
            }
            AppCommand::SetScale(scale) => self.set_window_scale(scale),
            AppCommand::SetFilter(filter) => self.set_video_filter(filter),
            AppCommand::OpenMedia { slot, kind } => {
                if let Err(err) = self.open_media(&slot, kind) {
                    self.fail(event_loop, err);
                }
            }
            AppCommand::SwitchVariant(id) => {
                if let Err(err) = self.switch_to_variant(&id) {
                    self.fail(event_loop, err);
                }
            }
            AppCommand::MediaTransport { slot, action } => self.media_transport(&slot, action),
            AppCommand::ToggleTurbo => self.toggle_turbo(),
            AppCommand::OpenState => {
                if let Err(err) = self.open_state() {
                    self.fail(event_loop, err);
                }
            }
        }
    }

    /// File → Open State…: pop a native picker filtered to the system's state
    /// formats, hand the chosen file to [`UiSystem::load_state_file`], and
    /// refresh the picture. A cancelled dialog or a rejected file is reported and
    /// ignored; only a genuine emulation error from the refresh frame propagates.
    fn open_state(&mut self) -> Result<(), UiError> {
        let Some((label, extensions)) = self.system.state_open_filter() else {
            return Ok(());
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open State")
            .add_filter(label, extensions)
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return Ok(()); // user cancelled
        };
        if let Err(err) = self.system.load_state_file(&mut self.runner.runtime, &path) {
            eprintln!("state: cannot load {}: {err}", path.display());
            return Ok(());
        }
        self.release_all_keys();
        self.runner.frame_capture = LatestFrameCapture::default();
        self.runner.audio_output.clear();
        self.runner.last_run_result = None;
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        println!("state: loaded {}", path.display());
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        Ok(())
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

    /// Tape → Fast Load (or F11): arm/disarm turbo. It only races while a tape is
    /// playing ([`Self::turbo_active`]); a fresh baseline keeps pacing smooth
    /// when it turns off mid-load.
    fn toggle_turbo(&mut self) {
        self.turbo_armed = !self.turbo_armed;
        self.next_slice_at = Instant::now();
        self.app_menu.set_turbo_armed(self.turbo_armed);
        println!(
            "tape fast-load {}",
            if self.turbo_armed { "armed" } else { "off" }
        );
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
    fn switch_to_variant(&mut self, variant: &str) -> Result<(), UiError> {
        let old_size = self.system.framebuffer_size(&self.runner.runtime);
        if let Err(err) = self
            .system
            .switch_variant(&mut self.runner.runtime, variant)
        {
            eprintln!("machine: cannot switch to variant {variant}: {err}");
            if let Some(current) = self.system.current_variant() {
                self.app_menu.set_current_variant(&current);
            }
            return Ok(());
        }
        // Re-pace from the new machine's timing and drop stale input + capture.
        self.recompute_pacing();
        self.release_all_keys();
        self.runner.frame_capture = LatestFrameCapture::default();
        self.runner.audio_output.clear();
        self.runner.last_run_result = None;
        // The framebuffer size can differ between variants; rebuild the presenter
        // and resize the window if so.
        let new_size = self.system.framebuffer_size(&self.runner.runtime);
        if new_size != old_size
            && let Some(window) = &self.window
        {
            let (w, h) = new_size;
            self.video = Some(WgpuVideoPresenter::new(window.clone(), w, h)?);
            let _ = window.request_inner_size(self.window_logical_size(self.scale));
        }
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        if let Some(current) = self.system.current_variant() {
            self.app_menu.set_current_variant(&current);
        }
        if let Some(window) = &self.window {
            window.set_title(&self.system.window_title());
            window.request_redraw();
        }
        Ok(())
    }

    /// File → Open: pop a native file picker filtered to the slot's media kind,
    /// load the chosen image into the named slot via [`MachineCore::load_media`],
    /// and refresh the picture. A cancelled dialog, an unreadable file, or a
    /// rejected load is reported and ignored; only a genuine emulation error
    /// from the refresh frame propagates. The dialog blocks the loop while open
    /// — fine, since it's user-initiated and the machine simply pauses.
    fn open_media(&mut self, slot: &str, kind: MediaKind) -> Result<(), UiError> {
        let (label, extensions) = media_filter(kind);
        let Some(path) = rfd::FileDialog::new()
            .set_title(format!("Open {label}"))
            .add_filter(label, extensions)
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return Ok(()); // user cancelled
        };
        let loaded = match read_media_asset(&path, kind) {
            Ok(loaded) => loaded,
            Err(err) => {
                eprintln!("media: cannot read {}: {err}", path.display());
                return Ok(());
            }
        };
        let mut set = MediaSet::new();
        set.push(MediaImage::new(slot.to_owned(), kind, &loaded.bytes));
        if let Err(err) = self.runner.runtime.load_media(&set) {
            eprintln!("media: {} rejected by the machine: {err}", path.display());
            return Ok(());
        }
        // Fresh capture/audio, then one frame so the inserted media shows. Some
        // cartridge machines need a Reset (Machine → Reset) to pick it up.
        self.release_all_keys();
        self.runner.frame_capture = LatestFrameCapture::default();
        self.runner.audio_output.clear();
        self.runner.last_run_result = None;
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        println!("media: loaded {} into slot \"{slot}\"", path.display());
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        Ok(())
    }

    /// Resize the window to an integer multiple of the native frame. The wgpu
    /// surface follows via the resulting [`WindowEvent::Resized`].
    fn set_window_scale(&mut self, scale: u32) {
        self.scale = scale;
        let size = self.window_logical_size(scale);
        if let Some(window) = &self.window {
            let _ = window.request_inner_size(size);
            window.request_redraw();
        }
        self.app_menu.set_current_scale(scale);
    }

    /// Switch the post-framebuffer video filter, preserving the derived
    /// pixel-aspect ratio (which `PresentationProfile::for_filter` resets).
    fn set_video_filter(&mut self, filter: VideoFilter) {
        let par = self.presentation.pixel_aspect_ratio;
        self.presentation = PresentationProfile::for_filter(filter);
        self.presentation.pixel_aspect_ratio = par;
        self.app_menu.set_current_filter(filter);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl<S: UiSystem> ApplicationHandler for App<S> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.create_window(event_loop) {
            self.fail(event_loop, err);
            return;
        }
        // Attach the native menu once the app is resumed (the macOS NSApp now
        // exists). Idempotent via the flag, since `resumed` can fire again.
        if !self.menu_installed {
            self.app_menu.install();
            self.menu_installed = true;
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window_id() != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Focused(false) => {
                self.modifiers = ModifiersState::empty();
                self.release_all_keys();
            }
            WindowEvent::Resized(size) => self.resize_surface(size.width, size.height),
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    self.resize_surface(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.repeat {
                    return;
                }
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if self.handle_shortcut(event_loop, code, pressed) {
                    return;
                }
                self.queue_key_state(code, pressed);
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.render() {
                    self.fail(event_loop, err);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Translate native-menu clicks into commands on our channel. muda's
        // receiver is a global crossbeam channel; non-Linux only.
        #[cfg(not(target_os = "linux"))]
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some(command) = self.app_menu.command_for(&event.id) {
                let _ = self.command_tx.send(command);
            }
        }
        // Drain queued commands (menu + shortcuts) at the frame boundary, so an
        // action never tears down state the current frame is using.
        while let Ok(command) = self.command_rx.try_recv() {
            self.handle_command(event_loop, command);
        }

        match self.advance_machine() {
            Ok(true) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Ok(false) => {}
            Err(err) => {
                self.fail(event_loop, err);
                return;
            }
        }
        self.update_halt_status();
        // While turbo-loading, poll back-to-back so the fast-load bursts run with
        // no wait between them; otherwise wake at the next paced slice.
        if self.turbo_active() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_slice_at));
        }
    }
}

/// A tape-transport action a function key maps to (only honoured on a system
/// that has a tape slot and doesn't itself use the key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TapeShortcut {
    Play,
    Stop,
    ToggleTurbo,
}

/// The tape-transport shortcut for a key, if any: F9 play, F10 stop, F11
/// toggle fast-load — matching the Spectrum/C64 bespoke runners.
fn tape_transport_shortcut(code: KeyCode) -> Option<TapeShortcut> {
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
enum StateAction {
    Save,
    Load,
}

/// Decide whether a key event is a save-state chord. The chord fires only on
/// key-*down* with Cmd (macOS) or Ctrl (elsewhere) held — never on a bare key,
/// so the machine keyboard keeps its S / L and F-keys, and never on release.
fn state_chord_action(
    modifiers: ModifiersState,
    code: KeyCode,
    pressed: bool,
) -> Option<StateAction> {
    if !pressed || !(modifiers.super_key() || modifiers.control_key()) {
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
fn media_filter(kind: MediaKind) -> (&'static str, &'static [&'static str]) {
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
fn slot_file_name(profile_id: &str) -> String {
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

/// Open a native window for `system` driving `runtime`, and run the winit
/// event loop until the window closes or a fatal error surfaces.
///
/// The runtime should already be initialised (media loaded); the harness runs
/// one frame up front so the first redraw has a picture.
///
/// # Errors
///
/// Returns [`UiError`] for an invalid scale, audio/video init failure, a
/// machine error, or an event-loop error.
pub fn run<S: UiSystem>(
    system: S,
    runtime: S::Runtime,
    scale: u32,
    video: VideoFilter,
) -> Result<(), UiError> {
    if scale == 0 {
        return Err(UiError::InvalidScale { value: scale });
    }
    // Harness-global controls every system shares, printed once so each runner's
    // own per-machine controls line doesn't have to repeat them.
    println!("Save-state: Cmd/Ctrl+S quick-save, Cmd/Ctrl+L quick-load (one slot per machine).");
    let mut runner = Runner::new(runtime)?;
    let frame_ticks = system.frame_ticks(&runner.runtime);
    runner.run_ticks(&[], frame_ticks)?;

    let mut app = App::new(system, runner, scale, video);
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    // Teardown (e.g. flush battery RAM) runs whether the session ended cleanly
    // or via a fatal error, so a crash still persists state. A fatal emulation
    // error takes precedence over a teardown failure (the latter is then just
    // logged), so the real cause isn't masked by a failed battery write.
    let teardown = app.system.on_exit(&mut app.runner.runtime);
    if let Some(err) = app.take_error() {
        if let Err(reason) = teardown {
            eprintln!("error: teardown failed: {reason}");
        }
        return Err(err);
    }
    teardown.map_err(UiError::Teardown)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_file_name_keeps_safe_chars_and_adds_extension() {
        assert_eq!(
            slot_file_name("sega-master-system-ntsc"),
            "sega-master-system-ntsc.state"
        );
        assert_eq!(slot_file_name("zx_spectrum_48k"), "zx_spectrum_48k.state");
    }

    #[test]
    fn slot_file_name_sanitises_path_and_other_separators() {
        // Anything outside [A-Za-z0-9_-] becomes '-', so the slot can never
        // escape the state directory or collide with path syntax.
        assert_eq!(slot_file_name("a/b"), "a-b.state");
        assert_eq!(slot_file_name("../evil"), "---evil.state");
        assert_eq!(slot_file_name("space here.v2"), "space-here-v2.state");
    }

    #[test]
    fn variant_info_accepts_static_and_owned_strings() {
        // The constructor takes anything Into<Cow>, so a system can build the
        // list from &'static labels or runtime-formatted Strings alike.
        let from_static = VariantInfo::new("zx-48k", "ZX Spectrum 48K");
        assert_eq!(from_static.id, "zx-48k");
        assert_eq!(from_static.label, "ZX Spectrum 48K");

        let from_owned =
            VariantInfo::new(String::from("zx-128k"), format!("ZX Spectrum {}", "128K"));
        assert_eq!(from_owned.id, "zx-128k");
        assert_eq!(from_owned.label, "ZX Spectrum 128K");

        // The id is what `AppCommand::SwitchVariant` carries back to the system.
        assert_eq!(
            AppCommand::SwitchVariant(from_owned.id.clone()),
            AppCommand::SwitchVariant(Cow::Borrowed("zx-128k"))
        );
    }

    #[test]
    fn tape_transport_shortcuts_match_the_bespoke_runners() {
        // F9 play / F10 stop / F11 turbo — the Spectrum/C64 layout.
        assert_eq!(
            tape_transport_shortcut(KeyCode::F9),
            Some(TapeShortcut::Play)
        );
        assert_eq!(
            tape_transport_shortcut(KeyCode::F10),
            Some(TapeShortcut::Stop)
        );
        assert_eq!(
            tape_transport_shortcut(KeyCode::F11),
            Some(TapeShortcut::ToggleTurbo)
        );
        // Other keys are not transport — they reach the machine (or other paths).
        assert_eq!(tape_transport_shortcut(KeyCode::F8), None);
        assert_eq!(tape_transport_shortcut(KeyCode::KeyL), None);
    }

    #[test]
    fn media_filter_maps_kinds_to_extensions() {
        assert_eq!(media_filter(MediaKind::Tape).0, "Tape images");
        assert!(media_filter(MediaKind::Disk).1.contains(&"dsk"));
        assert!(media_filter(MediaKind::Cartridge).1.contains(&"bin"));
        assert!(media_filter(MediaKind::Snapshot).1.contains(&"sna"));
    }

    #[test]
    fn save_state_chord_requires_a_modifier_and_fires_on_press_only() {
        let ctrl = ModifiersState::CONTROL;
        let cmd = ModifiersState::SUPER;
        let none = ModifiersState::empty();

        // Cmd/Ctrl + S/L on key-down → the matching action.
        assert_eq!(
            state_chord_action(ctrl, KeyCode::KeyS, true),
            Some(StateAction::Save)
        );
        assert_eq!(
            state_chord_action(cmd, KeyCode::KeyL, true),
            Some(StateAction::Load)
        );

        // Bare S / L (no modifier) stay with the machine keyboard.
        assert_eq!(state_chord_action(none, KeyCode::KeyS, true), None);
        assert_eq!(state_chord_action(none, KeyCode::KeyL, true), None);

        // Release never fires (the press was already consumed).
        assert_eq!(state_chord_action(ctrl, KeyCode::KeyS, false), None);

        // Other modified keys are not chords — e.g. Ctrl+F5 stays free.
        assert_eq!(state_chord_action(ctrl, KeyCode::F5, true), None);
    }
}
