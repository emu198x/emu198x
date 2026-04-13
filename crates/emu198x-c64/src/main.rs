//! `emu198x-c64` — minimal native Commodore 64 verification shell.
//!
//! This is intentionally narrow: one PAL/NTSC breadbin window, optional
//! startup snapshot/program import, direct keyboard input, hard reset, and live
//! audio/video over the existing runtime. It does not introduce a parallel
//! emulation stack or fake media behavior.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common_commodore_c64::timing::{TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use emu198x_shell::query::query_value;
use emu198x_shell::{
    AudioPacket, AudioSink, BootArtifacts, CapturedFrame, FirmwareImage, FirmwareSet,
    HeadlessSession, HostIo, InputEvent, LatestFrameCapture, MachineCore, MachineError,
    NullTraceSink, PixelFormat, QueryError, QueryResult, ResetKind, RunResult,
    SessionQueryProvider, boot_machine, read_firmware_asset, read_program_asset,
};
use pixels::{Pixels, SurfaceTexture, TextureError};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, Model, file_loader::load_host_file,
};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const KERNAL_ID: &str = "commodore-c64-kernal-rom";
const BASIC_ID: &str = "commodore-c64-basic-rom";
const CHARACTER_ID: &str = "commodore-c64-character-rom";
const DEFAULT_SCALE: u32 = 2;
const DEFAULT_IMPORT_BOOT_FRAMES: u32 = 200;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;

const USAGE: &str = "\
Usage: emu198x-c64 [OPTIONS]

Options:
    --rom-dir DIR        directory containing Commodore ROM images
    --kernal PATH        override KERNAL ROM path
    --basic PATH         override BASIC ROM path
    --chargen PATH       override character ROM path
    --model MODEL        pal or ntsc [default: pal]
    --load PATH          import one .prg or plain-text .bas file after boot
    --load-snapshot PATH restore a runtime snapshot before starting
    --scale N            integer window scale, default 2
    --help, -h           show this help

Controls:
    Esc                  quit
    F12                  hard reset
    Arrow keys           C64 cursor keys
    F1-F8                C64 function keys
    Alt / Command        Commodore key
    Tab                  Run/Stop

Examples:
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --load demo.bas
    emu198x-c64 --load-snapshot ready.c64.pst
";

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    model: ModelArg,
    rom_dir: Option<PathBuf>,
    kernal: Option<PathBuf>,
    basic: Option<PathBuf>,
    chargen: Option<PathBuf>,
    load: Option<PathBuf>,
    load_snapshot: Option<PathBuf>,
    scale: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ModelArg {
    #[default]
    Pal,
    Ntsc,
}

#[derive(Debug)]
struct LoadedFirmware {
    id: &'static str,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct LoadedProgram {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error(transparent)]
    Query(#[from] QueryError),

    #[error(transparent)]
    Session(#[from] emu198x_shell::SessionError),

    #[error(transparent)]
    Pixels(#[from] pixels::Error),

    #[error(transparent)]
    Texture(#[from] TextureError),

    #[error(transparent)]
    EventLoop(#[from] EventLoopError),

    #[error(transparent)]
    Os(#[from] OsError),

    #[error("audio backend failed: {reason}")]
    AudioBackend { reason: String },

    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },

    #[error("{reason}")]
    Setup { reason: String },

    #[error("frame packet used unsupported format {format:?}")]
    UnsupportedPixelFormat { format: PixelFormat },

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

struct C64Runner {
    runtime: C64Runtime,
    query_provider: C64SessionQueryProvider,
    frame_capture: LatestFrameCapture,
    audio_output: C64AudioOutput,
    last_run_result: Option<RunResult>,
    native_frame_ticks: u64,
    frame_width: u32,
    frame_height: u32,
    title_base: String,
}

impl C64Runner {
    fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        let machine = boot_runtime(cli).map_err(|reason| AppError::Setup { reason })?;
        let native_frame_ticks = cli.native_frame_ticks();
        let mut session = HeadlessSession::new_with_query_provider(
            machine,
            native_frame_ticks,
            C64SessionQueryProvider,
        );

        if let Some(path) = &cli.load {
            let _ = session.wait_for_boot(DEFAULT_IMPORT_BOOT_FRAMES)?;

            let loaded = load_program_bytes(path).map_err(|reason| AppError::Setup { reason })?;
            let message = load_host_file(session.machine_mut(), &loaded.name, &loaded.bytes)
                .map_err(|reason| AppError::Setup { reason })?;
            println!("{message}");
        }

        let runtime = session.into_machine();
        let frame_width = runtime.machine().vic().framebuffer_width();
        let frame_height = runtime.machine().vic().framebuffer_height();
        let audio_output = C64AudioOutput::new(runtime.machine().audio_sample_rate())?;
        let mut runner = Self {
            runtime,
            query_provider: C64SessionQueryProvider,
            frame_capture: LatestFrameCapture::default(),
            audio_output,
            last_run_result: None,
            native_frame_ticks,
            frame_width,
            frame_height,
            title_base: cli.window_title_base(),
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

    fn run_frame(&mut self, input_events: &[InputEvent]) -> Result<(), AppError> {
        let target = self.runtime.time().saturating_add(self.native_frame_ticks);
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events,
            frame_sink: &mut self.frame_capture,
            audio_sink: &mut self.audio_output,
            trace_sink: &mut trace_sink,
        };
        self.last_run_result = Some(self.runtime.run_until(target, &mut host)?);
        Ok(())
    }

    fn frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
    }

    fn frame_size(&self) -> (u32, u32) {
        (self.frame_width, self.frame_height)
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

    fn window_title(&self) -> String {
        let boot = if self.query_bool("boot.detected") {
            "booted"
        } else {
            "booting"
        };
        format!("{} | {}", self.title_base, boot)
    }
}

struct C64AudioOutput {
    _stream: Stream,
    shared: Arc<Mutex<AudioBuffer>>,
    sample_rate: u32,
    channels: u16,
}

impl C64AudioOutput {
    fn new(_source_rate: u32) -> Result<Self, AppError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AppError::AudioBackend {
                reason: "no default output device is available".to_owned(),
            })?;
        let supported = device
            .default_output_config()
            .map_err(|err| AppError::AudioBackend {
                reason: format!("failed to query the default output config: {err}"),
            })?;
        let config = supported.config();
        let max_samples = usize::try_from(
            (u64::from(config.sample_rate.0)
                * u64::from(config.channels)
                * u64::from(MAX_AUDIO_BUFFER_MS))
                / 1_000,
        )
        .unwrap_or(usize::MAX)
        .max(1);
        let shared = Arc::new(Mutex::new(AudioBuffer::new(max_samples)));
        let stream = build_output_stream(&device, &config, supported.sample_format(), &shared)?;
        stream.play().map_err(|err| AppError::AudioBackend {
            reason: format!("failed to start the audio stream: {err}"),
        })?;

        Ok(Self {
            _stream: stream,
            shared,
            sample_rate: config.sample_rate.0,
            channels: config.channels,
        })
    }

    fn clear(&mut self) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.samples.clear();
        }
    }
}

impl AudioSink for C64AudioOutput {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        let samples = convert_audio_packet(
            packet.samples,
            packet.sample_rate,
            packet.channels,
            self.sample_rate,
            self.channels,
        );
        let mut shared = self.shared.lock().map_err(|_| MachineError::Host {
            reason: "audio buffer lock poisoned".to_owned(),
        })?;
        shared.push(&samples);
        Ok(())
    }
}

struct AudioBuffer {
    samples: VecDeque<f32>,
    max_samples: usize,
}

impl AudioBuffer {
    fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        self.samples.extend(samples.iter().copied());
        while self.samples.len() > self.max_samples {
            let _ = self.samples.pop_front();
        }
    }
}

fn build_output_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    shared: &Arc<Mutex<AudioBuffer>>,
) -> Result<Stream, AppError> {
    match sample_format {
        SampleFormat::F32 => build_typed_output_stream::<f32>(device, config, shared),
        SampleFormat::I16 => build_typed_output_stream::<i16>(device, config, shared),
        SampleFormat::U16 => build_typed_output_stream::<u16>(device, config, shared),
        other => Err(AppError::AudioBackend {
            reason: format!("unsupported output sample format {other:?}"),
        }),
    }
}

fn build_typed_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: &Arc<Mutex<AudioBuffer>>,
) -> Result<Stream, AppError>
where
    T: SizedSample + FromSample<f32>,
{
    let shared = Arc::clone(shared);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| write_output_data(data, &shared),
            move |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|err| AppError::AudioBackend {
            reason: format!("failed to build the output stream: {err}"),
        })
}

fn write_output_data<T>(data: &mut [T], shared: &Arc<Mutex<AudioBuffer>>)
where
    T: SizedSample + FromSample<f32>,
{
    let Ok(mut shared) = shared.lock() else {
        for slot in data.iter_mut() {
            *slot = T::from_sample(0.0);
        }
        return;
    };

    for slot in data.iter_mut() {
        let sample = shared.samples.pop_front().unwrap_or(0.0);
        *slot = T::from_sample(sample);
    }
}

fn convert_audio_packet(
    samples: &[f32],
    source_rate: u32,
    source_channels: u8,
    output_rate: u32,
    output_channels: u16,
) -> Vec<f32> {
    if samples.is_empty() || source_rate == 0 || output_rate == 0 || output_channels == 0 {
        return Vec::new();
    }

    let mono = interleaved_to_mono(samples, source_channels);
    let frames_out = if source_rate == output_rate {
        mono.len()
    } else {
        usize::try_from(
            (mono.len() as u64 * u64::from(output_rate)).div_ceil(u64::from(source_rate)),
        )
        .unwrap_or(usize::MAX)
    };
    let channel_count = usize::from(output_channels);
    let mut converted = Vec::with_capacity(frames_out.saturating_mul(channel_count));

    if source_rate == output_rate {
        for &sample in &mono {
            for _ in 0..channel_count {
                converted.push(sample);
            }
        }
        return converted;
    }

    let step = f64::from(source_rate) / f64::from(output_rate);
    let last = mono.len().saturating_sub(1);

    for frame in 0..frames_out {
        let position = frame as f64 * step;
        let index = position.floor() as usize;
        let frac = (position - index as f64) as f32;
        let a = mono[index.min(last)];
        let b = mono[(index + 1).min(last)];
        let sample = a + (b - a) * frac;
        for _ in 0..channel_count {
            converted.push(sample);
        }
    }

    converted
}

fn interleaved_to_mono(samples: &[f32], channels: u8) -> Vec<f32> {
    let channel_count = usize::from(channels.max(1));
    if channel_count == 1 {
        return samples.to_vec();
    }

    let mut mono = Vec::with_capacity(samples.len().div_ceil(channel_count));
    for frame in samples.chunks(channel_count) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push(sum / frame.len() as f32);
    }
    mono
}

struct C64App {
    runner: C64Runner,
    scale: u32,
    frame_duration: Duration,
    next_frame_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, Vec<&'static str>>,
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    fatal_error: Option<AppError>,
}

impl C64App {
    fn new(runner: C64Runner, scale: u32, frame_duration: Duration) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        Ok(Self {
            runner,
            scale,
            frame_duration,
            next_frame_at: Instant::now(),
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

        let (frame_width, frame_height) = self.runner.frame_size();
        let logical_width = f64::from(frame_width.saturating_mul(self.scale));
        let logical_height = f64::from(frame_height.saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(self.runner.window_title())
            .with_inner_size(LogicalSize::new(logical_width, logical_height))
            .with_min_inner_size(LogicalSize::new(
                f64::from(frame_width),
                f64::from(frame_height),
            ));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, window.clone());
        let pixels = Pixels::new(frame_width, frame_height, surface)?;

        self.window = Some(window);
        self.pixels = Some(pixels);
        self.next_frame_at = Instant::now();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn advance_machine(&mut self) -> Result<bool, AppError> {
        let now = Instant::now();
        if now < self.next_frame_at {
            return Ok(false);
        }

        let mut ran_frames = 0;
        while Instant::now() >= self.next_frame_at && ran_frames < MAX_CATCH_UP_FRAMES {
            let inputs = std::mem::take(&mut self.pending_inputs);
            self.runner.run_frame(&inputs)?;
            self.next_frame_at += self.frame_duration;
            ran_frames += 1;
        }

        if ran_frames == MAX_CATCH_UP_FRAMES && Instant::now() >= self.next_frame_at {
            self.next_frame_at = Instant::now() + self.frame_duration;
        }

        Ok(ran_frames != 0)
    }

    fn render(&mut self) -> Result<(), AppError> {
        let Some(frame) = self.runner.frame() else {
            return Ok(());
        };
        let Some(pixels) = self.pixels.as_mut() else {
            return Ok(());
        };

        blit_rgba_frame(frame, pixels.frame_mut())?;
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
        let Some(names) = map_c64_keys(code) else {
            return;
        };

        if pressed {
            if self.pressed_keys.contains_key(&code) {
                return;
            }
            self.pressed_keys.insert(code, names.to_vec());
            self.pending_inputs
                .extend(names.iter().copied().map(|name| c64_key_event(name, true)));
            self.next_frame_at = Instant::now();
        } else if let Some(names) = self.pressed_keys.remove(&code) {
            self.pending_inputs
                .extend(names.into_iter().map(|name| c64_key_event(name, false)));
            self.next_frame_at = Instant::now();
        }
    }

    fn release_all_keys(&mut self) {
        let keys = std::mem::take(&mut self.pressed_keys);
        if keys.is_empty() {
            return;
        }
        for names in keys.into_values() {
            self.pending_inputs
                .extend(names.into_iter().map(|name| c64_key_event(name, false)));
        }
        self.next_frame_at = Instant::now();
    }

    fn handle_shortcut(
        &mut self,
        event_loop: &ActiveEventLoop,
        code: KeyCode,
        pressed: bool,
    ) -> bool {
        if !pressed {
            return matches!(code, KeyCode::Escape | KeyCode::F12);
        }

        let result = match code {
            KeyCode::Escape => {
                event_loop.exit();
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

impl ApplicationHandler for C64App {
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
                    window.set_title(&self.runner.window_title());
                    window.request_redraw();
                }
            }
            Ok(false) => {}
            Err(err) => {
                self.fail(event_loop, err);
                return;
            }
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
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
    println!("Controls: Esc quit, F12 reset.");

    let frame_duration = cli.frame_duration();
    let runner = C64Runner::from_cli(&cli)?;
    let mut app = C64App::new(runner, cli.scale, frame_duration)?;
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
            "--rom-dir" => cli.rom_dir = Some(PathBuf::from(next_arg(&mut iter, "--rom-dir"))),
            "--kernal" => cli.kernal = Some(PathBuf::from(next_arg(&mut iter, "--kernal"))),
            "--basic" => cli.basic = Some(PathBuf::from(next_arg(&mut iter, "--basic"))),
            "--chargen" => cli.chargen = Some(PathBuf::from(next_arg(&mut iter, "--chargen"))),
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--load" => cli.load = Some(PathBuf::from(next_arg(&mut iter, "--load"))),
            "--load-snapshot" => {
                cli.load_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--load-snapshot")));
            }
            "--scale" => {
                cli.scale = next_arg(&mut iter, "--scale")
                    .parse()
                    .unwrap_or_else(|_| die("--scale requires a positive integer"));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }

    cli
}

fn parse_model_arg(value: &str) -> ModelArg {
    match value {
        "pal" => ModelArg::Pal,
        "ntsc" => ModelArg::Ntsc,
        _ => die("--model expects pal or ntsc"),
    }
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
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

fn boot_runtime(cli: &Cli) -> Result<C64Runtime, String> {
    let firmware_storage = load_firmware_bytes(cli)?;
    let mut firmware = FirmwareSet::new();
    for image in &firmware_storage {
        firmware.push(FirmwareImage::new(image.id, &image.bytes));
    }

    let snapshot_bytes = match &cli.load_snapshot {
        Some(path) => Some(
            fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?,
        ),
        None => None,
    };

    boot_machine(
        &BootArtifacts {
            firmware,
            snapshot: snapshot_bytes.as_deref(),
        },
        |firmware| C64Runtime::from_firmware(cli.model.to_model(), firmware),
        || C64Runtime::blank(cli.model.to_model()),
    )
    .map_err(|err| format!("boot failed: {err}"))
}

fn load_firmware_bytes(cli: &Cli) -> Result<Vec<LoadedFirmware>, String> {
    let rom_dir = resolve_rom_dir(cli)?;
    let entries = [
        (
            KERNAL_ID,
            resolve_rom_path(
                cli.kernal.as_deref(),
                rom_dir.as_deref(),
                &["kernal.rom", "c64-kernal.rom"],
            )?,
        ),
        (
            BASIC_ID,
            resolve_rom_path(
                cli.basic.as_deref(),
                rom_dir.as_deref(),
                &["basic.rom", "c64-basic.rom"],
            )?,
        ),
        (
            CHARACTER_ID,
            resolve_rom_path(
                cli.chargen.as_deref(),
                rom_dir.as_deref(),
                &["chargen.rom", "c64-chargen.rom"],
            )?,
        ),
    ];

    entries
        .into_iter()
        .filter_map(|(id, path)| path.map(|path| (id, path)))
        .map(|(id, path)| {
            read_firmware_asset(&path)
                .map(|loaded| LoadedFirmware {
                    id,
                    bytes: loaded.bytes,
                })
                .map_err(|err| {
                    format!(
                        "failed to read firmware {id} from {}: {err}",
                        path.display()
                    )
                })
        })
        .collect()
}

fn load_program_bytes(path: &Path) -> Result<LoadedProgram, String> {
    let loaded = read_program_asset(path)
        .map_err(|err| format!("failed to read program {}: {err}", path.display()))?;
    let name = loaded.archive_member.unwrap_or_else(|| {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| path.display().to_string())
    });

    Ok(LoadedProgram {
        name,
        bytes: loaded.bytes,
    })
}

fn resolve_rom_dir(cli: &Cli) -> Result<Option<PathBuf>, String> {
    if let Some(dir) = &cli.rom_dir {
        return Ok(Some(dir.clone()));
    }

    if let Ok(dir) = std::env::var("EMU198X_C64_ROM_DIR") {
        return Ok(Some(PathBuf::from(dir)));
    }

    let Some(home) = std::env::var_os("HOME") else {
        return Ok(None);
    };
    let commodore_dir = PathBuf::from(&home).join(".emu198x/roms/commodore-c64");
    if commodore_dir.exists() {
        return Ok(Some(commodore_dir));
    }

    let legacy_dir = PathBuf::from(home).join(".emu198x/roms/c64");
    if legacy_dir.exists() {
        return Ok(Some(legacy_dir));
    }

    if cli.kernal.is_some()
        || cli.basic.is_some()
        || cli.chargen.is_some()
        || cli.load_snapshot.is_some()
    {
        return Ok(None);
    }

    Err(
        "no C64 ROM directory found — pass --rom-dir DIR, set EMU198X_C64_ROM_DIR, or create ~/.emu198x/roms/commodore-c64".into(),
    )
}

fn resolve_rom_path(
    explicit: Option<&Path>,
    rom_dir: Option<&Path>,
    filenames: &[&str],
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = explicit {
        return Ok(Some(path.to_path_buf()));
    }

    let Some(rom_dir) = rom_dir else {
        return Ok(None);
    };

    for filename in filenames {
        let candidate = rom_dir.join(filename);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }

    Err(format!(
        "missing required ROM in {} (looked for {})",
        rom_dir.display(),
        filenames.join(", ")
    ))
}

fn c64_frame_duration(model: ModelArg) -> Duration {
    let timing = match model {
        ModelArg::Pal => TIMING_PAL_BREADBIN,
        ModelArg::Ntsc => TIMING_NTSC_BREADBIN,
    };
    Duration::from_secs_f64(f64::from(timing.cycles_per_frame) / timing.cpu_hz as f64)
}

fn c64_key_event(name: &'static str, pressed: bool) -> InputEvent {
    InputEvent::Key {
        name: name.into(),
        pressed,
    }
}

fn map_c64_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Space => &["space"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::ShiftLeft => &["lshift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight | KeyCode::SuperLeft | KeyCode::SuperRight => {
            &["commodore"]
        }
        KeyCode::ArrowRight => &["right"],
        KeyCode::ArrowLeft => &["lshift", "right"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowUp => &["lshift", "down"],
        KeyCode::Home => &["home"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["lshift", "f1"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["lshift", "f3"],
        KeyCode::F5 => &["f5"],
        KeyCode::F6 => &["lshift", "f5"],
        KeyCode::F7 => &["f7"],
        KeyCode::F8 => &["lshift", "f7"],
        KeyCode::Minus => &["minus"],
        KeyCode::Equal => &["equals"],
        KeyCode::Comma => &["comma"],
        KeyCode::Period => &["period"],
        KeyCode::Slash => &["slash"],
        KeyCode::Semicolon => &["semicolon"],
        KeyCode::Quote => &["colon"],
        KeyCode::BracketLeft => &["at"],
        KeyCode::BracketRight => &["asterisk"],
        KeyCode::Backslash => &["plus"],
        KeyCode::Backquote => &["leftarrow"],
        KeyCode::Tab => &["runstop"],
        _ => return None,
    })
}

fn blit_rgba_frame(frame: &CapturedFrame, dst: &mut [u8]) -> Result<(), AppError> {
    if frame.format != PixelFormat::Rgba8888 {
        return Err(AppError::UnsupportedPixelFormat {
            format: frame.format,
        });
    }

    let expected_len = usize::try_from(frame.width)
        .ok()
        .and_then(|width| {
            usize::try_from(frame.height)
                .ok()
                .map(|height| width.saturating_mul(height).saturating_mul(4))
        })
        .unwrap_or(usize::MAX);

    if frame.pixels.len() != expected_len || dst.len() != expected_len {
        return Err(AppError::UnexpectedFrameGeometry {
            width: frame.width,
            height: frame.height,
            expected_width: frame.width,
            expected_height: frame.height,
        });
    }

    dst.copy_from_slice(&frame.pixels);
    Ok(())
}

impl Cli {
    fn native_frame_ticks(&self) -> u64 {
        match self.model {
            ModelArg::Pal => u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            ModelArg::Ntsc => u64::from(TIMING_NTSC_BREADBIN.cycles_per_frame),
        }
    }

    fn frame_duration(&self) -> Duration {
        c64_frame_duration(self.model)
    }

    fn window_title_base(&self) -> String {
        match self.model {
            ModelArg::Pal => "Emu198x Commodore 64 (PAL Breadbin)".to_owned(),
            ModelArg::Ntsc => "Emu198x Commodore 64 (NTSC Breadbin)".to_owned(),
        }
    }
}

impl ModelArg {
    const fn to_model(self) -> Model {
        match self {
            Self::Pal => Model::C64PalBreadbin,
            Self::Ntsc => Model::C64NtscBreadbin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_expected_flags() {
        let cli = parse_cli([
            "--model".to_string(),
            "ntsc".to_string(),
            "--rom-dir".to_string(),
            "roms".to_string(),
            "--load".to_string(),
            "demo.bas".to_string(),
            "--load-snapshot".to_string(),
            "ready.c64.pst".to_string(),
            "--scale".to_string(),
            "3".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::Ntsc,
                rom_dir: Some(PathBuf::from("roms")),
                kernal: None,
                basic: None,
                chargen: None,
                load: Some(PathBuf::from("demo.bas")),
                load_snapshot: Some(PathBuf::from("ready.c64.pst")),
                scale: 3,
            }
        );
    }

    #[test]
    fn key_map_covers_cursors_and_shifted_function_keys() {
        assert_eq!(
            map_c64_keys(KeyCode::ArrowLeft),
            Some(&["lshift", "right"][..])
        );
        assert_eq!(
            map_c64_keys(KeyCode::ArrowUp),
            Some(&["lshift", "down"][..])
        );
        assert_eq!(map_c64_keys(KeyCode::F2), Some(&["lshift", "f1"][..]));
        assert_eq!(map_c64_keys(KeyCode::F8), Some(&["lshift", "f7"][..]));
        assert_eq!(map_c64_keys(KeyCode::Tab), Some(&["runstop"][..]));
        assert_eq!(map_c64_keys(KeyCode::AltLeft), Some(&["commodore"][..]));
    }
}
