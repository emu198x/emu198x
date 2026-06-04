//! Interactive UI mode — `--ui` (default).
//!
//! A native Amiga verifier window: shared `wgpu` video, live Paula
//! audio, mouse / keyboard / joystick input, and DF0 ADF insertion
//! across the A1000 / A500-family / A600 / A1200 / A2000 models.
//! Compiled only with the `ui` Cargo feature; the dispatcher in
//! `main.rs` routes here when no headless-only flag is present.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu198x_native_video::{PresentationProfile, VideoFilter, WgpuVideoPresenter};
use emu198x_shell::{
    ButtonInputMap, ButtonTarget, CapturedFrame, FirmwareImage, FirmwareSet, HostControl, HostIo,
    InputEvent, LatestFrameCapture, MachineCore, MediaImage, MediaKind, MediaSet,
    NativeAudioOutput, NativeGamepadInput, NullTraceSink, ResetKind, RunResult,
    read_firmware_asset, read_media_asset,
};
use runtime_commodore_amiga::{
    A500_PAL_CCK_HZ, A500_PAL_FRAME_TICKS, AmigaRuntimeKind, AudioControls, DISPLAY_HEIGHT,
    DISPLAY_WIDTH, PaulaChannel,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::{AppError, ModelArg, USAGE, die, find_rom_path, next_arg, parse_model_arg};

const KICKSTART_ID: &str = "commodore-amiga-kickstart-rom";
const A1000_BOOTSTRAP_ID: &str = "commodore-amiga-a1000-bootstrap-rom";
const DEFAULT_FLOPPY_SLOT: &str = "floppy-0";
const DEFAULT_SCALE: u32 = 1;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;
const WINDOW_TITLE: &str = "Emu198x Amiga";
// The joystick lives in Amiga control port 2 (JOY1DAT) per *Mapping the
// Amiga* p.460 — the runtime maps input port 2 onto the machine's
// joystick. (Mouse is port 1 / JOY0DAT, routed via pointer events.)
const AMIGA_JOYSTICK_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(2, "up")),
    (HostControl::Down, ButtonTarget::new(2, "down")),
    (HostControl::Left, ButtonTarget::new(2, "left")),
    (HostControl::Right, ButtonTarget::new(2, "right")),
    (HostControl::South, ButtonTarget::new(2, "fire")),
    (HostControl::East, ButtonTarget::new(2, "fire")),
    (HostControl::West, ButtonTarget::new(2, "fire")),
    (HostControl::North, ButtonTarget::new(2, "fire")),
]);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    model: ModelArg,
    rom_dir: Option<PathBuf>,
    kickstart: Option<PathBuf>,
    disk: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

struct AmigaRunner {
    runtime: AmigaRuntimeKind,
    frame_capture: LatestFrameCapture,
    audio_output: NativeAudioOutput,
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
        let mut runtime = AmigaRuntimeKind::from_firmware(model, &firmware)?;

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
            audio_output: NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?,
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

    fn toggle_audio_channel(&mut self, channel: PaulaChannel) -> bool {
        let controls = self.runtime.audio_controls();
        let enabled = !controls.channel(channel).enabled();
        self.runtime.set_audio_channel_enabled(channel, enabled);
        enabled
    }

    fn cycle_audio_channel_gain(&mut self, channel: PaulaChannel) -> f32 {
        let controls = self.runtime.audio_controls();
        let next = next_audio_gain(controls.channel(channel).gain());
        self.runtime.set_audio_channel_gain(channel, next);
        next
    }

    fn reset_audio_controls(&mut self) {
        self.runtime.set_audio_controls(AudioControls::default());
    }
}

struct AmigaApp {
    runner: AmigaRunner,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, &'static str>,
    pressed_buttons: HashMap<KeyCode, HostControl>,
    pressed_mouse_buttons: HashMap<MouseButton, &'static str>,
    last_cursor_position: Option<(f64, f64)>,
    keyboard_joystick: bool,
    gamepads: NativeGamepadInput,
    window: Option<std::sync::Arc<Window>>,
    video: Option<WgpuVideoPresenter>,
    presentation: PresentationProfile,
    fatal_error: Option<AppError>,
}

impl AmigaApp {
    fn new(runner: AmigaRunner, scale: u32, video: VideoFilter) -> Result<Self, AppError> {
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
            pressed_buttons: HashMap::new(),
            pressed_mouse_buttons: HashMap::new(),
            last_cursor_position: None,
            keyboard_joystick: false,
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

        let logical_width = f64::from(DISPLAY_WIDTH.saturating_mul(self.scale));
        let logical_height = f64::from(DISPLAY_HEIGHT.saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(self.window_title())
            .with_inner_size(LogicalSize::new(logical_width, logical_height))
            .with_min_inner_size(LogicalSize::new(
                f64::from(DISPLAY_WIDTH),
                f64::from(DISPLAY_HEIGHT),
            ));
        let window = std::sync::Arc::new(event_loop.create_window(attributes)?);
        let video = WgpuVideoPresenter::new(window.clone(), DISPLAY_WIDTH, DISPLAY_HEIGHT)?;

        self.window = Some(window);
        self.video = Some(video);
        self.next_slice_at = Instant::now();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn window_title(&self) -> String {
        let mut title = WINDOW_TITLE.to_owned();
        if self.keyboard_joystick {
            title.push_str(" | joy1 keys");
        }
        title
    }

    fn advance_machine(&mut self) -> Result<bool, AppError> {
        self.gamepads
            .drain_events(&AMIGA_JOYSTICK_MAP, &mut self.pending_inputs);

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
        if self.keyboard_joystick && self.queue_joystick_key_state(code, pressed) {
            return;
        }

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

    fn queue_joystick_key_state(&mut self, code: KeyCode, pressed: bool) -> bool {
        let Some(control) = map_amiga_joystick_key(code) else {
            return false;
        };

        if pressed {
            if self.pressed_buttons.contains_key(&code) {
                return true;
            }
            self.pressed_buttons.insert(code, control);
            if let Some(input) = AMIGA_JOYSTICK_MAP.event(control, true) {
                self.pending_inputs.push(input);
            }
        } else if let Some(control) = self.pressed_buttons.remove(&code)
            && let Some(input) = AMIGA_JOYSTICK_MAP.event(control, false)
        {
            self.pending_inputs.push(input);
        }
        self.next_slice_at = Instant::now();
        true
    }

    fn set_keyboard_joystick(&mut self, enabled: bool) {
        if self.keyboard_joystick == enabled {
            return;
        }
        self.release_all_inputs();
        self.keyboard_joystick = enabled;
        self.next_slice_at = Instant::now();
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

    fn release_all_inputs(&mut self) {
        self.release_all_keys();
        self.release_all_buttons();
        self.release_all_mouse_buttons();
        self.last_cursor_position = None;
        self.next_slice_at = Instant::now();
    }

    fn release_all_keys(&mut self) {
        let keys = std::mem::take(&mut self.pressed_keys);
        for name in keys.into_values() {
            self.pending_inputs.push(key_event(name, false));
        }
    }

    fn release_all_buttons(&mut self) {
        let buttons = std::mem::take(&mut self.pressed_buttons);
        for control in buttons.into_values() {
            if let Some(input) = AMIGA_JOYSTICK_MAP.event(control, false) {
                self.pending_inputs.push(input);
            }
        }
    }

    fn release_all_mouse_buttons(&mut self) {
        let buttons = std::mem::take(&mut self.pressed_mouse_buttons);
        for name in buttons.into_values() {
            self.pending_inputs.push(pointer_button_event(name, false));
        }
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
                    | KeyCode::PageUp
                    | KeyCode::Numpad0
                    | KeyCode::Numpad1
                    | KeyCode::Numpad2
                    | KeyCode::Numpad3
                    | KeyCode::Numpad4
                    | KeyCode::Numpad5
                    | KeyCode::Numpad6
                    | KeyCode::Numpad7
                    | KeyCode::Numpad8
            );
        }

        match code {
            KeyCode::Escape => {
                event_loop.exit();
                true
            }
            KeyCode::F12 => {
                self.release_all_inputs();
                if let Err(err) = self.runner.reset() {
                    self.fail(event_loop, err);
                }
                true
            }
            KeyCode::PageUp => {
                self.set_keyboard_joystick(!self.keyboard_joystick);
                eprintln!(
                    "input: keyboard joystick {}",
                    if self.keyboard_joystick {
                        "enabled on port 2"
                    } else {
                        "disabled"
                    }
                );
                if let Some(window) = &self.window {
                    window.set_title(&self.window_title());
                    window.request_redraw();
                }
                true
            }
            KeyCode::Numpad0 => {
                self.runner.reset_audio_controls();
                eprintln!("audio: reset Paula channel controls");
                true
            }
            KeyCode::Numpad1 => self.toggle_audio_channel_shortcut(PaulaChannel::Channel0),
            KeyCode::Numpad2 => self.toggle_audio_channel_shortcut(PaulaChannel::Channel1),
            KeyCode::Numpad3 => self.toggle_audio_channel_shortcut(PaulaChannel::Channel2),
            KeyCode::Numpad4 => self.toggle_audio_channel_shortcut(PaulaChannel::Channel3),
            KeyCode::Numpad5 => self.cycle_audio_channel_gain_shortcut(PaulaChannel::Channel0),
            KeyCode::Numpad6 => self.cycle_audio_channel_gain_shortcut(PaulaChannel::Channel1),
            KeyCode::Numpad7 => self.cycle_audio_channel_gain_shortcut(PaulaChannel::Channel2),
            KeyCode::Numpad8 => self.cycle_audio_channel_gain_shortcut(PaulaChannel::Channel3),
            _ => false,
        }
    }

    fn toggle_audio_channel_shortcut(&mut self, channel: PaulaChannel) -> bool {
        let enabled = self.runner.toggle_audio_channel(channel);
        eprintln!(
            "audio: {} {}",
            channel.label(),
            if enabled { "enabled" } else { "muted" }
        );
        true
    }

    fn cycle_audio_channel_gain_shortcut(&mut self, channel: PaulaChannel) -> bool {
        let gain = self.runner.cycle_audio_channel_gain(channel);
        eprintln!("audio: {} gain {:.0}%", channel.label(), gain * 100.0);
        true
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
            WindowEvent::Focused(false) => self.release_all_inputs(),
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

pub fn run(cli: Cli) -> Result<(), AppError> {
    println!(
        "Controls: Esc quit, F12 reset, mouse port 1, gamepad joystick port 2, Page Up toggles joystick arrows/space, A-Z/0-9/Space/Enter/Tab/Backspace keyboard, numpad 1-4 toggle Paula channels, numpad 5-8 cycle channel gain, numpad 0 reset audio."
    );

    let runner = AmigaRunner::from_cli(&cli)?;
    let mut app = AmigaApp::new(runner, cli.scale, cli.video)?;
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    if let Some(err) = app.take_error() {
        return Err(err);
    }

    Ok(())
}

pub fn parse_cli<I>(args: I) -> Cli
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
            "--video" => {
                cli.video = parse_video_arg(&next_arg(&mut iter, "--video"));
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

fn parse_video_arg(video: &str) -> VideoFilter {
    video
        .parse()
        .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"))
}

fn firmware_id_for_model_arg(model: ModelArg) -> &'static str {
    match model {
        ModelArg::A1000 => A1000_BOOTSTRAP_ID,
        ModelArg::A500
        | ModelArg::A500A501
        | ModelArg::A500Plus
        | ModelArg::A500Maxed
        | ModelArg::A600
        | ModelArg::A1200
        | ModelArg::A2000 => KICKSTART_ID,
    }
}

fn resolve_firmware_path(cli: &Cli) -> Result<PathBuf, String> {
    find_rom_path(cli.model, cli.rom_dir.as_deref(), cli.kickstart.as_deref())
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

fn map_amiga_joystick_key(code: KeyCode) -> Option<HostControl> {
    Some(match code {
        KeyCode::ArrowUp => HostControl::Up,
        KeyCode::ArrowDown => HostControl::Down,
        KeyCode::ArrowLeft => HostControl::Left,
        KeyCode::ArrowRight => HostControl::Right,
        KeyCode::Space => HostControl::South,
        _ => return None,
    })
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
    fn maps_host_keys_to_joystick_mode_controls() {
        assert_eq!(
            map_amiga_joystick_key(KeyCode::ArrowUp),
            Some(HostControl::Up)
        );
        assert_eq!(
            map_amiga_joystick_key(KeyCode::Space),
            Some(HostControl::South)
        );
        assert_eq!(map_amiga_key(KeyCode::PageUp), None);
    }

    #[test]
    fn audio_gain_cycles_through_debug_levels() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}
