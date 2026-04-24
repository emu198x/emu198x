//! `emu198x-amiga` — minimal native Amiga verifier shell.

use std::collections::{HashMap, VecDeque};
use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use emu198x_shell::{
    AudioPacket, AudioSink, CapturedFrame, FirmwareImage, FirmwareSet, HostIo, InputEvent,
    LatestFrameCapture, MachineCore, MachineError, MediaImage, MediaKind, MediaSet, NullTraceSink,
    PixelFormat, ResetKind, RunResult, read_firmware_asset, read_media_asset,
};
use pixels::{Pixels, SurfaceTexture, TextureError};
use runtime_commodore_amiga::{
    A500_PAL_CCK_HZ, A500_PAL_FRAME_TICKS, AmigaRuntime, DISPLAY_HEIGHT, DISPLAY_WIDTH, Model,
};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const KICKSTART_ID: &str = "commodore-amiga-kickstart-rom";
const A1000_BOOTSTRAP_ID: &str = "commodore-amiga-a1000-bootstrap-rom";
const DEFAULT_FLOPPY_SLOT: &str = "floppy-0";
const DEFAULT_SCALE: u32 = 1;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;
const PAULA_RUNTIME_AUDIO_RATE: u32 = 48_000;
const WINDOW_TITLE: &str = "Emu198x Amiga";

const USAGE: &str = "\
Usage: emu198x-amiga [OPTIONS]

Options:
    --rom-dir DIR        directory containing Amiga ROM images
    --kickstart PATH     explicit ROM path (Kickstart on A500, bootstrap on A1000)
    --model MODEL        a1000 | a500 | a500-a501 | a500-plus | a500-maxed [default: a500]
    --disk PATH          insert one ADF image into DF0:
    --scale N            integer window scale, default 1
    --help, -h           show this help

Controls:
    Esc                  quit
    F12                  hard reset
    Mouse                port-0 Amiga mouse
    A-Z, 0-9             Amiga keyboard
    Space, Enter, Tab    Amiga keyboard
    Backspace            Amiga keyboard

ROM directory resolution (first match wins):
    1. --rom-dir DIR
    2. EMU198X_AMIGA_ROM_DIR
    3. ~/.emu198x/roms/commodore-amiga
    4. ~/.emu198x/roms/amiga

Examples:
    emu198x-amiga --model a500-a501 --disk workbench13.adf
    emu198x-amiga --kickstart kick13.rom --disk workbench13.adf
    emu198x-amiga --model a1000 --kickstart a1000-bootstrap.rom --disk kick12.adf
";

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    model: ModelArg,
    rom_dir: Option<PathBuf>,
    kickstart: Option<PathBuf>,
    disk: Option<PathBuf>,
    scale: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ModelArg {
    A1000,
    #[default]
    A500,
    A500A501,
    A500Plus,
    A500Maxed,
}

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Machine(#[from] MachineError),

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

    #[error("{reason}")]
    Setup { reason: String },

    #[error("audio backend failed: {reason}")]
    AudioBackend { reason: String },

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

struct AmigaRunner {
    runtime: AmigaRuntime,
    frame_capture: LatestFrameCapture,
    audio_output: AmigaAudioOutput,
    last_run_result: Option<RunResult>,
}

impl AmigaRunner {
    fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        let model = cli.model.to_model();
        let firmware_path =
            resolve_firmware_path(cli).map_err(|reason| AppError::Setup { reason })?;
        let firmware_bytes =
            read_firmware_asset(&firmware_path).map_err(|err| AppError::Setup {
                reason: format!(
                    "failed to read Amiga firmware {}: {err}",
                    firmware_path.display()
                ),
            })?;

        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            firmware_id_for_model_arg(cli.model),
            &firmware_bytes.bytes,
        ));
        let mut runtime = AmigaRuntime::from_firmware(model, &firmware)?;

        if let Some(path) = &cli.disk {
            let disk = read_media_asset(path, MediaKind::Disk).map_err(|err| AppError::Setup {
                reason: format!("failed to read disk {}: {err}", path.display()),
            })?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                DEFAULT_FLOPPY_SLOT,
                MediaKind::Disk,
                &disk.bytes,
            ));
            runtime.load_media(&media)?;
        }

        let mut runner = Self {
            runtime,
            frame_capture: LatestFrameCapture::default(),
            audio_output: AmigaAudioOutput::new(PAULA_RUNTIME_AUDIO_RATE)?,
            last_run_result: None,
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
        let _ = self.run_ticks(input_events, A500_PAL_FRAME_TICKS)?;
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
}

struct AmigaAudioOutput {
    _stream: Stream,
    shared: Arc<Mutex<AudioBuffer>>,
    sample_rate: u32,
    channels: u16,
}

impl AmigaAudioOutput {
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

impl AudioSink for AmigaAudioOutput {
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

struct AmigaApp {
    runner: AmigaRunner,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, &'static str>,
    pressed_mouse_buttons: HashMap<MouseButton, &'static str>,
    last_cursor_position: Option<(f64, f64)>,
    window: Option<std::sync::Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    fatal_error: Option<AppError>,
}

impl AmigaApp {
    fn new(runner: AmigaRunner, scale: u32) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        Ok(Self {
            runner,
            scale,
            slice_ticks: subframe_ticks(A500_PAL_FRAME_TICKS),
            slice_duration: subframe_duration(amiga_frame_duration()),
            next_slice_at: Instant::now(),
            pending_inputs: Vec::new(),
            pressed_keys: HashMap::new(),
            pressed_mouse_buttons: HashMap::new(),
            last_cursor_position: None,
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

        let logical_width = f64::from(DISPLAY_WIDTH.saturating_mul(self.scale));
        let logical_height = f64::from(DISPLAY_HEIGHT.saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(logical_width, logical_height))
            .with_min_inner_size(LogicalSize::new(
                f64::from(DISPLAY_WIDTH),
                f64::from(DISPLAY_HEIGHT),
            ));
        let window = std::sync::Arc::new(event_loop.create_window(attributes)?);
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, window.clone());
        let pixels = Pixels::new(DISPLAY_WIDTH, DISPLAY_HEIGHT, surface)?;

        self.window = Some(window);
        self.pixels = Some(pixels);
        self.next_slice_at = Instant::now();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn advance_machine(&mut self) -> Result<bool, AppError> {
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
        let Some(name) = map_amiga_key(code) else {
            return;
        };

        if pressed {
            if self.pressed_keys.contains_key(&code) {
                return;
            }
            self.pressed_keys.insert(code, name);
            self.pending_inputs.push(key_event(name, true));
            self.next_slice_at = Instant::now();
        } else if let Some(name) = self.pressed_keys.remove(&code) {
            self.pending_inputs.push(key_event(name, false));
            self.next_slice_at = Instant::now();
        }
    }

    fn queue_mouse_motion(&mut self, x: f64, y: f64) {
        let (x, y) = self.cursor_to_frame_position(x, y);
        let Some((last_x, last_y)) = self.last_cursor_position.replace((x, y)) else {
            return;
        };

        let dx = round_f64_to_i32(x - last_x);
        let dy = round_f64_to_i32(y - last_y);
        if dx == 0 && dy == 0 {
            return;
        }

        self.pending_inputs.push(InputEvent::PointerMotion {
            device: "mouse-1".into(),
            dx,
            dy,
        });
        self.next_slice_at = Instant::now();
    }

    fn cursor_to_frame_position(&self, x: f64, y: f64) -> (f64, f64) {
        let Some(window) = &self.window else {
            return (x, y);
        };
        let size = window.inner_size();
        let width = f64::from(size.width.max(1));
        let height = f64::from(size.height.max(1));
        (
            x * f64::from(DISPLAY_WIDTH) / width,
            y * f64::from(DISPLAY_HEIGHT) / height,
        )
    }

    fn queue_mouse_button_state(&mut self, button: MouseButton, pressed: bool) {
        let Some(name) = map_mouse_button(button) else {
            return;
        };

        if pressed {
            if self.pressed_mouse_buttons.contains_key(&button) {
                return;
            }
            self.pressed_mouse_buttons.insert(button, name);
            self.pending_inputs.push(pointer_button_event(name, true));
            self.next_slice_at = Instant::now();
        } else if let Some(name) = self.pressed_mouse_buttons.remove(&button) {
            self.pending_inputs.push(pointer_button_event(name, false));
            self.next_slice_at = Instant::now();
        }
    }

    fn release_all_keys(&mut self) {
        let keys = std::mem::take(&mut self.pressed_keys);
        for name in keys.into_values() {
            self.pending_inputs.push(key_event(name, false));
        }
        let buttons = std::mem::take(&mut self.pressed_mouse_buttons);
        for name in buttons.into_values() {
            self.pending_inputs.push(pointer_button_event(name, false));
        }
        self.last_cursor_position = None;
        self.next_slice_at = Instant::now();
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

        match code {
            KeyCode::Escape => {
                event_loop.exit();
                true
            }
            KeyCode::F12 => {
                self.release_all_keys();
                if let Err(err) = self.runner.reset() {
                    self.fail(event_loop, err);
                }
                true
            }
            _ => false,
        }
    }
}

impl ApplicationHandler for AmigaApp {
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
            WindowEvent::CursorMoved { position, .. } => {
                self.queue_mouse_motion(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.queue_mouse_button_state(button, state == ElementState::Pressed);
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
                    window.request_redraw();
                }
            }
            Ok(false) => {}
            Err(err) => {
                self.fail(event_loop, err);
                return;
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_slice_at));
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
        "Controls: Esc quit, F12 reset, mouse port 0, A-Z/0-9/Space/Enter/Tab/Backspace keyboard."
    );

    let runner = AmigaRunner::from_cli(&cli)?;
    let mut app = AmigaApp::new(runner, cli.scale)?;
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
            "--kickstart" => {
                cli.kickstart = Some(PathBuf::from(next_arg(&mut iter, "--kickstart")));
            }
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--disk" => cli.disk = Some(PathBuf::from(next_arg(&mut iter, "--disk"))),
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
        "a1000" => ModelArg::A1000,
        "a500" => ModelArg::A500,
        "a500-a501" => ModelArg::A500A501,
        "a500-plus" => ModelArg::A500Plus,
        "a500-maxed" => ModelArg::A500Maxed,
        _ => die("--model expects a1000, a500, a500-a501, a500-plus, or a500-maxed"),
    }
}

fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a path or value")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

impl ModelArg {
    const fn to_model(self) -> Model {
        match self {
            Self::A1000 => Model::A1000OcsPal,
            Self::A500 => Model::A500OcsPal,
            Self::A500A501 => Model::A500OcsPalA501,
            Self::A500Plus => Model::A500PlusOcsPal,
            Self::A500Maxed => Model::A500OcsPalMaxed,
        }
    }
}

fn firmware_id_for_model_arg(model: ModelArg) -> &'static str {
    match model {
        ModelArg::A1000 => A1000_BOOTSTRAP_ID,
        ModelArg::A500 | ModelArg::A500A501 | ModelArg::A500Plus | ModelArg::A500Maxed => {
            KICKSTART_ID
        }
    }
}

fn resolve_firmware_path(cli: &Cli) -> Result<PathBuf, String> {
    if let Some(path) = &cli.kickstart {
        return Ok(path.clone());
    }

    let rom_dir = candidate_rom_dirs(cli)
        .into_iter()
        .find(|dir| dir.is_dir())
        .ok_or_else(|| {
            "no Amiga ROM directory found; use --kickstart PATH or --rom-dir DIR".to_owned()
        })?;

    let candidates: &[&str] = match cli.model {
        ModelArg::A1000 => &[
            "a1000-bootstrap.rom",
            "a1000_bootstrap.rom",
            "bootstrap.rom",
        ],
        ModelArg::A500 | ModelArg::A500A501 | ModelArg::A500Plus | ModelArg::A500Maxed => &[
            "kick13.rom",
            "kick12.rom",
            "kick31.rom",
            "kickstart.rom",
            "kick.rom",
        ],
    };

    for name in candidates {
        let path = rom_dir.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(format!(
        "no Amiga firmware ROM found in {}; tried {}",
        rom_dir.display(),
        candidates.join(", ")
    ))
}

fn candidate_rom_dirs(cli: &Cli) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = &cli.rom_dir {
        dirs.push(dir.clone());
    }
    if let Some(dir) = env::var_os("EMU198X_AMIGA_ROM_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(home) = env::var_os("HOME") {
        dirs.push(Path::new(&home).join(".emu198x/roms/commodore-amiga"));
        dirs.push(Path::new(&home).join(".emu198x/roms/amiga"));
    }
    dirs
}

fn amiga_frame_duration() -> Duration {
    Duration::from_secs_f64(A500_PAL_FRAME_TICKS as f64 / (A500_PAL_CCK_HZ * 2) as f64)
}

fn subframe_ticks(frame_ticks: u64) -> u64 {
    frame_ticks.div_ceil(u64::from(INPUT_SLICES_PER_FRAME))
}

fn subframe_duration(frame_duration: Duration) -> Duration {
    Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(INPUT_SLICES_PER_FRAME))
}

fn key_event(name: &'static str, pressed: bool) -> InputEvent {
    InputEvent::Key {
        name: name.into(),
        pressed,
    }
}

fn pointer_button_event(name: &'static str, pressed: bool) -> InputEvent {
    InputEvent::PointerButton {
        device: "mouse-1".into(),
        button: name.into(),
        pressed,
    }
}

fn round_f64_to_i32(value: f64) -> i32 {
    if value.is_nan() {
        0
    } else if value > f64::from(i32::MAX) {
        i32::MAX
    } else if value < f64::from(i32::MIN) {
        i32::MIN
    } else {
        value.round() as i32
    }
}

fn map_amiga_key(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::Digit0 => "0",
        KeyCode::KeyA => "a",
        KeyCode::KeyB => "b",
        KeyCode::KeyC => "c",
        KeyCode::KeyD => "d",
        KeyCode::KeyE => "e",
        KeyCode::KeyF => "f",
        KeyCode::KeyG => "g",
        KeyCode::KeyH => "h",
        KeyCode::KeyI => "i",
        KeyCode::KeyJ => "j",
        KeyCode::KeyK => "k",
        KeyCode::KeyL => "l",
        KeyCode::KeyM => "m",
        KeyCode::KeyN => "n",
        KeyCode::KeyO => "o",
        KeyCode::KeyP => "p",
        KeyCode::KeyQ => "q",
        KeyCode::KeyR => "r",
        KeyCode::KeyS => "s",
        KeyCode::KeyT => "t",
        KeyCode::KeyU => "u",
        KeyCode::KeyV => "v",
        KeyCode::KeyW => "w",
        KeyCode::KeyX => "x",
        KeyCode::KeyY => "y",
        KeyCode::KeyZ => "z",
        KeyCode::Space => "space",
        KeyCode::Backspace => "backspace",
        KeyCode::Tab => "tab",
        KeyCode::Enter | KeyCode::NumpadEnter => "enter",
        _ => return None,
    })
}

fn map_mouse_button(button: MouseButton) -> Option<&'static str> {
    Some(match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        _ => return None,
    })
}

fn blit_rgba_frame(frame: &CapturedFrame, target: &mut [u8]) -> Result<(), AppError> {
    if frame.format != PixelFormat::Rgba8888 {
        return Err(AppError::UnsupportedPixelFormat {
            format: frame.format,
        });
    }

    if frame.width != DISPLAY_WIDTH || frame.height != DISPLAY_HEIGHT {
        return Err(AppError::UnexpectedFrameGeometry {
            width: frame.width,
            height: frame.height,
            expected_width: DISPLAY_WIDTH,
            expected_height: DISPLAY_HEIGHT,
        });
    }

    target.copy_from_slice(&frame.pixels);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_model_disk_and_scale() {
        let cli = parse_cli([
            "--model".to_owned(),
            "a500-a501".to_owned(),
            "--disk".to_owned(),
            "workbench13.adf".to_owned(),
            "--scale".to_owned(),
            "2".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::A500A501,
                rom_dir: None,
                kickstart: None,
                disk: Some(PathBuf::from("workbench13.adf")),
                scale: 2,
            }
        );
    }

    #[test]
    fn maps_basic_keyboard_keys() {
        assert_eq!(map_amiga_key(KeyCode::KeyA), Some("a"));
        assert_eq!(map_amiga_key(KeyCode::Digit1), Some("1"));
        assert_eq!(map_amiga_key(KeyCode::Space), Some("space"));
        assert_eq!(map_amiga_key(KeyCode::Enter), Some("enter"));
    }

    #[test]
    fn maps_mouse_buttons() {
        assert_eq!(map_mouse_button(MouseButton::Left), Some("left"));
        assert_eq!(map_mouse_button(MouseButton::Right), Some("right"));
        assert_eq!(map_mouse_button(MouseButton::Middle), Some("middle"));
        assert_eq!(map_mouse_button(MouseButton::Other(1)), None);
    }

    #[test]
    fn audio_conversion_downmixes_stereo_at_same_rate() {
        let converted = convert_audio_packet(&[0.25, -0.5, 0.75, -1.0], 48_000, 2, 48_000, 2);

        assert_eq!(converted, vec![-0.125, -0.125, -0.125, -0.125]);
    }

    #[test]
    fn audio_conversion_resamples_to_output_rate() {
        let converted = convert_audio_packet(&[0.0, 0.0, 1.0, 1.0], 2, 2, 4, 1);

        assert_eq!(converted, vec![0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn model_args_map_to_runtime_models() {
        assert_eq!(ModelArg::A1000.to_model(), Model::A1000OcsPal);
        assert_eq!(ModelArg::A500.to_model(), Model::A500OcsPal);
        assert_eq!(ModelArg::A500A501.to_model(), Model::A500OcsPalA501);
        assert_eq!(ModelArg::A500Plus.to_model(), Model::A500PlusOcsPal);
        assert_eq!(ModelArg::A500Maxed.to_model(), Model::A500OcsPalMaxed);
    }
}
