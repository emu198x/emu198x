//! `emu198x-spectrum` — minimal native Spectrum verification shell.
//!
//! This is intentionally narrow: one 48K window, optional ROM/tape loading,
//! direct keyboard input, and basic media transport control for interactive
//! verification. It sits above the existing runtime and shared shell boundary;
//! it does not introduce a parallel emulation stack.

mod live_machine;
mod menu;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use muda::MenuEvent;

use crate::live_machine::{LiveSpectrumRuntime, build_runtime};
use crate::menu::{AppCommand, AppMenu, MachineKind};

use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K};
use emu198x_native_video::{
    PresentationProfile, VideoFilter, VideoPresenterError, WgpuVideoPresenter,
};
use emu198x_shell::query::query_value;
use emu198x_shell::{
    AssetLoadError, CapturedFrame, ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession,
    HostIo, InputEvent, LatestFrameCapture, MachineError, MediaImage, MediaKind, MediaSet,
    MediaTransportAction, MediaTransportCommand, NativeAudioError, NativeAudioOutput,
    NullTraceSink, QueryError, QueryResult, ResetKind, RunResult, read_firmware_asset,
    read_media_asset,
};
use runtime_sinclair_zx_spectrum::{
    AudioControls, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, SpeakerChannel,
    Spectrum48kRuntime, SpectrumSessionQueryProvider, autoload_basic_tape,
};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const DEFAULT_ROM_ID: &str = "sinclair-zx-spectrum-48k-rom";
const DEFAULT_TAPE_SLOT: &str = "tape-1";
const DEFAULT_SCALE: u32 = 2;
/// Sub-divisions per emulator frame for input quantisation. The
/// runtime's `run_until` actually advances in whole-frame increments
/// (`machine.run_frame()` runs the full 279552 half-cycles regardless
/// of the `target` we pass), so a slice smaller than a frame still
/// runs a full frame's worth of emulation. Setting this to 1 aligns
/// the binary's pacing with the runtime's true granularity. Inputs
/// land at frame boundaries (~20 ms latency), which matches real
/// hardware: the keyboard matrix is scanned once per frame anyway.
const INPUT_SLICES_PER_FRAME: u32 = 1;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_TURBO_TAPE_FRAMES: u32 = 32;
const MAX_AUDIO_BUFFER_MS: u32 = 250;

const USAGE: &str = "\
Usage: emu198x-spectrum [OPTIONS]

Options:
    --rom PATH         48K ROM image or zip containing one ROM candidate
    --tape PATH        TAP/TZX image or zip containing one tape candidate
    --play-tape        start tape transport immediately after media load
    --autoload-tape    wait for boot, type LOAD \"\", and start tape-1
    --turbo-tape       run unthrottled while the tape is playing
    --scale N          integer window scale, default 2
    --video MODE       raw | lcd | crt [default: raw]
    --help, -h         show this help

Controls:
    Esc                quit
    F9                 start tape
    F10                stop tape
    F11                toggle tape turbo
    F12                hard reset
    Numpad 1           toggle speaker output
    Numpad 2           cycle speaker gain
    Numpad 0           reset speaker controls
    Left/Down/Up/Right host aliases for Spectrum 5/6/7/8 game keys
    Alt                Symbol Shift

Examples:
    emu198x-spectrum
    emu198x-spectrum --rom 48.rom --tape manic_miner.zip
    emu198x-spectrum --tape manic_miner.zip --autoload-tape
    emu198x-spectrum --tape '/Users/stevehill/Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]/Manic Miner (1983)(Bug-Byte).zip'
";

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    rom: Option<PathBuf>,
    tape: Option<PathBuf>,
    play_tape: bool,
    autoload_tape: bool,
    turbo_tape: bool,
    scale: u32,
    video: VideoFilter,
}

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Asset(#[from] AssetLoadError),

    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error(transparent)]
    Query(#[from] QueryError),

    #[error(transparent)]
    Session(#[from] emu198x_shell::SessionError),

    #[error(transparent)]
    SpectrumAutoload(#[from] runtime_sinclair_zx_spectrum::SpectrumAutoloadError),

    #[error(transparent)]
    Audio(#[from] NativeAudioError),

    #[error(transparent)]
    Video(#[from] VideoPresenterError),

    #[error(transparent)]
    EventLoop(#[from] EventLoopError),

    #[error(transparent)]
    Os(#[from] OsError),

    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },

    #[error("no ROM supplied and default Spectrum ROM was not found at {path}")]
    MissingRom { path: String },

    #[error("tape transport requested without tape media")]
    MissingTape,

    #[error("--autoload-tape conflicts with --play-tape")]
    ConflictingTapeWorkflow,
}

struct SpectrumRunner {
    runtime: Box<dyn LiveSpectrumRuntime>,
    frame_capture: LatestFrameCapture,
    audio_output: NativeAudioOutput,
    last_run_result: Option<RunResult>,
    native_frame_ticks: u64,
}

impl SpectrumRunner {
    fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        if cli.play_tape && cli.autoload_tape {
            return Err(AppError::ConflictingTapeWorkflow);
        }

        let rom_path = resolve_rom_path(cli)?;
        let rom = read_firmware_asset(&rom_path)?.bytes;

        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(DEFAULT_ROM_ID, &rom));
        let runtime = Spectrum48kRuntime::from_firmware(&firmware)?;
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_48K.halfcycles_per_frame),
            SpectrumSessionQueryProvider,
        );

        if let Some(tape_path) = &cli.tape {
            let tape = read_media_asset(tape_path, MediaKind::Tape)?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                DEFAULT_TAPE_SLOT,
                MediaKind::Tape,
                &tape.bytes,
            ));
            session.load_media(&media)?;
        }

        if cli.autoload_tape {
            if cli.tape.is_none() {
                return Err(AppError::MissingTape);
            }
            autoload_basic_tape(
                &mut session,
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            )?;
        } else if cli.play_tape {
            if cli.tape.is_none() {
                return Err(AppError::MissingTape);
            }
            session.command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                DEFAULT_TAPE_SLOT,
                MediaTransportAction::Start,
            )))?;
        }

        let runtime: Box<dyn LiveSpectrumRuntime> = Box::new(session.into_machine());
        let native_frame_ticks = u64::from(runtime.frame_halfcycles());
        let audio_output = NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?;
        let mut runner = Self {
            runtime,
            frame_capture: LatestFrameCapture::default(),
            audio_output,
            last_run_result: None,
            native_frame_ticks,
        };
        runner.run_frame(&[])?;
        Ok(runner)
    }

    /// Replaces the live runtime in-place. Drops the previous boxed
    /// runtime, swaps in the new one, recomputes the per-variant frame
    /// length, and clears host-side state (audio output, last frame,
    /// last run result) so frame timestamps and audio continuity start
    /// fresh. Pacing constants on the App must also be refreshed since
    /// `frame_halfcycles` and `frame_duration` differ across variants.
    fn replace_runtime(&mut self, runtime: Box<dyn LiveSpectrumRuntime>) {
        self.runtime = runtime;
        self.native_frame_ticks = u64::from(self.runtime.frame_halfcycles());
        self.frame_capture = LatestFrameCapture::default();
        self.audio_output.clear();
        self.last_run_result = None;
    }

    fn reset(&mut self) -> Result<(), AppError> {
        self.runtime.reset(ResetKind::Hard);
        self.last_run_result = None;
        self.frame_capture = LatestFrameCapture::default();
        self.audio_output.clear();
        self.run_frame(&[])?;
        Ok(())
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), AppError> {
        self.runtime.command(command)?;
        Ok(())
    }

    fn run_frame(&mut self, input_events: &[InputEvent]) -> Result<(), AppError> {
        let _ = self.run_ticks(input_events, self.native_frame_ticks)?;
        Ok(())
    }

    fn run_ticks(&mut self, input_events: &[InputEvent], ticks: u64) -> Result<bool, AppError> {
        let previous_frame_timestamp = self.frame().map(|frame| frame.timestamp);
        let target = self.runtime.time().saturating_add(ticks);
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events,
            frame_sink: &mut self.frame_capture,
            audio_sink: &mut self.audio_output,
            trace_sink: &mut trace_sink,
        };
        self.last_run_result = Some(self.runtime.run_until(target, &mut host)?);
        Ok(self.frame().map(|frame| frame.timestamp) != previous_frame_timestamp)
    }

    fn frame_duration(&self) -> Duration {
        self.runtime.frame_duration()
    }

    fn frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
    }

    fn toggle_audio_channel(&mut self, channel: SpeakerChannel) -> bool {
        let controls = self.runtime.audio_controls();
        let enabled = !controls.channel(channel).enabled();
        self.runtime.set_audio_channel_enabled(channel, enabled);
        enabled
    }

    fn cycle_audio_channel_gain(&mut self, channel: SpeakerChannel) -> f32 {
        let controls = self.runtime.audio_controls();
        let next = next_audio_gain(controls.channel(channel).gain());
        self.runtime.set_audio_channel_gain(channel, next);
        next
    }

    fn reset_audio_controls(&mut self) {
        self.runtime.set_audio_controls(AudioControls::default());
    }

    fn query(&self, path: &str) -> Result<QueryResult, AppError> {
        match query_value(
            self.runtime.profile(),
            self.runtime.time(),
            self.native_frame_ticks,
            self.frame().is_some(),
            false,
            self.last_run_result,
            path,
        ) {
            Ok(result) => Ok(result),
            Err(QueryError::UnknownPath { .. }) => self
                .runtime
                .query(path)?
                .ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })
                .map_err(AppError::from),
            Err(err) => Err(AppError::from(err)),
        }
    }

    fn query_bool(&self, path: &str) -> bool {
        self.query(path)
            .ok()
            .and_then(|result| result.value.as_bool())
            .unwrap_or(false)
    }

    fn window_title(&self) -> String {
        // Kept deliberately cheap so the per-frame title update doesn't
        // walk the screen-text grid. Tape state is two flag reads; the
        // boot-banner / row-23-prompt decoration that used to live here
        // ran a full 24×32 cell decode against 96 ROM glyphs twice per
        // frame and dominated the GUI's frame budget. Variant identity
        // comes from the live runtime's profile.display_name — updates
        // on Machine-menu switch.
        let tape = match (
            self.query_bool("spectrum.tape.loaded"),
            self.query_bool("spectrum.tape.playing"),
        ) {
            (true, true) => "tape playing",
            (true, false) => "tape loaded",
            (false, _) => "no tape",
        };
        let display_name = self.runtime.profile().display_name.as_ref();
        format!("Emu198x | {display_name} | {tape}")
    }

    fn tape_playing(&self) -> bool {
        self.query_bool("spectrum.tape.playing")
    }
}

struct SpectrumApp {
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
    fn new(
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
        let menu = AppMenu::new(current_machine);
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

    fn take_error(&mut self) -> Option<AppError> {
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
            // see crates/emu198x-spectrum/src/menu.rs.
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

fn main() {
    let cli = parse_cli(std::env::args().skip(1));
    if let Err(err) = run(cli) {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), AppError> {
    println!(
        "Controls: Esc quit, F9 start tape, F10 stop tape, F11 tape turbo, F12 reset, numpad 1 toggle speaker, numpad 2 cycle speaker gain, numpad 0 reset audio."
    );

    let runner = SpectrumRunner::from_cli(&cli)?;
    let mut app = SpectrumApp::new(runner, cli.scale, cli.turbo_tape, cli.video)?;
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    if let Some(err) = app.take_error() {
        return Err(err);
    }

    Ok(())
}

fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli {
        scale: DEFAULT_SCALE,
        ..Cli::default()
    };
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--play-tape" => cli.play_tape = true,
            "--autoload-tape" => cli.autoload_tape = true,
            "--turbo-tape" => cli.turbo_tape = true,
            "--scale" => {
                cli.scale = next_arg(&mut iter, "--scale")
                    .parse()
                    .unwrap_or_else(|_| die("--scale requires a positive integer"));
            }
            "--video" => {
                cli.video = parse_video_arg(&next_arg(&mut iter, "--video"));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ if arg.starts_with('-') => die(&format!("unknown flag: {arg}")),
            _ => {
                if cli.tape.is_none() {
                    cli.tape = Some(PathBuf::from(arg));
                } else {
                    die("only one positional tape path is supported");
                }
            }
        }
    }

    cli
}

fn parse_video_arg(video: &str) -> VideoFilter {
    video
        .parse()
        .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"))
}

fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
    iter.next()
        .unwrap_or_else(|| die(&format!("missing value for {flag}")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    process::exit(1);
}

fn resolve_rom_path(cli: &Cli) -> Result<PathBuf, AppError> {
    if let Some(path) = &cli.rom {
        return Ok(path.clone());
    }

    let default = default_rom_path();
    if default.is_file() {
        Ok(default)
    } else {
        Err(AppError::MissingRom {
            path: default.display().to_string(),
        })
    }
}

fn default_rom_path() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    Path::new(&home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
}

/// `~/.emu198x/roms` — the on-disk firmware bundle convention shared
/// with the `runtime-sinclair-zx-spectrum` golden tests. Returns `None`
/// only on hosts without a `HOME` environment variable, which never
/// happens on macOS / Linux but keeps unwrap-free behaviour explicit.
fn rom_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms"))
}

/// Per-variant ROM bundle: list of `(firmware_id, on-disk path)` pairs
/// that the runtime crate's `from_firmware` constructor expects for
/// the given variant.
///
/// Path conventions follow `runtime-sinclair-zx-spectrum/tests/goldens.rs`:
/// 16K/48K/+ share `sinclair-zx-spectrum-48k/48.rom`; 128K, +2, +2A/+3,
/// and +2B each have their own bundle directory. The +2A and +3 share
/// `amstrad-zx-spectrum-plus3/plus3-{0..3}.rom` (ROM v4.0); the +2B
/// uses its own `amstrad-zx-spectrum-plus2b/plus3-{0..3}.rom` (ROM v4.1).
fn variant_rom_bundle(kind: MachineKind, root: &Path) -> Vec<(&'static str, PathBuf)> {
    match kind {
        MachineKind::Spectrum16K
        | MachineKind::Spectrum48K
        | MachineKind::SpectrumPlus => vec![(
            "sinclair-zx-spectrum-48k-rom",
            root.join("sinclair-zx-spectrum-48k/48.rom"),
        )],
        MachineKind::Spectrum128K => vec![
            (
                "sinclair-zx-spectrum-128k-rom-0",
                root.join("sinclair-zx-spectrum-128k/128-0.rom"),
            ),
            (
                "sinclair-zx-spectrum-128k-rom-1",
                root.join("sinclair-zx-spectrum-128k/128-1.rom"),
            ),
        ],
        MachineKind::SpectrumPlus2 => vec![
            (
                "sinclair-zx-spectrum-plus2-rom-0",
                root.join("amstrad-zx-spectrum-plus2/plus2-0.rom"),
            ),
            (
                "sinclair-zx-spectrum-plus2-rom-1",
                root.join("amstrad-zx-spectrum-plus2/plus2-1.rom"),
            ),
        ],
        MachineKind::SpectrumPlus2A | MachineKind::SpectrumPlus3 => (0..4)
            .map(|i| {
                let id: &'static str = match i {
                    0 => "sinclair-zx-spectrum-plus3-rom-0",
                    1 => "sinclair-zx-spectrum-plus3-rom-1",
                    2 => "sinclair-zx-spectrum-plus3-rom-2",
                    _ => "sinclair-zx-spectrum-plus3-rom-3",
                };
                (
                    id,
                    root.join(format!("amstrad-zx-spectrum-plus3/plus3-{i}.rom")),
                )
            })
            .collect(),
        MachineKind::SpectrumPlus2B => (0..4)
            .map(|i| {
                let id: &'static str = match i {
                    0 => "sinclair-zx-spectrum-plus3-rom-0",
                    1 => "sinclair-zx-spectrum-plus3-rom-1",
                    2 => "sinclair-zx-spectrum-plus3-rom-2",
                    _ => "sinclair-zx-spectrum-plus3-rom-3",
                };
                (
                    id,
                    root.join(format!("amstrad-zx-spectrum-plus2b/plus3-{i}.rom")),
                )
            })
            .collect(),
    }
}

/// Reads every ROM the variant declares from disk into owned byte
/// vectors. Caller assembles a `FirmwareSet` by borrowing into the
/// returned vec — the bytes must outlive the set, so the caller holds
/// both.
///
/// # Errors
///
/// Returns [`AppError::MissingRom`] if `HOME` is unset or any required
/// ROM is absent on disk; surfaces filesystem errors via the `From<io::Error>`
/// arm.
fn read_variant_firmware(kind: MachineKind) -> Result<Vec<(&'static str, Vec<u8>)>, AppError> {
    let root = rom_root().ok_or_else(|| AppError::MissingRom {
        path: "$HOME unset; cannot locate ROM bundle".to_owned(),
    })?;
    let bundle = variant_rom_bundle(kind, &root);
    let mut images = Vec::with_capacity(bundle.len());
    for (id, path) in bundle {
        if !path.is_file() {
            return Err(AppError::MissingRom {
                path: path.display().to_string(),
            });
        }
        let bytes = std::fs::read(&path)?;
        images.push((id, bytes));
    }
    Ok(images)
}

fn subframe_ticks(frame_ticks: u64) -> u64 {
    frame_ticks.div_ceil(u64::from(INPUT_SLICES_PER_FRAME))
}

fn subframe_duration(frame_duration: Duration) -> Duration {
    Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(INPUT_SLICES_PER_FRAME))
}

fn next_audio_gain(gain: f32) -> f32 {
    if gain > 0.75 {
        0.5
    } else if gain > 0.375 {
        0.25
    } else if gain > 0.0 {
        0.0
    } else {
        1.0
    }
}

fn spectrum_key_event(name: &'static str, pressed: bool) -> InputEvent {
    InputEvent::Key {
        name: name.into(),
        pressed,
    }
}

fn map_spectrum_keys(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
        KeyCode::KeyA => &["a"],
        KeyCode::KeyB => &["b"],
        KeyCode::KeyC => &["c"],
        KeyCode::KeyD => &["d"],
        KeyCode::KeyE => &["e"],
        KeyCode::KeyF => &["f"],
        KeyCode::KeyG => &["g"],
        KeyCode::KeyH => &["h"],
        KeyCode::KeyI => &["i"],
        KeyCode::KeyJ => &["j"],
        KeyCode::KeyK => &["k"],
        KeyCode::KeyL => &["l"],
        KeyCode::KeyM => &["m"],
        KeyCode::KeyN => &["n"],
        KeyCode::KeyO => &["o"],
        KeyCode::KeyP => &["p"],
        KeyCode::KeyQ => &["q"],
        KeyCode::KeyR => &["r"],
        KeyCode::KeyS => &["s"],
        KeyCode::KeyT => &["t"],
        KeyCode::KeyU => &["u"],
        KeyCode::KeyV => &["v"],
        KeyCode::KeyW => &["w"],
        KeyCode::KeyX => &["x"],
        KeyCode::KeyY => &["y"],
        KeyCode::KeyZ => &["z"],
        KeyCode::Digit0 => &["0"],
        KeyCode::Digit1 => &["1"],
        KeyCode::Digit2 => &["2"],
        KeyCode::Digit3 => &["3"],
        KeyCode::Digit4 => &["4"],
        KeyCode::Digit5 => &["5"],
        KeyCode::Digit6 => &["6"],
        KeyCode::Digit7 => &["7"],
        KeyCode::Digit8 => &["8"],
        KeyCode::Digit9 => &["9"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Space => &["space"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["caps"],
        KeyCode::AltLeft | KeyCode::AltRight => &["symbol"],
        // The Spectrum+ (1984) and every later model — 128K, +2, +2A,
        // +2B, +3 — have full-stroke keyboards with labelled cursor
        // keys whose membrane wiring closes the exact same matrix
        // contacts as Caps Shift + 5/6/7/8. The ROM never sees a
        // dedicated "cursor" scancode; it sees Caps held and a number
        // key pressed. So this *is* the hardware mapping, not a
        // synthesis. Boot menus on the 128K-family rely on it; games
        // that read the matrix directly see exactly what the real
        // hardware would send.
        KeyCode::ArrowLeft => &["caps", "5"],
        KeyCode::ArrowDown => &["caps", "6"],
        KeyCode::ArrowUp => &["caps", "7"],
        KeyCode::ArrowRight => &["caps", "8"],
        KeyCode::Backspace => &["caps", "0"],
        KeyCode::Quote => &["symbol", "p"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults_to_scale_two() {
        let cli = parse_cli(std::iter::empty::<String>());

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: None,
                play_tape: false,
                autoload_tape: false,
                turbo_tape: false,
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_rom_tape_and_scale() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "48.rom".to_owned(),
            "--tape".to_owned(),
            "manic.zip".to_owned(),
            "--play-tape".to_owned(),
            "--scale".to_owned(),
            "3".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("48.rom")),
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: true,
                autoload_tape: false,
                turbo_tape: false,
                scale: 3,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_autoload() {
        let cli = parse_cli([
            "--tape".to_owned(),
            "manic.zip".to_owned(),
            "--autoload-tape".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: false,
                autoload_tape: true,
                turbo_tape: false,
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_turbo() {
        let cli = parse_cli([
            "--tape".to_owned(),
            "manic.zip".to_owned(),
            "--turbo-tape".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: false,
                autoload_tape: false,
                turbo_tape: true,
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_positional_tape_path() {
        let cli = parse_cli(["manic.zip".to_owned()]);

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: false,
                autoload_tape: false,
                turbo_tape: false,
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_video_filter() {
        let cli = parse_cli(["--video".to_owned(), "crt".to_owned()]);

        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn cursor_keys_map_to_caps_shift_combos() {
        // Matches Spectrum+ / 128K-family hardware: physical cursor
        // keys are membrane-wired as Caps Shift + 5/6/7/8.
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowLeft),
            Some(&["caps", "5"][..])
        );
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowDown),
            Some(&["caps", "6"][..])
        );
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowUp),
            Some(&["caps", "7"][..])
        );
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowRight),
            Some(&["caps", "8"][..])
        );
        assert_eq!(map_spectrum_keys(KeyCode::AltLeft), Some(&["symbol"][..]));
    }

    #[test]
    fn enter_maps_from_main_and_keypad_return() {
        assert_eq!(map_spectrum_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(
            map_spectrum_keys(KeyCode::NumpadEnter),
            Some(&["enter"][..])
        );
    }

    #[test]
    fn audio_gain_cycles_through_debug_levels() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }

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

        // The non-strict bounds let `INPUT_SLICES_PER_FRAME = 1`
        // (frame-level pacing) pass — that's the configuration that
        // matches the runtime's real granularity. Strict `<` was
        // appropriate when we believed the runtime supported
        // sub-frame slicing; it doesn't.
        assert!(slice_ticks <= frame_ticks);
        assert!(slice_ticks * u64::from(INPUT_SLICES_PER_FRAME) >= frame_ticks);
        assert!(slice_duration <= frame_duration);
    }
}
