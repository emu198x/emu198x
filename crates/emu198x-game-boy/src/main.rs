//! `emu198x-game-boy` — minimal native Game Boy verifier shell.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use common_nintendo_game_boy::timing::MCYCLE_HZ;
use common_nintendo_game_boy::{MCYCLES_PER_FRAME, SCREEN_HEIGHT, SCREEN_WIDTH};
use emu198x_shell::{
    ButtonInputMap, ButtonTarget, CapturedFrame, HostControl, HostIo, InputEvent,
    LatestFrameCapture, MachineCore, MachineError, MediaImage, MediaKind, MediaSet,
    NativeAudioError, NativeAudioOutput, NativeGamepadInput, NullTraceSink, PixelFormat, ResetKind,
    RunResult, read_media_asset,
};
use pixels::{Pixels, SurfaceTexture, TextureError};
use runtime_nintendo_game_boy::{ApuChannel, AudioControls, GameBoyRuntime, Model};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const DEFAULT_SCALE: u32 = 4;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;
const WINDOW_TITLE: &str = "Emu198x Game Boy";
const GAME_BOY_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
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
Usage: emu198x-game-boy [OPTIONS] [ROM]

Options:
    --rom PATH            Game Boy ROM image or zip containing one ROM candidate
    --model MODEL         dmg0 | dmg | mgb | sgb | sgb2 [default: dmg]
    --load-snapshot PATH  restore a runtime snapshot before starting
    --scale N             integer window scale, default 4
    --help, -h            show this help

Controls:
    Esc                   quit
    F12                   hard reset
    1-4                   toggle audio channels: pulse1, pulse2, wave, noise
    5-8                   cycle channel gain: 100%, 50%, 25%, muted
    0                     reset audio channel controls
    Arrow keys            D-pad
    Z                     B
    X                     A
    Right Shift           Select
    Enter                 Start

Examples:
    emu198x-game-boy tetris.gb
    emu198x-game-boy --rom game.gb --model mgb
    emu198x-game-boy --load-snapshot ready.gb.pst
";

#[derive(Debug, PartialEq, Eq)]
struct Cli {
    rom: Option<PathBuf>,
    model: Model,
    load_snapshot: Option<PathBuf>,
    scale: u32,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            model: Model::Dmg,
            load_snapshot: None,
            scale: DEFAULT_SCALE,
        }
    }
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

    #[error(transparent)]
    Audio(#[from] NativeAudioError),

    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },

    #[error("{reason}")]
    Setup { reason: String },

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

struct GameBoyRunner {
    runtime: GameBoyRuntime,
    frame_capture: LatestFrameCapture,
    audio_output: NativeAudioOutput,
    last_run_result: Option<RunResult>,
    native_frame_ticks: u64,
}

impl GameBoyRunner {
    fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        if cli.rom.is_none() && cli.load_snapshot.is_none() {
            return Err(AppError::Setup {
                reason: "provide a ROM path or --load-snapshot PATH".to_owned(),
            });
        }

        let mut runtime = GameBoyRuntime::blank(cli.model);
        if let Some(path) = &cli.load_snapshot {
            let bytes = std::fs::read(path).map_err(|err| AppError::Setup {
                reason: format!("failed to read snapshot {}: {err}", path.display()),
            })?;
            runtime.restore(&bytes)?;
        }
        if let Some(path) = &cli.rom {
            let loaded =
                read_media_asset(path, MediaKind::Cartridge).map_err(|err| AppError::Setup {
                    reason: format!("failed to load ROM {}: {err}", path.display()),
                })?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                "cartridge",
                MediaKind::Cartridge,
                &loaded.bytes,
            ));
            runtime.load_media(&media)?;
        }

        let mut runner = Self {
            runtime,
            frame_capture: LatestFrameCapture::default(),
            audio_output: NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?,
            last_run_result: None,
            native_frame_ticks: u64::from(MCYCLES_PER_FRAME),
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

    fn reset_audio_controls(&mut self) {
        self.runtime.set_audio_controls(AudioControls::default());
    }
}

struct GameBoyApp {
    runner: GameBoyRunner,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, HostControl>,
    gamepads: NativeGamepadInput,
    window: Option<std::sync::Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    fatal_error: Option<AppError>,
}

impl GameBoyApp {
    fn new(runner: GameBoyRunner, scale: u32) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        Ok(Self {
            runner,
            scale,
            slice_ticks: subframe_ticks(u64::from(MCYCLES_PER_FRAME)),
            slice_duration: subframe_duration(game_boy_frame_duration()),
            next_slice_at: Instant::now(),
            pending_inputs: Vec::new(),
            pressed_keys: HashMap::new(),
            gamepads: NativeGamepadInput::new(),
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

        let logical_width = f64::from(SCREEN_WIDTH.saturating_mul(self.scale));
        let logical_height = f64::from(SCREEN_HEIGHT.saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(logical_width, logical_height))
            .with_min_inner_size(LogicalSize::new(
                f64::from(SCREEN_WIDTH),
                f64::from(SCREEN_HEIGHT),
            ));
        let window = std::sync::Arc::new(event_loop.create_window(attributes)?);
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, window.clone());
        let pixels = Pixels::new(SCREEN_WIDTH, SCREEN_HEIGHT, surface)?;

        self.window = Some(window);
        self.pixels = Some(pixels);
        self.next_slice_at = Instant::now();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn advance_machine(&mut self) -> Result<bool, AppError> {
        self.gamepads
            .drain_events(&GAME_BOY_BUTTON_MAP, &mut self.pending_inputs);

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
        let Some(control) = map_game_boy_key(code) else {
            return;
        };

        if pressed {
            if self.pressed_keys.contains_key(&code) {
                return;
            }
            self.pressed_keys.insert(code, control);
            if let Some(input) = GAME_BOY_BUTTON_MAP.event(control, true) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        } else if let Some(control) = self.pressed_keys.remove(&code) {
            if let Some(input) = GAME_BOY_BUTTON_MAP.event(control, false) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        }
    }

    fn release_all_keys(&mut self) {
        let keys = std::mem::take(&mut self.pressed_keys);
        for control in keys.into_values() {
            if let Some(input) = GAME_BOY_BUTTON_MAP.event(control, false) {
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
            KeyCode::Digit0 => {
                self.runner.reset_audio_controls();
                eprintln!("audio: reset channel controls");
                true
            }
            KeyCode::Digit1 => self.toggle_audio_channel_shortcut(ApuChannel::Pulse1),
            KeyCode::Digit2 => self.toggle_audio_channel_shortcut(ApuChannel::Pulse2),
            KeyCode::Digit3 => self.toggle_audio_channel_shortcut(ApuChannel::Wave),
            KeyCode::Digit4 => self.toggle_audio_channel_shortcut(ApuChannel::Noise),
            KeyCode::Digit5 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Pulse1),
            KeyCode::Digit6 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Pulse2),
            KeyCode::Digit7 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Wave),
            KeyCode::Digit8 => self.cycle_audio_channel_gain_shortcut(ApuChannel::Noise),
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

impl ApplicationHandler for GameBoyApp {
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
        "Controls: Esc quit, F12 reset, arrows/gamepad D-pad, Z/gamepad east B, X/gamepad south A, Shift Select, Enter Start. Audio: 1-4 toggle channels, 5-8 cycle channel gain, 0 reset audio."
    );

    let runner = GameBoyRunner::from_cli(&cli)?;
    let mut app = GameBoyApp::new(runner, cli.scale)?;
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
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
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

fn parse_model_arg(model: &str) -> Model {
    match model {
        "dmg0" => Model::Dmg0,
        "dmg" => Model::Dmg,
        "mgb" => Model::Mgb,
        "sgb" => Model::Sgb,
        "sgb2" => Model::Sgb2,
        _ => die("--model expects dmg0, dmg, mgb, sgb, or sgb2"),
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
    process::exit(1);
}

fn game_boy_frame_duration() -> Duration {
    Duration::from_secs_f64(f64::from(MCYCLES_PER_FRAME) / f64::from(MCYCLE_HZ))
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

fn map_game_boy_key(code: KeyCode) -> Option<HostControl> {
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

fn blit_indexed_frame(frame: &CapturedFrame, target: &mut [u8]) -> Result<(), AppError> {
    if frame.format != PixelFormat::Indexed8 {
        return Err(AppError::UnsupportedPixelFormat {
            format: frame.format,
        });
    }

    if frame.width != SCREEN_WIDTH || frame.height != SCREEN_HEIGHT {
        return Err(AppError::UnexpectedFrameGeometry {
            width: frame.width,
            height: frame.height,
            expected_width: SCREEN_WIDTH,
            expected_height: SCREEN_HEIGHT,
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
    fn parse_cli_accepts_positional_rom_and_model() {
        let cli = parse_cli([
            "--model".to_owned(),
            "mgb".to_owned(),
            "--scale".to_owned(),
            "5".to_owned(),
            "game.gb".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("game.gb")),
                model: Model::Mgb,
                load_snapshot: None,
                scale: 5,
            }
        );
    }

    #[test]
    fn maps_controls_to_joypad_buttons() {
        assert_eq!(map_game_boy_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(map_game_boy_key(KeyCode::KeyZ), Some(HostControl::East));
        assert_eq!(map_game_boy_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(
            map_game_boy_key(KeyCode::ArrowLeft),
            Some(HostControl::Left)
        );
    }

    #[test]
    fn audio_gain_shortcut_cycles_down_then_restores() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}
