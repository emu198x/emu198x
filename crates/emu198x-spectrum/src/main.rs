//! `emu198x-spectrum` — minimal native Spectrum verification shell.
//!
//! This is intentionally narrow: one 48K window, optional ROM/tape loading,
//! direct keyboard input, and basic media transport control for interactive
//! verification. It sits above the existing runtime and shared shell boundary;
//! it does not introduce a parallel emulation stack.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K};
use emu198x_shell::query::query_value;
use emu198x_shell::{
    AssetLoadError, CapturedFrame, ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession,
    HostIo, InputEvent, LatestFrameCapture, MachineCore, MachineError, MediaImage, MediaKind,
    MediaSet, MediaTransportAction, MediaTransportCommand, NativeAudioError, NativeAudioOutput,
    NullTraceSink, PixelFormat, QueryError, QueryResult, ResetKind, RunResult,
    SessionQueryProvider, read_firmware_asset, read_media_asset,
};
use pixels::{Pixels, SurfaceTexture, TextureError};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, Spectrum48kRuntime,
    SpectrumSessionQueryProvider, autoload_basic_tape,
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
const WINDOW_TITLE_BASE: &str = "Emu198x Spectrum 48K";
const INPUT_SLICES_PER_FRAME: u32 = 8;
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
    --help, -h         show this help

Controls:
    Esc                quit
    F9                 start tape
    F10                stop tape
    F11                toggle tape turbo
    F12                hard reset
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
    Pixels(#[from] pixels::Error),

    #[error(transparent)]
    Texture(#[from] TextureError),

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

    #[error("frame packet used unsupported format {format:?}")]
    UnsupportedPixelFormat { format: PixelFormat },

    #[error("indexed frame is missing a palette")]
    MissingPalette,

    #[error(
        "frame geometry {width}x{height} does not match expected {expected_width}x{expected_height}"
    )]
    UnexpectedFrameGeometry {
        width: u32,
        height: u32,
        expected_width: u32,
        expected_height: u32,
    },
}

struct SpectrumRunner {
    runtime: Spectrum48kRuntime,
    query_provider: SpectrumSessionQueryProvider,
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

        let runtime = session.into_machine();
        let audio_output = NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?;
        let mut runner = Self {
            runtime,
            query_provider: SpectrumSessionQueryProvider,
            frame_capture: LatestFrameCapture::default(),
            audio_output,
            last_run_result: None,
            native_frame_ticks: u64::from(TIMING_48K.halfcycles_per_frame),
        };
        runner.run_frame(&[])?;
        Ok(runner)
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

    fn frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
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
                .query_provider
                .query(&self.runtime, path)?
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

    fn query_text_line(&self, path: &str, line_index: usize) -> Option<String> {
        self.query(path)
            .ok()
            .and_then(|result| result.value.as_array().cloned())
            .and_then(|lines| lines.get(line_index).cloned())
            .and_then(|line| line.as_str().map(str::to_owned))
    }

    fn window_title(&self) -> String {
        let boot = if self.query_bool("boot.detected") {
            "booted"
        } else {
            "booting"
        };
        let tape = match (
            self.query_bool("spectrum.tape.loaded"),
            self.query_bool("spectrum.tape.playing"),
        ) {
            (true, true) => "tape playing",
            (true, false) => "tape loaded",
            (false, _) => "no tape",
        };
        let prompt = self
            .query_text_line("screen.text.lines", 23)
            .unwrap_or_default();
        let prompt = prompt.trim();

        if prompt.is_empty() {
            format!("{WINDOW_TITLE_BASE} | {boot} | {tape}")
        } else {
            format!("{WINDOW_TITLE_BASE} | {boot} | {tape} | {prompt}")
        }
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
    pixels: Option<Pixels<'static>>,
    fatal_error: Option<AppError>,
}

impl SpectrumApp {
    fn new(runner: SpectrumRunner, scale: u32, turbo_tape: bool) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        let slice_ticks = subframe_ticks(runner.native_frame_ticks);
        let slice_duration = subframe_duration(spectrum_frame_duration());
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
            pixels: None,
            fatal_error: None,
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
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, window.clone());
        let pixels = Pixels::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32, surface)?;

        self.window = Some(window);
        self.pixels = Some(pixels);
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

    fn advance_machine(&mut self) -> Result<bool, AppError> {
        if self.turbo_tape_active() {
            let mut ran_frames = 0;
            while ran_frames < MAX_TURBO_TAPE_FRAMES && self.turbo_tape_active() {
                let inputs = std::mem::take(&mut self.pending_inputs);
                self.runner.run_frame(&inputs)?;
                ran_frames += 1;
            }
            self.next_slice_at = Instant::now() + self.slice_duration;
            return Ok(ran_frames != 0);
        }

        let now = Instant::now();
        if now < self.next_slice_at {
            return Ok(false);
        }

        let mut ran_slices = 0;
        let max_catch_up_slices = MAX_CATCH_UP_FRAMES.saturating_mul(INPUT_SLICES_PER_FRAME);
        let mut frame_completed = false;
        while Instant::now() >= self.next_slice_at && ran_slices < max_catch_up_slices {
            let inputs = std::mem::take(&mut self.pending_inputs);
            frame_completed |= self.runner.run_ticks(&inputs, self.slice_ticks)?;
            self.next_slice_at += self.slice_duration;
            ran_slices += 1;
        }

        if ran_slices == max_catch_up_slices && Instant::now() >= self.next_slice_at {
            self.next_slice_at = Instant::now() + self.slice_duration;
        }

        Ok(frame_completed)
    }

    fn render(&mut self) -> Result<(), AppError> {
        let Some(frame) = self.runner.frame() else {
            return Ok(());
        };
        let Some(pixels) = self.pixels.as_mut() else {
            return Ok(());
        };

        blit_indexed_frame(frame, pixels.frame_mut())?;
        pixels.render()?;
        Ok(())
    }

    fn resize_surface(&mut self, width: u32, height: u32) -> Result<(), AppError> {
        if let Some(pixels) = self.pixels.as_mut() {
            pixels.resize_surface(width, height)?;
        }
        Ok(())
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
                KeyCode::Escape | KeyCode::F9 | KeyCode::F10 | KeyCode::F11 | KeyCode::F12
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
                _ => return false,
            };

        if let Err(err) = result {
            self.fail(event_loop, err);
        }
        true
    }
}

impl ApplicationHandler for SpectrumApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.create_window(event_loop) {
            self.fail(event_loop, err);
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
                if let Err(err) = self.resize_surface(size.width, size.height) {
                    self.fail(event_loop, err);
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    if let Err(err) = self.resize_surface(size.width, size.height) {
                        self.fail(event_loop, err);
                    }
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
        match self.advance_machine() {
            Ok(true) => {
                if let Some(window) = &self.window {
                    window.set_title(&self.window_title());
                    window.request_redraw();
                }
            }
            Ok(false) => {}
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
    println!("Controls: Esc quit, F9 start tape, F10 stop tape, F11 tape turbo, F12 reset.");

    let runner = SpectrumRunner::from_cli(&cli)?;
    let mut app = SpectrumApp::new(runner, cli.scale, cli.turbo_tape)?;
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

fn spectrum_frame_duration() -> Duration {
    spectrum_duration_for_ticks(u64::from(TIMING_48K.halfcycles_per_frame))
}

fn spectrum_duration_for_ticks(halfcycles: u64) -> Duration {
    Duration::from_secs_f64(halfcycles as f64 / TIMING_48K.master_hz as f64)
}

fn subframe_ticks(frame_ticks: u64) -> u64 {
    frame_ticks.div_ceil(u64::from(INPUT_SLICES_PER_FRAME))
}

fn subframe_duration(frame_duration: Duration) -> Duration {
    Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(INPUT_SLICES_PER_FRAME))
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
        // Host arrow keys are gameplay aliases for 5/6/7/8 in the minimal
        // verifier shell. Synthesizing literal Caps Shift cursor combos causes
        // false extra controls in games that read the matrix directly.
        KeyCode::ArrowLeft => &["5"],
        KeyCode::ArrowDown => &["6"],
        KeyCode::ArrowUp => &["7"],
        KeyCode::ArrowRight => &["8"],
        KeyCode::Backspace => &["caps", "0"],
        KeyCode::Quote => &["symbol", "p"],
        _ => return None,
    })
}

fn blit_indexed_frame(frame: &CapturedFrame, target: &mut [u8]) -> Result<(), AppError> {
    if frame.format != PixelFormat::Indexed8 {
        return Err(AppError::UnsupportedPixelFormat {
            format: frame.format,
        });
    }

    if frame.width != SCREEN_WIDTH as u32 || frame.height != SCREEN_HEIGHT as u32 {
        return Err(AppError::UnexpectedFrameGeometry {
            width: frame.width,
            height: frame.height,
            expected_width: SCREEN_WIDTH as u32,
            expected_height: SCREEN_HEIGHT as u32,
        });
    }

    let palette = frame.palette.as_ref().ok_or(AppError::MissingPalette)?;
    for (index, rgba) in frame.pixels.iter().zip(target.chunks_exact_mut(4)) {
        let value = palette[*index as usize];
        rgba[0] = (value >> 24) as u8;
        rgba[1] = (value >> 16) as u8;
        rgba[2] = (value >> 8) as u8;
        rgba[3] = value as u8;
    }

    Ok(())
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
            }
        );
    }

    #[test]
    fn cursor_keys_map_to_game_key_aliases() {
        assert_eq!(map_spectrum_keys(KeyCode::ArrowLeft), Some(&["5"][..]));
        assert_eq!(map_spectrum_keys(KeyCode::ArrowUp), Some(&["7"][..]));
        assert_eq!(map_spectrum_keys(KeyCode::ArrowRight), Some(&["8"][..]));
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
    fn subframe_helpers_preserve_timing_budget() {
        let frame_ticks = u64::from(TIMING_48K.halfcycles_per_frame);
        let slice_ticks = subframe_ticks(frame_ticks);
        let slice_duration = subframe_duration(spectrum_frame_duration());

        assert!(slice_ticks < frame_ticks);
        assert!(slice_ticks * u64::from(INPUT_SLICES_PER_FRAME) >= frame_ticks);
        assert!(slice_duration < spectrum_frame_duration());
        assert!(spectrum_duration_for_ticks(slice_ticks) < spectrum_frame_duration());
    }
}
