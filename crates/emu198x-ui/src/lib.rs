//! Shared native-UI harness for Emu198x runners.
//!
//! Six runners (NES, Game Boy, Amiga, C64, Dragon, Spectrum) each carried a
//! near-identical winit `ApplicationHandler` + wgpu video + framed audio +
//! keyboard/gamepad-input loop. This crate factors that common spine out: a
//! runner supplies a small [`UiSystem`] descriptor (its runtime, framebuffer
//! size, frame timing, button map, and key map) and calls [`run`] to get a
//! native window with `raw`/`lcd`/`crt` video filters, framed audio, gamepad
//! input, F12-reset, Cmd/Ctrl+S / +L quick save-states, and a native
//! menu bar (App / File / Machine / State / View; see [`menu`]).
//!
//! The File menu's media open is built from each machine's declared media
//! slots, so it needs no per-system code. Still to come for the menu: machine
//! variant switching (a live-runtime trait) and multi-slot save-states.

mod export;
mod frame_stats;
#[cfg(all(target_os = "linux", feature = "gtk"))]
mod gtk_host;
mod host_audio;
mod host_core;
mod icon;
mod keyboard;
pub mod launch;
mod media_open;
mod menu;
mod overlay;
mod pointer;
mod quick_load;

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use host_core::{FileFilter, HostCore, MenuSync, Notice, Runner, Shell, Wake};
use menu::{AppCommand, AppMenu};

use emu198x_native_video::{VideoPresenterError, WgpuVideoPresenter};
use emu198x_shell::{CapturedFrame, MachineCore, MachineError, MachineTime, PixelFormat};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowAttributes, WindowId};

// Re-exported so a `UiSystem` impl can describe itself without depending on
// winit / native-video / shell input types directly.
pub use emu198x_native_video::VideoFilter;
pub use emu198x_shell::display::Display;
pub use emu198x_shell::{
    AxisInputMap, AxisTarget, ButtonInputMap, ButtonTarget, HostAxis, HostControl,
};
pub use keyboard::KeywordModifier;
pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

/// The empty axis map returned by [`UiSystem::axis_map`]'s default — a system
/// with no analogue inputs gets no axis routing.
static EMPTY_AXIS_MAP: AxisInputMap = AxisInputMap::new(&[]);

/// A switchable machine variant for the Machine menu's variant radio. `id` is a
/// stable string the system maps back to its model — the same id its script/MCP
/// `set_machine` accepts — and `label` is the human-readable menu text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantInfo {
    /// Stable variant identifier (round-trips through [`UiSystem::switch_variant`]).
    pub id: Cow<'static, str>,
    /// Human-readable menu label.
    pub label: Cow<'static, str>,
    /// Optional submenu label, such as a base model containing several presets.
    /// Ungrouped entries remain directly under Machine.
    pub group: Option<Cow<'static, str>>,
}

impl VariantInfo {
    /// Build a [`VariantInfo`] from anything that converts into a `Cow`.
    pub fn new(id: impl Into<Cow<'static, str>>, label: impl Into<Cow<'static, str>>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            group: None,
        }
    }

    /// Put this choice under a shared submenu, preserving its switch id.
    #[must_use]
    pub fn in_group(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.group = Some(label.into());
        self
    }
}

/// One selectable drive model for a configurable device port (the Machine →
/// Drives radios). `id` round-trips through [`UiSystem::set_port_drive`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriveOption {
    /// Stable model identifier (e.g. `"none"`, `"1541"`).
    pub id: Cow<'static, str>,
    /// Human-readable menu label.
    pub label: Cow<'static, str>,
    /// Whether this model can currently be selected (its firmware is present).
    /// A `false` option is shown disabled so the user sees why it is unavailable.
    pub available: bool,
}

/// One configurable device port and the drive models it can hold (the Machine →
/// Drives submenu). Systems without swappable drives return no ports (default),
/// so no Drives menu appears.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrivePortInfo {
    /// The device number this port answers to (e.g. an IEC device 8–11).
    pub device: u8,
    /// Human-readable port label (e.g. `"Device 8"`).
    pub label: Cow<'static, str>,
    /// The models offered for this port, in menu order.
    pub options: Vec<DriveOption>,
    /// The `id` of the currently-selected option, for the radio check.
    pub current: Cow<'static, str>,
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

    /// Default video filter when the CLI doesn't override it. Raw for
    /// every machine but the Dragon, whose window opens on the CRT filter.
    fn default_video(&self) -> VideoFilter {
        VideoFilter::Raw
    }

    /// What this machine's video output reached — a television, a monitor, or
    /// a panel. The harness derives the pixel aspect from it.
    ///
    /// Takes the runtime because the answer can move underneath: a variant
    /// switch can cross regions, and a machine with a display card drives a
    /// monitor from a chipset that also feeds a set. `None` means unstated,
    /// and gives square pixels.
    ///
    /// See `knowledge/decisions/pixel-aspect-comes-from-the-raster.md`.
    /// Defaults to the runtime's own answer, which is where this belongs: a
    /// display is a fact about the machine, and stating it there puts it on
    /// the query surface for headless and MCP as well as the window. Override
    /// only for a system whose runtime has not been moved yet.
    fn display(&self, runtime: &Self::Runtime) -> Option<Display> {
        runtime.display()
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

    /// Whether this system offers host-layout character input, enabled by default.
    fn supports_host_keyboard(&self) -> bool {
        false
    }

    /// Translate one host character to target key names, using the same names
    /// as `map_keys`. Unsupported characters must return `None`.
    fn host_character_keys(&self, _runtime: &Self::Runtime, _ch: char) -> Option<Vec<String>> {
        None
    }

    /// Optional explicit target-key modifier in host character mode.
    /// Holding it uses physical target keys; releasing it restores layout typing.
    fn host_keyword_modifier(&self) -> Option<KeywordModifier> {
        None
    }

    /// Named controls in host mode, such as arrows and editing keys.
    fn host_control_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        self.map_keys(code)
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

    /// The pointer device name (e.g. `"mouse-1"`) if this system has a mouse,
    /// else `None` (default). `Some` opts the window into mouse capture: the
    /// harness turns the cursor into `InputEvent::PointerMotion` counts and
    /// clicks into `InputEvent::PointerButton`, both tagged with this device
    /// name. The counts chase the guest's own pointer when
    /// [`guest_pointer`](Self::guest_pointer) can locate it, and otherwise
    /// replay the cursor's movement in framebuffer pixels.
    fn mouse_device(&self) -> Option<&'static str> {
        None
    }

    /// Where the guest's own mouse pointer is, in framebuffer pixels, or
    /// `None` (default) if the machine cannot say — no pointer on screen, or
    /// no way to find it.
    ///
    /// `Some` lets the harness keep that pointer under the host cursor: after
    /// each frame it sends the mouse counts that close part of the gap, and it
    /// hides the host cursor over the window so the guest's pointer stands in
    /// for it. Without it the harness can only replay the cursor's movement,
    /// which drifts once the guest accelerates the pointer or stops it at a
    /// screen edge. Only consulted when [`mouse_device`](Self::mouse_device)
    /// is `Some`.
    fn guest_pointer(&self, _runtime: &Self::Runtime) -> Option<(f64, f64)> {
        None
    }

    /// Mouse counts that move the guest pointer one framebuffer pixel on each
    /// axis, before any acceleration the guest applies. Default one count per
    /// pixel. The closed loop corrects for an inexact figure, so this only has
    /// to be about right; it sets how quickly the pointer catches up.
    fn pointer_counts_per_pixel(&self) -> (f64, f64) {
        (1.0, 1.0)
    }

    /// The controller port an absolute pointing device is plugged into — a
    /// light gun, light pen or tablet — or `None` (default) if this system has
    /// none.
    ///
    /// `Some` opts the window into reporting where the cursor *is* rather than
    /// how far it moved: the harness sends [`InputEvent::Aim`] on every cursor
    /// move, normalized across the framebuffer, and `None` when the pointer
    /// leaves the window. A system may have both this and a
    /// [`mouse_device`](Self::mouse_device); they answer different questions
    /// and neither can be derived from the other.
    fn aim_port(&self) -> Option<u8> {
        None
    }

    /// Control name a host mouse button maps to on the aiming device's port,
    /// or `None` to ignore it. Only consulted when
    /// [`Self::aim_port`](Self::aim_port) is `Some`.
    ///
    /// Default: the left button is the trigger. A light gun's trigger is an
    /// ordinary control on its port — on the Master System it is the same pin
    /// a pad uses for button 1 — so this goes out as
    /// [`InputEvent::Button`], not as a pointer event.
    fn map_aim_button(&self, button: MouseButton) -> Option<&'static str> {
        match button {
            MouseButton::Left => Some("trigger"),
            _ => None,
        }
    }

    /// Name for a host mouse button, or `None` to ignore it. Only consulted when
    /// [`Self::mouse_device`] is `Some`. Default: the standard three buttons.
    fn map_mouse_button(&self, button: MouseButton) -> Option<&'static str> {
        match button {
            MouseButton::Left => Some("left"),
            MouseButton::Right => Some("right"),
            MouseButton::Middle => Some("middle"),
            _ => None,
        }
    }

    /// File type for exporting a tape recording. `None` hides the command.
    fn tape_export_filter(&self) -> Option<(&'static str, &'static str)> {
        None
    }

    /// Decode the recorded tape without consuming it. The UI owns the picker,
    /// writing and feedback. Called only when `tape_export_filter` is `Some`.
    fn export_tape(&self, _runtime: &Self::Runtime) -> Result<Vec<u8>, String> {
        Err("Tape recording export is not available for this system.".to_owned())
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

    /// The configurable device ports and their drive-model options (the Machine
    /// → Drives menu). Read from the runtime so the current selection and each
    /// model's availability reflect live state. Empty (the default) ⇒ no Drives
    /// menu.
    fn drive_ports(&self, _runtime: &Self::Runtime) -> Vec<DrivePortInfo> {
        Vec::new()
    }

    /// Set the drive model on `device` to the option `kind_id` (from
    /// [`Self::drive_ports`]), applying it to the live runtime. Unlike a variant
    /// switch this does not rebuild the machine or change timing — it swaps a
    /// drive on the bus — so the harness only refreshes the radio afterwards.
    /// Default: unsupported (systems with no drive ports never reach here).
    fn set_port_drive(
        &mut self,
        _runtime: &mut Self::Runtime,
        _device: u8,
        _kind_id: &str,
    ) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "set port drive",
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
    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },
    #[error("phosphor decay must be finite and non-negative, got {value}")]
    InvalidPhosphorDecay { value: f32 },
    #[error("teardown failed: {0}")]
    Teardown(String),
    #[error("the window could not open: {0}")]
    Host(String),
}

/// The winit application: owns the window, the wgpu presenter and the native
/// menu bar, and feeds a [`HostCore`] its events.
struct App<S: UiSystem> {
    core: HostCore<S>,
    window: Option<Arc<Window>>,
    video: Option<WgpuVideoPresenter>,
    frame_stats: Option<frame_stats::FrameStats>,
    fatal_error: Option<UiError>,
    /// Native menu bar (no-op stub on Linux). Owns the muda menu tree.
    app_menu: AppMenu,
    /// Set once the menu has been attached to the OS (in `resumed`).
    menu_installed: bool,
    /// Whether the host cursor is currently hidden over the window, so
    /// [`Self::sync_host_cursor`] only touches the window on a change.
    host_cursor_hidden: bool,
    /// Where menu clicks go: the core's command channel.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    command_tx: Sender<AppCommand>,
}

/// The winit window, presenter and menu, lent to the core as its [`Shell`].
struct WinitShell<'a> {
    window: Option<&'a Arc<Window>>,
    video: &'a mut Option<WgpuVideoPresenter>,
    menu: &'a AppMenu,
}

impl Shell for WinitShell<'_> {
    fn set_title(&mut self, title: &str) {
        if let Some(window) = self.window {
            window.set_title(title);
        }
    }

    fn request_redraw(&mut self) {
        if let Some(window) = self.window {
            window.request_redraw();
        }
    }

    fn request_picture_size(&mut self, width: f64, height: f64) {
        if let Some(window) = self.window {
            let _ = window.request_inner_size(LogicalSize::new(width, height));
        }
    }

    fn framebuffer_resized(&mut self, width: u32, height: u32) -> Result<(), UiError> {
        if let Some(window) = self.window {
            *self.video = Some(WgpuVideoPresenter::new(window.clone(), width, height)?);
        }
        Ok(())
    }

    fn reset_video_history(&mut self) {
        if let Some(video) = self.video.as_mut() {
            video.reset_temporal_history();
        }
    }

    fn sync_menu(&mut self, change: MenuSync<'_>) {
        match change {
            MenuSync::Scale(scale) => self.menu.set_current_scale(scale),
            MenuSync::Filter(filter) => self.menu.set_current_filter(filter),
            MenuSync::Variant(id) => self.menu.set_current_variant(id),
            MenuSync::PortDrive { device, kind_id } => {
                self.menu.set_current_port_drive(device, kind_id);
            }
            MenuSync::Turbo(armed) => self.menu.set_turbo_armed(armed),
            MenuSync::HostKeyboard(enabled) => self.menu.set_host_keyboard(enabled),
        }
    }

    fn pick_file(&mut self, title: &str, (label, extensions): FileFilter<'_>) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title(title)
            .add_filter(label, extensions)
            .add_filter("All files", &["*"])
            .pick_file()
    }

    fn pick_save_file(
        &mut self,
        title: &str,
        file_name: &str,
        (label, extensions): FileFilter<'_>,
        all_files: bool,
    ) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(file_name)
            .add_filter(label, extensions);
        if all_files {
            dialog = dialog.add_filter("All files", &["*"]);
        }
        dialog.save_file()
    }

    fn message(&mut self, title: &str, level: Notice, text: &str) {
        rfd::MessageDialog::new()
            .set_title(title)
            .set_level(match level {
                Notice::Info => rfd::MessageLevel::Info,
                Notice::Warning => rfd::MessageLevel::Warning,
                Notice::Error => rfd::MessageLevel::Error,
            })
            .set_description(text)
            .show();
    }
}

impl<S: UiSystem> App<S> {
    fn new(core: HostCore<S>, video: VideoFilter) -> Self {
        let system = &core.system;
        let runtime = &core.runner.runtime;
        let variants = system.variants();
        let current_variant = system.current_variant();
        let drive_ports = system.drive_ports(runtime);
        let app_menu = AppMenu::new(
            &system.window_title(),
            core.scale,
            video,
            &runtime.profile().media_slots,
            &crate::menu::MachineMenu {
                variants: &variants,
                current_variant: current_variant.as_deref(),
                drive_ports: &drive_ports,
                host_keyboard: system.supports_host_keyboard(),
            },
            system.state_open_filter().is_some(),
            system.tape_export_filter().is_some(),
        );
        let command_tx = core.command_sender();
        Self {
            core,
            window: None,
            video: None,
            frame_stats: std::env::var_os("EMU198X_FRAME_STATS").map(|_| Default::default()),
            fatal_error: None,
            app_menu,
            menu_installed: false,
            host_cursor_hidden: false,
            command_tx,
        }
    }

    /// The core, and the window lent to it as its shell.
    fn split(&mut self) -> (&mut HostCore<S>, WinitShell<'_>) {
        (
            &mut self.core,
            WinitShell {
                window: self.window.as_ref(),
                video: &mut self.video,
                menu: &self.app_menu,
            },
        )
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
        self.core.update_pixel_aspect();
        let (display_width, display_height) = self.core.display_size();
        let (width, height) = self.core.picture_size(self.core.scale);
        let attributes = icon::with_icon(
            WindowAttributes::default()
                .with_title(self.core.window_title())
                .with_inner_size(LogicalSize::new(width, height))
                .with_min_inner_size(LogicalSize::new(display_width, display_height)),
        );
        let window = Arc::new(event_loop.create_window(attributes)?);
        icon::show_in_dock();
        let (fb_width, fb_height) = self.core.system.framebuffer_size(&self.core.runner.runtime);
        let video = WgpuVideoPresenter::new(window.clone(), fb_width, fb_height)?;
        self.window = Some(window);
        self.video = Some(video);
        self.core.restart_pacing();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    /// The window's inner size, for mapping the cursor onto the framebuffer.
    fn window_size(&self) -> Option<(u32, u32)> {
        self.window.as_ref().map(|window| {
            let size = window.inner_size();
            (size.width, size.height)
        })
    }

    fn render(&mut self) -> Result<(), UiError> {
        let core = &mut self.core;
        // A halted machine draws the diagnostic overlay over the frozen frame
        // instead of presenting it — so the cause is on-screen, not a mystery.
        if let Some(message) = &core.halt_message {
            let (width, height) = core.system.framebuffer_size(&core.runner.runtime);
            let pixels = overlay::build_halt_overlay(width, height, message);
            let frame = CapturedFrame {
                signal: None,
                timestamp: MachineTime::new(0),
                format: PixelFormat::Rgba8888,
                width,
                height,
                palette: None,
                pixels,
            };
            if let Some(video) = self.video.as_mut() {
                let mut profile = core.presentation;
                profile.filter = VideoFilter::Raw;
                video.present(&frame, &profile)?;
            }
            return Ok(());
        }

        let (Some(frame), Some(video)) = (core.runner.frame(), self.video.as_mut()) else {
            return Ok(());
        };
        // Debug stepping and a switch to a variant without an electrical
        // source must still display their raw state, rather than end the UI.
        if frame.signal.is_none() && core.presentation.filter.uses_signal() {
            let fallback = if core.presentation.filter.is_modern() {
                VideoFilter::Raw
            } else {
                VideoFilter::Crt
            };
            core.presentation.filter = fallback;
            self.app_menu.set_current_filter(fallback);
            eprintln!("electrical video is unavailable for this frame; using {fallback} display");
        }
        let clock = core.runner.runtime.profile().clock.rate;
        core.presentation.clock_hz = clock.numerator_hz as f64 / clock.denominator_hz as f64;
        let started = Instant::now();
        let presented = video.present_with_status(frame, &core.presentation)?;
        if let Some(stats) = self.frame_stats.as_mut() {
            if !presented {
                stats.suspend();
                return Ok(());
            }
            stats.observe(
                frame.timestamp,
                core.presentation.clock_hz,
                core.presentation.filter,
                started.elapsed(),
                core.system.frame_duration(&core.runner.runtime),
            );
        }
        Ok(())
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        if let Some(video) = self.video.as_mut() {
            video.resize_surface(width, height);
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
        // exists, and so does the window Windows attaches it to). Idempotent
        // via the flag, since `resumed` can fire again.
        if !self.menu_installed
            && let Some(window) = self.window.clone()
        {
            self.app_menu.install(&window);
            self.menu_installed = true;
            // A Windows menu bar takes its height from the client area, which
            // would shrink the picture below its whole-number scale. Ask for
            // the scaled size again now the bar is there.
            #[cfg(target_os = "windows")]
            {
                let (core, mut shell) = self.split();
                let scale = core.scale;
                core.set_window_scale(&mut shell, scale);
            }
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
        self.handle_window_event(event_loop, event);
        self.sync_host_cursor();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Translate native-menu clicks into commands on the core's channel.
        // muda's receiver is a global crossbeam channel; non-Linux only.
        #[cfg(not(target_os = "linux"))]
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some(command) = self.app_menu.command_for(&event.id) {
                let _ = self.command_tx.send(command);
            }
        }
        let (core, mut shell) = self.split();
        match core.tick(&mut shell) {
            Ok(Wake::Now) => event_loop.set_control_flow(ControlFlow::Poll),
            Ok(Wake::At(at)) => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            Err(err) => self.fail(event_loop, err),
        }
        self.sync_host_cursor();
    }
}

impl<S: UiSystem> App<S> {
    /// Show or hide the host cursor as the core asks, touching the window
    /// only on a change.
    fn sync_host_cursor(&mut self) {
        let hidden = !self.core.host_cursor_visible();
        if hidden == self.host_cursor_hidden {
            return;
        }
        if let Some(window) = &self.window {
            window.set_cursor_visible(!hidden);
        }
        self.host_cursor_hidden = hidden;
    }

    fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.core.modifiers_changed(modifiers.state());
            }
            WindowEvent::Focused(false) => self.core.focus_lost(),
            WindowEvent::CursorLeft { .. } => self.core.cursor_left(),
            WindowEvent::CursorMoved { position, .. } => {
                let size = self.window_size();
                self.core.cursor_moved(position.x, position.y, size);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.core
                    .mouse_button(button, state == ElementState::Pressed);
            }
            WindowEvent::Resized(size) => self.resize_surface(size.width, size.height),
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some((width, height)) = self.window_size() {
                    self.resize_surface(width, height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                let (core, mut shell) = self.split();
                core.key_event(&mut shell, code, &event.logical_key, pressed, event.repeat);
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.render() {
                    self.fail(event_loop, err);
                }
            }
            _ => {}
        }
    }
}

/// Open a native window for `system` driving `runtime`, and run the winit
/// event loop until the window closes or a fatal error surfaces.
///
/// The runtime should already be initialised (media loaded); the harness runs
/// one frame up front so the first redraw has a picture.
///
/// Sound plays through the host's default output device when `audio` is
/// set. If that device cannot be opened the window warns once and runs
/// silent; it never fails to start for want of sound.
///
/// # Errors
///
/// Returns [`UiError`] for an invalid scale, video init failure, a machine
/// error, or an event-loop error.
pub fn run<S: UiSystem + 'static>(
    system: S,
    runtime: S::Runtime,
    scale: u32,
    video: VideoFilter,
    audio: bool,
) -> Result<(), UiError> {
    run_with_phosphor(
        system,
        runtime,
        scale,
        video,
        emu198x_native_video::DEFAULT_PHOSPHOR_TAU_MS,
        audio,
    )
}

/// Open a window with a chosen CRT phosphor 1/e decay time in milliseconds.
/// Zero disables persistence; other video modes bypass it.
///
/// # Errors
/// Returns [`UiError`] for invalid scale/decay, machine or host failures.
pub fn run_with_phosphor<S: UiSystem + 'static>(
    system: S,
    runtime: S::Runtime,
    scale: u32,
    video: VideoFilter,
    phosphor_ms: f32,
    audio: bool,
) -> Result<(), UiError> {
    if scale == 0 {
        return Err(UiError::InvalidScale { value: scale });
    }
    if !phosphor_ms.is_finite() || phosphor_ms < 0.0 {
        return Err(UiError::InvalidPhosphorDecay { value: phosphor_ms });
    }
    // Harness-global controls every system shares, printed once so each runner's
    // own per-machine controls line doesn't have to repeat them.
    println!("Save-state: Cmd/Ctrl+S quick-save, Cmd/Ctrl+L quick-load (one slot per machine).");
    let mut runner = Runner::new(runtime, audio);
    let frame_ticks = system.frame_ticks(&runner.runtime);
    runner.run_ticks(&[], frame_ticks)?;

    let mut core = HostCore::new(system, runner, scale, video);
    core.presentation.phosphor_tau_ms = phosphor_ms;
    let (mut core, error) = run_host(core, video)?;

    // Teardown (e.g. flush battery RAM) runs whether the session ended cleanly
    // or via a fatal error, so a crash still persists state. A fatal emulation
    // error takes precedence over a teardown failure (the latter is then just
    // logged), so the real cause isn't masked by a failed battery write.
    let teardown = core.on_exit();
    if let Some(err) = error {
        if let Err(reason) = teardown {
            eprintln!("error: teardown failed: {reason}");
        }
        return Err(err);
    }
    teardown.map_err(UiError::Teardown)?;
    Ok(())
}

/// Run `core` in this platform's window until it closes, returning it for
/// teardown along with the error that ended the session, if any.
fn run_host<S: UiSystem + 'static>(
    core: HostCore<S>,
    video: VideoFilter,
) -> Result<(HostCore<S>, Option<UiError>), UiError> {
    #[cfg(all(target_os = "linux", feature = "gtk"))]
    if gtk_host::chosen() {
        return gtk_host::run(core, video);
    }
    let mut app = App::new(core, video);
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;
    let error = app.take_error();
    Ok((app.core, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_core::{
        StateAction, TapeShortcut, media_filter, slot_file_name, state_chord_action,
        tape_export_chord, tape_transport_shortcut,
    };
    use emu198x_shell::MediaKind;
    use winit::keyboard::ModifiersState;

    #[test]
    fn tape_export_requires_an_explicit_host_chord() {
        assert!(tape_export_chord(
            ModifiersState::SUPER | ModifiersState::SHIFT,
            KeyCode::KeyE
        ));
        assert!(tape_export_chord(
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            KeyCode::KeyE
        ));
        assert!(!tape_export_chord(ModifiersState::empty(), KeyCode::KeyE));
        assert!(!tape_export_chord(ModifiersState::CONTROL, KeyCode::KeyE));
        assert!(!tape_export_chord(
            ModifiersState::ALT | ModifiersState::SHIFT,
            KeyCode::KeyE
        ));
        assert!(!tape_export_chord(
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            KeyCode::KeyS
        ));
    }

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

    /// The winit key for a catalogue shortcut key.
    fn key_code(key: emu198x_shell::commands::ShortcutKey) -> KeyCode {
        use emu198x_shell::commands::ShortcutKey;
        match key {
            ShortcutKey::Function(9) => KeyCode::F9,
            ShortcutKey::Function(10) => KeyCode::F10,
            ShortcutKey::Function(11) => KeyCode::F11,
            ShortcutKey::Function(12) => KeyCode::F12,
            ShortcutKey::Letter('E') => KeyCode::KeyE,
            ShortcutKey::Letter('K') => KeyCode::KeyK,
            ShortcutKey::Letter('L') => KeyCode::KeyL,
            ShortcutKey::Letter('S') => KeyCode::KeyS,
            other => panic!("no winit key for {other:?}; add it here"),
        }
    }

    #[test]
    fn the_shortcut_handlers_follow_the_catalogue() {
        use emu198x_shell::commands::expect_command;
        let shortcut = |id: &str| {
            let spec = expect_command(id);
            let shortcut = spec
                .shortcut
                .unwrap_or_else(|| panic!("`{id}` has no shortcut"));
            let mut mods = ModifiersState::empty();
            if shortcut.host {
                mods |= ModifiersState::CONTROL;
            }
            if shortcut.shift {
                mods |= ModifiersState::SHIFT;
            }
            (mods, key_code(shortcut.key))
        };

        for (id, expected) in [
            ("media.tape.play", TapeShortcut::Play),
            ("media.tape.stop", TapeShortcut::Stop),
            ("media.tape.fast-forward", TapeShortcut::ToggleTurbo),
        ] {
            let (mods, code) = shortcut(id);
            assert!(mods.is_empty(), "`{id}` is a bare function key");
            assert_eq!(tape_transport_shortcut(code), Some(expected), "{id}");
        }
        for (id, expected) in [
            ("state.quick-save", StateAction::Save),
            ("state.quick-load", StateAction::Load),
        ] {
            let (mods, code) = shortcut(id);
            assert_eq!(state_chord_action(mods, code, true), Some(expected), "{id}");
        }
        let (mods, code) = shortcut("media.tape.export");
        assert!(tape_export_chord(mods, code));
        // Reset and the keyboard-mode chord are matched inline in
        // `handle_shortcut`; pin their catalogue entries to what it checks.
        assert_eq!(
            shortcut("machine.reset"),
            (ModifiersState::empty(), KeyCode::F12)
        );
        assert_eq!(
            shortcut("machine.keyboard.host"),
            (
                ModifiersState::CONTROL | ModifiersState::SHIFT,
                KeyCode::KeyK
            )
        );
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
