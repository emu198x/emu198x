//! `SpectrumApp` — the winit `ApplicationHandler` that drives the
//! interactive UI mode. Owns the window, the wgpu video presenter, the
//! frame-pacing loop, the keyboard plumbing, and the muda command
//! channel; delegates machine state to `SpectrumRunner`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use emu198x_native_video::{PresentationProfile, VideoFilter, WgpuVideoPresenter};
use emu198x_shell::{
    ControlCommand, FirmwareImage, FirmwareSet, InputEvent, MediaTransportAction,
    MediaTransportCommand,
};
use muda::MenuEvent;
use runtime_sinclair_zx_spectrum::SpeakerChannel;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::AppError;
use crate::live_machine::build_runtime;
use crate::machine::{MachineKind, read_variant_firmware};
use crate::ui::input::{map_spectrum_keys, spectrum_key_event};
use crate::ui::menu::{AppCommand, AppMenu};
use crate::ui::runner::{DEFAULT_TAPE_SLOT, SpectrumRunner};

/// Sub-divisions per emulator frame for input quantisation. The
/// runtime's `run_until` actually advances in whole-frame increments
/// (`machine.run_frame()` runs the full 279552 half-cycles regardless
/// of the `target` we pass), so a slice smaller than a frame still
/// runs a full frame's worth of emulation. Setting this to 1 aligns
/// the binary's pacing with the runtime's true granularity. Inputs
/// land at frame boundaries (~20 ms latency), which matches real
/// hardware: the keyboard matrix is scanned once per frame anyway.
pub(crate) const INPUT_SLICES_PER_FRAME: u32 = 1;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_TURBO_TAPE_FRAMES: u32 = 32;

pub struct SpectrumApp {
    runner: SpectrumRunner,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    next_slice_at: Instant,
    turbo_tape: bool,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, Vec<&'static str>>,
    window: Option<Arc<Window>>,
    video: Option<WgpuVideoPresenter>,
    presentation: PresentationProfile,
    fatal_error: Option<AppError>,
    menu: AppMenu,
    menu_installed: bool,
    current_machine: MachineKind,
    command_tx: Sender<AppCommand>,
    command_rx: Receiver<AppCommand>,
    fps_window_start: Instant,
    fps_window_frames: u32,
}

impl SpectrumApp {
    pub fn new(
        runner: SpectrumRunner,
        scale: u32,
        turbo_tape: bool,
        video: VideoFilter,
    ) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        let slice_ticks = subframe_ticks(runner.native_frame_ticks);
        let slice_duration = subframe_duration(runner.frame_duration());
        let current_machine = MachineKind::Spectrum48K;
        let menu = AppMenu::new(current_machine, runner.supports_disk_slot(), scale, video);
        let (command_tx, command_rx) = mpsc::channel();
        Ok(Self {
            runner,
            scale,
            slice_ticks,
            slice_duration,
            next_slice_at: Instant::now(),
            turbo_tape,
            pending_inputs: Vec::new(),
            pressed_keys: HashMap::new(),
            window: None,
            video: None,
            presentation: PresentationProfile::for_filter(video),
            fatal_error: None,
            menu,
            menu_installed: false,
            current_machine,
            command_tx,
            command_rx,
            fps_window_start: Instant::now(),
            fps_window_frames: 0,
        })
    }

    pub fn take_error(&mut self) -> Option<AppError> {
        self.fatal_error.take()
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: AppError) {
        eprintln!("error: {err}");
        self.fatal_error = Some(err);
        event_loop.exit();
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppError> {
        if self.window.is_some() {
            return Ok(());
        }

        let logical_width = f64::from((SCREEN_WIDTH as u32).saturating_mul(self.scale));
        let logical_height = f64::from((SCREEN_HEIGHT as u32).saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(self.window_title())
            .with_inner_size(LogicalSize::new(logical_width, logical_height))
            .with_min_inner_size(LogicalSize::new(
                f64::from(SCREEN_WIDTH as u32),
                f64::from(SCREEN_HEIGHT as u32),
            ));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let video =
            WgpuVideoPresenter::new(window.clone(), SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)?;

        self.window = Some(window);
        self.video = Some(video);
        self.next_slice_at = Instant::now();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn turbo_tape_active(&self) -> bool {
        self.turbo_tape && self.runner.tape_playing()
    }

    fn window_title(&self) -> String {
        let mut title = self.runner.window_title();
        if self.turbo_tape {
            if self.runner.tape_playing() {
                title.push_str(" | turbo");
            } else {
                title.push_str(" | turbo armed");
            }
        }
        title
    }

    fn set_turbo_tape(&mut self, enabled: bool) {
        self.turbo_tape = enabled;
        self.next_slice_at = Instant::now() + self.slice_duration;
    }

    /// Returns the number of emulator frames completed during this
    /// call. The caller can sum the returns over a wall-clock window
    /// to compute actual emu frames per second — important because
    /// the catch-up loop may complete multiple frames in one pass and
    /// a bool return would silently coalesce them.
    fn advance_machine(&mut self) -> Result<u32, AppError> {
        if self.turbo_tape_active() {
            let mut ran_frames = 0u32;
            while ran_frames < MAX_TURBO_TAPE_FRAMES && self.turbo_tape_active() {
                let inputs = std::mem::take(&mut self.pending_inputs);
                self.runner.run_frame(&inputs)?;
                ran_frames += 1;
            }
            self.next_slice_at = Instant::now() + self.slice_duration;
            return Ok(ran_frames);
        }

        let now = Instant::now();
        if now < self.next_slice_at {
            return Ok(0);
        }

        let mut ran_slices = 0;
        let max_catch_up_slices = MAX_CATCH_UP_FRAMES.saturating_mul(INPUT_SLICES_PER_FRAME);
        let mut frames_completed = 0u32;
        while Instant::now() >= self.next_slice_at && ran_slices < max_catch_up_slices {
            let inputs = std::mem::take(&mut self.pending_inputs);
            if self.runner.run_ticks(&inputs, self.slice_ticks)? {
                frames_completed += 1;
            }
            self.next_slice_at += self.slice_duration;
            ran_slices += 1;
        }

        if ran_slices == max_catch_up_slices && Instant::now() >= self.next_slice_at {
            self.next_slice_at = Instant::now() + self.slice_duration;
        }

        Ok(frames_completed)
    }

    fn render(&mut self) -> Result<(), AppError> {
        let Some(frame) = self.runner.frame() else {
            return Ok(());
        };
        let Some(video) = self.video.as_mut() else {
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
        let Some(names) = map_spectrum_keys(code) else {
            return;
        };

        if pressed {
            if self.pressed_keys.contains_key(&code) {
                return;
            }
            self.pressed_keys.insert(code, names.to_vec());
            self.pending_inputs.extend(
                names
                    .iter()
                    .copied()
                    .map(|name| spectrum_key_event(name, true)),
            );
            self.next_slice_at = Instant::now();
        } else if let Some(names) = self.pressed_keys.remove(&code) {
            self.pending_inputs.extend(
                names
                    .into_iter()
                    .map(|name| spectrum_key_event(name, false)),
            );
            self.next_slice_at = Instant::now();
        }
    }

    fn release_all_keys(&mut self) {
        let keys = std::mem::take(&mut self.pressed_keys);
        if keys.is_empty() {
            return;
        }
        for names in keys.into_values() {
            self.pending_inputs.extend(
                names
                    .into_iter()
                    .map(|name| spectrum_key_event(name, false)),
            );
        }
        self.next_slice_at = Instant::now();
    }

    fn handle_shortcut(
        &mut self,
        event_loop: &ActiveEventLoop,
        code: KeyCode,
        pressed: bool,
    ) -> bool {
        if !pressed {
            return matches!(
                code,
                KeyCode::Escape
                    | KeyCode::F9
                    | KeyCode::F10
                    | KeyCode::F11
                    | KeyCode::F12
                    | KeyCode::Numpad0
                    | KeyCode::Numpad1
                    | KeyCode::Numpad2
            );
        }

        let result =
            match code {
                KeyCode::Escape => {
                    event_loop.exit();
                    return true;
                }
                KeyCode::F9 => self.runner.command(&ControlCommand::MediaTransport(
                    MediaTransportCommand::new(DEFAULT_TAPE_SLOT, MediaTransportAction::Start),
                )),
                KeyCode::F10 => self.runner.command(&ControlCommand::MediaTransport(
                    MediaTransportCommand::new(DEFAULT_TAPE_SLOT, MediaTransportAction::Stop),
                )),
                KeyCode::F11 => {
                    self.set_turbo_tape(!self.turbo_tape);
                    if let Some(window) = &self.window {
                        window.set_title(&self.window_title());
                        window.request_redraw();
                    }
                    return true;
                }
                KeyCode::F12 => {
                    self.release_all_keys();
                    self.runner.reset()
                }
                KeyCode::Numpad0 => {
                    self.runner.reset_audio_controls();
                    eprintln!("audio: reset speaker controls");
                    return true;
                }
                KeyCode::Numpad1 => {
                    self.toggle_audio_channel_shortcut(SpeakerChannel::Speaker);
                    return true;
                }
                KeyCode::Numpad2 => {
                    self.cycle_audio_channel_gain_shortcut(SpeakerChannel::Speaker);
                    return true;
                }
                _ => return false,
            };

        if let Err(err) = result {
            self.fail(event_loop, err);
        }
        true
    }

    fn toggle_audio_channel_shortcut(&mut self, channel: SpeakerChannel) {
        let enabled = self.runner.toggle_audio_channel(channel);
        eprintln!(
            "audio: {} {}",
            channel.label(),
            if enabled { "enabled" } else { "muted" }
        );
    }

    fn cycle_audio_channel_gain_shortcut(&mut self, channel: SpeakerChannel) {
        let gain = self.runner.cycle_audio_channel_gain(channel);
        eprintln!("audio: {} gain {:.0}%", channel.label(), gain * 100.0);
    }

    /// Processes one queued [`AppCommand`] at a frame boundary. Each
    /// command runs *between* frames so a switch never tears down state
    /// the runtime is currently using; same channel will carry rfd
    /// dialog replies and MCP commands in future cuts. See
    /// `wiki/decisions/native-menu-shell.md`.
    fn handle_command(&mut self, cmd: AppCommand) {
        match cmd {
            AppCommand::SwitchMachine(kind) => self.switch_machine(kind),
            AppCommand::OpenSnapshot => self.open_snapshot(),
            AppCommand::LoadSnapshot => self.load_state(),
            AppCommand::OpenTape => self.open_tape(),
            AppCommand::OpenDisk => self.open_disk(),
            AppCommand::SaveSnapshot => self.save_snapshot(),
            AppCommand::SetWindowScale(scale) => self.set_window_scale(scale),
            AppCommand::SetVideoFilter(filter) => self.set_video_filter(filter),
            AppCommand::OpenUrl(url) => Self::open_url(url),
        }
    }

    /// Resizes the window to `scale × native frame` and refreshes the
    /// View menu radio so only the new scale is checked. winit's
    /// `request_inner_size` tells the OS the new size; the resize
    /// event then drives the wgpu surface reconfigure.
    fn set_window_scale(&mut self, scale: u32) {
        if scale == 0 {
            eprintln!("view: rejecting zero scale");
            return;
        }
        self.scale = scale;
        if let Some(window) = &self.window {
            let logical_width = f64::from((SCREEN_WIDTH as u32).saturating_mul(scale));
            let logical_height = f64::from((SCREEN_HEIGHT as u32).saturating_mul(scale));
            let _ = window.request_inner_size(LogicalSize::new(logical_width, logical_height));
            window.request_redraw();
        }
        self.menu.set_current_scale(scale);
        eprintln!("view: window scale → {scale}×");
    }

    /// Switches the post-framebuffer video filter and refreshes the
    /// View menu radio.
    fn set_video_filter(&mut self, filter: VideoFilter) {
        self.presentation = PresentationProfile::for_filter(filter);
        self.menu.set_current_filter(filter);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        eprintln!("view: video filter → {filter:?}");
    }

    /// Pops a snapshot file picker and restores the selection if the
    /// user picked one. Errors are logged and the running session
    /// continues — the user sees a dialog dismiss without any other
    /// side effect.
    fn open_snapshot(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open Snapshot")
            .add_filter("Spectrum snapshots", &["sna", "z80", "zip"])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        if let Err(err) = self.runner.import_portable_snapshot_from_path(&path) {
            eprintln!("file: failed to load snapshot {}: {err}", path.display());
            return;
        }
        if let Some(window) = &self.window {
            window.set_title(&self.window_title());
            window.request_redraw();
        }
        eprintln!("file: loaded snapshot {}", path.display());
    }

    /// Pops a tape file picker and (on selection) loads the tape into
    /// slot tape-1 and starts transport so the program begins loading.
    /// F10 stops the tape if the user wants to inspect editor state
    /// instead.
    fn open_tape(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open Tape")
            .add_filter("Spectrum tapes", &["tap", "tzx", "zip"])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        if let Err(err) = self.runner.load_tape_from_path(&path, true) {
            eprintln!("file: failed to load tape {}: {err}", path.display());
            return;
        }
        if let Some(window) = &self.window {
            window.set_title(&self.window_title());
            window.request_redraw();
        }
        eprintln!("file: loaded tape {}", path.display());
    }

    /// Pops a disk file picker and inserts the selection into the +3's
    /// drive. Only reachable when the live variant supports disks; the
    /// menu item is disabled otherwise.
    fn open_disk(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open Disk")
            .add_filter("Amstrad / +3 disks", &["dsk", "edsk", "zip"])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        if let Err(err) = self.runner.load_disk_from_path(&path) {
            eprintln!("file: failed to load disk {}: {err}", path.display());
            return;
        }
        eprintln!("file: loaded disk {}", path.display());
    }

    /// Pops a save dialog and writes the current emulator state to the
    /// selected location. Uses the runtime's postcard save format with
    /// the `.emu198x-state` extension — an emu198x-internal save state,
    /// not a portable `.sna` / `.z80` snapshot. Save the latter via
    /// future export tooling; for now this menu item supports the
    /// quick-save / quick-load workflow the State menu owns.
    fn save_snapshot(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save State")
            .add_filter("emu198x quick state", &["emu198x-state"])
            .set_file_name("state.emu198x-state")
            .save_file()
        else {
            return;
        };
        if let Err(err) = self.runner.save_snapshot_to_path(&path) {
            eprintln!("state: failed to save state {}: {err}", path.display());
            return;
        }
        eprintln!("state: saved state {}", path.display());
    }

    /// Pops an open dialog accepting any of the three snapshot formats
    /// (postcard `.emu198x-state`, portable `.sna` / `.z80`) and
    /// dispatches via the auto-detect runner helper.
    fn load_state(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Load State")
            .add_filter("Snapshots", &["emu198x-state", "sna", "z80", "zip"])
            .add_filter("emu198x quick state", &["emu198x-state"])
            .add_filter("Spectrum snapshots", &["sna", "z80", "zip"])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        if let Err(err) = self.runner.load_any_snapshot_from_path(&path) {
            eprintln!("state: failed to load state {}: {err}", path.display());
            return;
        }
        if let Some(window) = &self.window {
            window.set_title(&self.window_title());
            window.request_redraw();
        }
        eprintln!("state: loaded state {}", path.display());
    }

    /// Launches `url` in the system browser. macOS-first via the
    /// `open` command; other platforms get a stub error.
    fn open_url(url: &'static str) {
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = std::process::Command::new("open").arg(url).status() {
                eprintln!("help: failed to open {url}: {err}");
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            eprintln!("help: opening URLs is macOS-only for now ({url})");
        }
    }

    /// Replaces the running runtime with a fresh one for `kind`. Loads
    /// the variant's ROM bundle, builds the boxed runtime via
    /// [`build_runtime`], and updates host-side state (pacing
    /// constants, audio buffer, window title, menu indicator).
    /// On firmware-missing or build failure, logs the error and keeps
    /// the menu indicator pinned to the actually-running machine so
    /// the checkmark never lies about what's loaded.
    fn switch_machine(&mut self, kind: MachineKind) {
        if kind == self.current_machine {
            return;
        }

        let images = match read_variant_firmware(kind) {
            Ok(images) => images,
            Err(err) => {
                eprintln!("menu: cannot switch to {}: {err}", kind.label());
                self.menu.set_current_machine(self.current_machine);
                return;
            }
        };
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in &images {
            firmware.push(FirmwareImage::new(*id, bytes));
        }
        let runtime = match build_runtime(kind, &firmware) {
            Ok(runtime) => runtime,
            Err(err) => {
                eprintln!("menu: cannot switch to {}: {err}", kind.label());
                self.menu.set_current_machine(self.current_machine);
                return;
            }
        };

        // Drop the old box, install the new one, refresh per-variant
        // pacing. Turbo-tape resets too — different variant, no carry-
        // over from the previous tape session.
        self.runner.replace_runtime(runtime);
        self.slice_ticks = subframe_ticks(self.runner.native_frame_ticks);
        self.slice_duration = subframe_duration(self.runner.frame_duration());
        self.next_slice_at = Instant::now();
        self.turbo_tape = false;
        self.pending_inputs.clear();
        self.pressed_keys.clear();
        self.current_machine = kind;
        self.menu.set_current_machine(kind);
        self.menu.set_disk_supported(self.runner.supports_disk_slot());
        if let Some(window) = &self.window {
            window.set_title(&self.window_title());
            window.request_redraw();
        }
        eprintln!("menu: switched to {}", kind.label());
    }
}

impl ApplicationHandler for SpectrumApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.create_window(event_loop) {
            self.fail(event_loop, err);
            return;
        }
        if !self.menu_installed {
            // muda's macOS menu attaches to NSApp once the application is
            // up. Other platforms use init_for_hwnd / init_for_gtk_window —
            // see crates/emu198x-spectrum/src/ui/menu.rs.
            self.menu.install_for_nsapp();
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
            WindowEvent::Focused(false) => self.release_all_keys(),
            WindowEvent::Resized(size) => {
                self.resize_surface(size.width, size.height);
            }
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
        // Drain menu events into the command channel, then drain the
        // channel and process every queued command. Both happen at the
        // frame boundary so a command never tears down state mid-frame.
        // Same path will carry rfd dialog replies and MCP commands in
        // future cuts; see wiki/decisions/native-menu-shell.md.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(cmd) = self.menu.action_map.get(&event.id) {
                let _ = self.command_tx.send(cmd.clone());
            }
        }
        while let Ok(cmd) = self.command_rx.try_recv() {
            self.handle_command(cmd);
        }

        match self.advance_machine() {
            Ok(frames_completed) => {
                if frames_completed > 0 {
                    self.fps_window_frames += frames_completed;
                    if std::env::var_os("EMU198X_FPS").is_some() {
                        let elapsed = self.fps_window_start.elapsed();
                        if elapsed >= Duration::from_secs(1) {
                            let fps =
                                f64::from(self.fps_window_frames) / elapsed.as_secs_f64();
                            eprintln!("emu fps: {fps:.1}");
                            self.fps_window_start = Instant::now();
                            self.fps_window_frames = 0;
                        }
                    }
                    if let Some(window) = &self.window {
                        window.set_title(&self.window_title());
                        window.request_redraw();
                    }
                }
            }
            Err(err) => {
                self.fail(event_loop, err);
                return;
            }
        }

        if self.turbo_tape_active() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_slice_at));
        }
    }
}

pub(crate) fn subframe_ticks(frame_ticks: u64) -> u64 {
    frame_ticks.div_ceil(u64::from(INPUT_SLICES_PER_FRAME))
}

pub(crate) fn subframe_duration(frame_duration: Duration) -> Duration {
    Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(INPUT_SLICES_PER_FRAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::timing::TIMING_48K;

    #[test]
    fn subframe_helpers_preserve_timing_budget() {
        // 48K reference values: 14 MHz master, 280032 hc/frame. The
        // pacing helpers are variant-agnostic (they take a frame
        // length and divide), so 48K is just one representative case.
        let frame_ticks = u64::from(TIMING_48K.halfcycles_per_frame);
        let frame_duration = Duration::from_secs_f64(
            frame_ticks as f64 / TIMING_48K.master_hz as f64,
        );
        let slice_ticks = subframe_ticks(frame_ticks);
        let slice_duration = subframe_duration(frame_duration);

        assert!(slice_ticks <= frame_ticks);
        assert!(slice_ticks * u64::from(INPUT_SLICES_PER_FRAME) >= frame_ticks);
        assert!(slice_duration <= frame_duration);
    }
}
