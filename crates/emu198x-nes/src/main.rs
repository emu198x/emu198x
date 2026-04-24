//! `emu198x-nes` — minimal native NES verifier shell.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu198x_native_video::{
    PresentationProfile, VideoFilter, VideoPresenterError, WgpuVideoPresenter,
};
use emu198x_shell::{
    ButtonInputMap, ButtonTarget, CapturedFrame, HostControl, HostIo, InputEvent,
    LatestFrameCapture, MachineCore, MachineError, MediaImage, MediaKind, MediaSet,
    NativeAudioError, NativeAudioOutput, NativeGamepadInput, NullTraceSink, ResetKind, RunResult,
    read_media_asset,
};
use machine_nintendo_nes::{FB_HEIGHT, FB_WIDTH};
use runtime_nintendo_nes::{ApuChannel, Model, NesRuntime};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const DEFAULT_SCALE: u32 = 3;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;
const NES_FRAME_TICKS: u64 = 341 * 262;
const NES_PPU_DOT_HZ: f64 = 5_369_318.0;
const WINDOW_TITLE: &str = "Emu198x NES";
const NES_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "a")),
    (HostControl::East, ButtonTarget::new(1, "b")),
    (HostControl::West, ButtonTarget::new(1, "b")),
    (HostControl::Start, ButtonTarget::new(1, "start")),
    (HostControl::Select, ButtonTarget::new(1, "select")),
]);

const USAGE: &str = "\
Usage: emu198x-nes [OPTIONS] [ROM]

Options:
    --rom PATH      iNES/NES 2.0 ROM image or zip containing one ROM candidate
    --scale N       integer window scale, default 3
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Controls:
    Esc             quit
    F12             hard reset
    Arrow keys      D-pad
    Z               B
    X               A
    Right Shift     Select
    Enter           Start
    1-5             toggle Pulse 1, Pulse 2, Triangle, Noise, DMC
    6-0             cycle Pulse 1, Pulse 2, Triangle, Noise, DMC gain

Examples:
    emu198x-nes smb.nes
    emu198x-nes --rom nestest.nes --scale 2
";

#[derive(Debug, PartialEq, Eq)]
struct Cli {
    rom: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Raw,
        }
    }
}

#[derive(Debug, Error)]
enum AppError {
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

    #[error("{reason}")]
    Setup { reason: String },
}

struct NesRunner {
    runtime: NesRuntime,
    cartridge_media: Vec<u8>,
    frame_capture: LatestFrameCapture,
    audio_output: NativeAudioOutput,
    last_run_result: Option<RunResult>,
}

impl NesRunner {
    fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        let Some(path) = &cli.rom else {
            return Err(AppError::Setup {
                reason: "provide a ROM path with --rom PATH or as a positional argument".to_owned(),
            });
        };

        let loaded =
            read_media_asset(path, MediaKind::Cartridge).map_err(|err| AppError::Setup {
                reason: format!("failed to load ROM {}: {err}", path.display()),
            })?;
        let mut runtime = NesRuntime::blank(Model::NesNtsc);
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "cartridge-1",
            MediaKind::Cartridge,
            &loaded.bytes,
        ));
        runtime.load_media(&media)?;

        let mut runner = Self {
            runtime,
            cartridge_media: loaded.bytes,
            frame_capture: LatestFrameCapture::default(),
            audio_output: NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?,
            last_run_result: None,
        };
        runner.run_frame(&[])?;
        Ok(runner)
    }

    fn reset(&mut self) -> Result<(), AppError> {
        self.runtime.reset(ResetKind::Hard);
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "cartridge-1",
            MediaKind::Cartridge,
            &self.cartridge_media,
        ));
        self.runtime.load_media(&media)?;
        self.last_run_result = None;
        self.frame_capture = LatestFrameCapture::default();
        self.audio_output.clear();
        self.run_frame(&[])?;
        Ok(())
    }

    fn run_frame(&mut self, input_events: &[InputEvent]) -> Result<(), AppError> {
        let _ = self.run_ticks(input_events, NES_FRAME_TICKS)?;
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

    fn toggle_audio_channel(&mut self, channel: ApuChannel) -> Option<bool> {
        let controls = self.runtime.audio_controls()?;
        let enabled = !controls.channel(channel).enabled();
        self.runtime.set_audio_channel_enabled(channel, enabled);
        Some(enabled)
    }

    fn cycle_audio_channel_gain(&mut self, channel: ApuChannel) -> Option<f32> {
        let controls = self.runtime.audio_controls()?;
        let next = next_audio_gain(controls.channel(channel).gain());
        self.runtime.set_audio_channel_gain(channel, next);
        Some(next)
    }
}

struct NesApp {
    runner: NesRunner,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, HostControl>,
    gamepads: NativeGamepadInput,
    window: Option<std::sync::Arc<Window>>,
    video: Option<WgpuVideoPresenter>,
    presentation: PresentationProfile,
    fatal_error: Option<AppError>,
}

impl NesApp {
    fn new(runner: NesRunner, scale: u32, video: VideoFilter) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        Ok(Self {
            runner,
            scale,
            slice_ticks: subframe_ticks(NES_FRAME_TICKS),
            slice_duration: subframe_duration(nes_frame_duration()),
            next_slice_at: Instant::now(),
            pending_inputs: Vec::new(),
            pressed_keys: HashMap::new(),
            gamepads: NativeGamepadInput::new(),
            window: None,
            video: None,
            presentation: PresentationProfile::for_filter(video),
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

        let logical_width = f64::from(FB_WIDTH.saturating_mul(self.scale));
        let logical_height = f64::from(FB_HEIGHT.saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(logical_width, logical_height))
            .with_min_inner_size(LogicalSize::new(f64::from(FB_WIDTH), f64::from(FB_HEIGHT)));
        let window = std::sync::Arc::new(event_loop.create_window(attributes)?);
        let video = WgpuVideoPresenter::new(window.clone(), FB_WIDTH, FB_HEIGHT)?;

        self.window = Some(window);
        self.video = Some(video);
        self.next_slice_at = Instant::now();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn advance_machine(&mut self) -> Result<bool, AppError> {
        self.gamepads
            .drain_events(&NES_BUTTON_MAP, &mut self.pending_inputs);

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
        let Some(control) = map_nes_key(code) else {
            return;
        };

        if pressed {
            if self.pressed_keys.contains_key(&code) {
                return;
            }
            self.pressed_keys.insert(code, control);
            if let Some(input) = NES_BUTTON_MAP.event(control, true) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        } else if let Some(control) = self.pressed_keys.remove(&code) {
            if let Some(input) = NES_BUTTON_MAP.event(control, false) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        }
    }

    fn release_all_keys(&mut self) {
        let keys = std::mem::take(&mut self.pressed_keys);
        for control in keys.into_values() {
            if let Some(input) = NES_BUTTON_MAP.event(control, false) {
                self.pending_inputs.push(input);
            }
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
                    | KeyCode::F12
                    | KeyCode::Digit0
                    | KeyCode::Digit1
                    | KeyCode::Digit2
                    | KeyCode::Digit3
                    | KeyCode::Digit4
                    | KeyCode::Digit5
                    | KeyCode::Digit6
                    | KeyCode::Digit7
                    | KeyCode::Digit8
                    | KeyCode::Digit9
            );
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
            KeyCode::Digit1 => self.toggle_audio_channel_shortcut(ApuChannel::Pulse1),
            KeyCode::Digit2 => self.toggle_audio_channel_shortcut(ApuChannel::Pulse2),
            KeyCode::Digit3 => self.toggle_audio_channel_shortcut(ApuChannel::Triangle),
            KeyCode::Digit4 => self.toggle_audio_channel_shortcut(ApuChannel::Noise),
            KeyCode::Digit5 => self.toggle_audio_channel_shortcut(ApuChannel::Dmc),
            KeyCode::Digit6 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Pulse1),
            KeyCode::Digit7 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Pulse2),
            KeyCode::Digit8 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Triangle),
            KeyCode::Digit9 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Noise),
            KeyCode::Digit0 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Dmc),
            _ => false,
        }
    }

    fn toggle_audio_channel_shortcut(&mut self, channel: ApuChannel) -> bool {
        if let Some(enabled) = self.runner.toggle_audio_channel(channel) {
            eprintln!(
                "audio: {} {}",
                channel.label(),
                if enabled { "enabled" } else { "muted" }
            );
        }
        true
    }

    fn cycle_audio_channel_gain_shortcut(&mut self, channel: ApuChannel) -> bool {
        if let Some(gain) = self.runner.cycle_audio_channel_gain(channel) {
            eprintln!("audio: {} gain {:.0}%", channel.label(), gain * 100.0);
        }
        true
    }
}

impl ApplicationHandler for NesApp {
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
        "Controls: Esc quit, F12 reset, arrows/gamepad D-pad, Z/gamepad east B, X/gamepad south A, Shift Select, Enter Start, 1-5 toggle APU channels, 6-0 cycle channel gain."
    );

    let runner = NesRunner::from_cli(&cli)?;
    let mut app = NesApp::new(runner, cli.scale, cli.video)?;
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
    let mut cli = Cli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
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
                if cli.rom.is_none() {
                    cli.rom = Some(PathBuf::from(arg));
                } else {
                    die("only one positional ROM path is supported");
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

fn nes_frame_duration() -> Duration {
    Duration::from_secs_f64(NES_FRAME_TICKS as f64 / NES_PPU_DOT_HZ)
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

fn map_nes_key(code: KeyCode) -> Option<HostControl> {
    Some(match code {
        KeyCode::KeyX => HostControl::South,
        KeyCode::KeyZ => HostControl::East,
        KeyCode::ShiftRight => HostControl::Select,
        KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
        KeyCode::ArrowUp => HostControl::Up,
        KeyCode::ArrowDown => HostControl::Down,
        KeyCode::ArrowLeft => HostControl::Left,
        KeyCode::ArrowRight => HostControl::Right,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_positional_rom_and_scale() {
        let cli = parse_cli(["--scale".to_owned(), "2".to_owned(), "game.nes".to_owned()]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("game.nes")),
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_video_filter() {
        let cli = parse_cli([
            "--video".to_owned(),
            "crt".to_owned(),
            "game.nes".to_owned(),
        ]);

        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn maps_controls_to_controller_buttons() {
        assert_eq!(map_nes_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(map_nes_key(KeyCode::KeyZ), Some(HostControl::East));
        assert_eq!(map_nes_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(map_nes_key(KeyCode::ArrowLeft), Some(HostControl::Left));
    }

    #[test]
    fn audio_gain_cycles_through_debug_levels() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}
