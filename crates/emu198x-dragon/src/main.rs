//! `emu198x-dragon` — minimal native Dragon 32 verifier shell.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu198x_native_video::{
    PresentationProfile, VideoFilter, VideoPresenterError, WgpuVideoPresenter,
};
use emu198x_shell::{
    ButtonInputMap, ButtonTarget, CapturedFrame, FirmwareImage, FirmwareSet, HostControl, HostIo,
    InputEvent, LatestFrameCapture, MachineCore, MachineError, MediaImage, MediaKind, MediaSet,
    NativeAudioError, NativeAudioOutput, NativeGamepadInput, NullTraceSink, ResetKind, RunResult,
    read_firmware_asset, read_media_asset,
};
use emu198x_shell::{HeadlessSession, SessionError};
use motorola_vdg_6847::{VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT, VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const DEFAULT_SCALE: u32 = 2;
const DRAGON_CPU_HZ: u64 = 894_886;
const DRAGON_FRAME_HZ: u64 = 50;
const DRAGON_FRAME_CYCLES: u64 = DRAGON_CPU_HZ / DRAGON_FRAME_HZ;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;
const AUTOLOAD_BOOT_FRAMES: u32 = 100;
const AUTOLOAD_KEY_EDGE_FRAMES: u32 = 4;
const AUTOLOAD_START_SETTLE_FRAMES: u32 = 60;
const WINDOW_TITLE: &str = "Emu198x Dragon 32";
const DRAGON_GAMEPAD_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
    (HostControl::Start, ButtonTarget::new(1, "enter")),
    (HostControl::Select, ButtonTarget::new(1, "clear")),
]);

const USAGE: &str = "\
Usage: emu198x-dragon [OPTIONS] --rom PATH

Options:
    --rom PATH       Dragon 32 BASIC ROM, or zip containing one ROM/bin candidate
    --tape PATH      Dragon CAS tape image, or zip containing one .cas member
    --cart PATH      Dragon cartridge ROM/DGN image, or zip containing one cartridge member
    --snapshot PATH  PC-Dragon PAK snapshot, or zip containing one .pak member
    --autoload       type CLOAD/CLOADM, wait for load, then type RUN/EXEC
    --scale N        integer window scale, default 2
    --video MODE     raw | lcd | crt [default: crt]
    --help, -h       show this help

Controls:
    Esc              quit
    F12              hard reset
    A-Z, 0-9         Dragon keyboard keys
    @ : ; , - . /    Dragon punctuation keys
    ! \" # $ % & ' ( ) * + < = > ?
                     Dragon shifted symbols
    Arrows           Dragon arrow keys
    Enter            Dragon Enter
    Space            Dragon Space
    Shift            Dragon Shift
    Backspace        Dragon Clear
    F1               Dragon Break
    Gamepad D-pad/left stick
                     Dragon joystick 1 axes
    Gamepad South/East
                     Dragon joystick 1 fire
";

#[derive(Debug, PartialEq, Eq)]
struct Cli {
    rom: Option<PathBuf>,
    tape: Option<PathBuf>,
    cart: Option<PathBuf>,
    snapshot: Option<PathBuf>,
    autoload: bool,
    scale: u32,
    video: VideoFilter,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            rom: None,
            tape: None,
            cart: None,
            snapshot: None,
            autoload: false,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Crt,
        }
    }
}

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error(transparent)]
    Video(#[from] VideoPresenterError),

    #[error(transparent)]
    Audio(#[from] NativeAudioError),

    #[error(transparent)]
    EventLoop(#[from] EventLoopError),

    #[error(transparent)]
    Os(#[from] OsError),

    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },

    #[error("{reason}")]
    Setup { reason: String },
}

struct DragonRunner {
    runtime: DragonRuntime,
    audio_output: NativeAudioOutput,
    frame_capture: LatestFrameCapture,
    last_run_result: Option<RunResult>,
    native_frame_ticks: u64,
}

impl DragonRunner {
    fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        let runtime = runtime_from_cli(cli)?;
        let mut runner = Self {
            runtime,
            audio_output: NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?,
            frame_capture: LatestFrameCapture::default(),
            last_run_result: None,
            native_frame_ticks: DRAGON_FRAME_CYCLES,
        };
        runner.run_frame(&[])?;
        Ok(runner)
    }

    fn reset(&mut self) -> Result<(), AppError> {
        self.runtime.reset(ResetKind::Hard);
        self.audio_output.clear();
        self.last_run_result = None;
        self.frame_capture = LatestFrameCapture::default();
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
}

fn runtime_from_cli(cli: &Cli) -> Result<DragonRuntime, AppError> {
    if cli.autoload && cli.tape.is_none() {
        return Err(AppError::Setup {
            reason: "--autoload requires --tape PATH".to_owned(),
        });
    }

    let rom = cli.rom.as_ref().ok_or_else(|| AppError::Setup {
        reason: "provide --rom PATH".to_owned(),
    })?;
    let loaded = read_firmware_asset(rom).map_err(|err| AppError::Setup {
        reason: format!("failed to load Dragon ROM {}: {err}", rom.display()),
    })?;
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("dragon32-basic-rom", &loaded.bytes));
    let runtime = DragonRuntime::from_firmware(Model::Dragon32Pal, &firmware)?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    );

    if let Some(tape) = &cli.tape {
        let loaded = read_media_asset(tape, MediaKind::Tape).map_err(|err| AppError::Setup {
            reason: format!("failed to load Dragon tape {}: {err}", tape.display()),
        })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
        session.load_media(&media)?;
        if let Some(summary) = session.machine().tape_summary() {
            let name = summary.header_name.as_deref().unwrap_or("<no header>");
            println!(
                "Loaded tape: {name}, {} CAS blocks, checksums {}",
                summary.blocks,
                if summary.checksums_valid {
                    "valid"
                } else {
                    "invalid"
                }
            );
        }
    }

    if let Some(cart) = &cli.cart {
        let loaded =
            read_media_asset(cart, MediaKind::Cartridge).map_err(|err| AppError::Setup {
                reason: format!("failed to load Dragon cartridge {}: {err}", cart.display()),
            })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "cartridge-1",
            MediaKind::Cartridge,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        println!("Loaded cartridge: {} bytes", loaded.bytes.len());
    }

    if let Some(snapshot) = &cli.snapshot {
        let loaded =
            read_media_asset(snapshot, MediaKind::Snapshot).map_err(|err| AppError::Setup {
                reason: format!(
                    "failed to load Dragon snapshot {}: {err}",
                    snapshot.display()
                ),
            })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "snapshot-1",
            MediaKind::Snapshot,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        println!("Loaded snapshot: {} bytes", loaded.bytes.len());
    }

    if cli.autoload {
        autoload_tape(&mut session)?;
    }

    Ok(session.into_machine())
}

struct DragonApp {
    runner: DragonRunner,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<PhysicalKey, Vec<String>>,
    gamepads: NativeGamepadInput,
    window: Option<Arc<Window>>,
    video: Option<WgpuVideoPresenter>,
    presentation: PresentationProfile,
    fatal_error: Option<AppError>,
}

impl DragonApp {
    fn new(runner: DragonRunner, scale: u32, video: VideoFilter) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        Ok(Self {
            runner,
            scale,
            slice_ticks: DRAGON_FRAME_CYCLES.div_ceil(u64::from(INPUT_SLICES_PER_FRAME)),
            slice_duration: Duration::from_secs_f64(
                1.0 / DRAGON_FRAME_HZ as f64 / f64::from(INPUT_SLICES_PER_FRAME),
            ),
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

        let frame_width = VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH as u32;
        let frame_height = VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT as u32;
        let attributes = WindowAttributes::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(
                f64::from(frame_width.saturating_mul(self.scale)),
                f64::from(frame_height.saturating_mul(self.scale)),
            ))
            .with_min_inner_size(LogicalSize::new(
                f64::from(frame_width),
                f64::from(frame_height),
            ));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let video = WgpuVideoPresenter::new(window.clone(), frame_width, frame_height)?;

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
            .drain_events(&DRAGON_GAMEPAD_MAP, &mut self.pending_inputs);

        let now = Instant::now();
        if now < self.next_slice_at {
            return Ok(false);
        }

        let max_catch_up_slices = MAX_CATCH_UP_FRAMES.saturating_mul(INPUT_SLICES_PER_FRAME);
        let mut ran_slices = 0;
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

    fn queue_key_state(&mut self, physical_key: PhysicalKey, logical_key: &Key, pressed: bool) {
        if pressed {
            if self.pressed_keys.contains_key(&physical_key) {
                return;
            }
            let Some(names) = map_dragon_keys(logical_key, physical_key) else {
                return;
            };
            for name in &names {
                self.pending_inputs.push(InputEvent::Key {
                    name: name.clone().into(),
                    pressed: true,
                });
            }
            self.pressed_keys.insert(physical_key, names);
            self.next_slice_at = Instant::now();
        } else if let Some(names) = self.pressed_keys.remove(&physical_key) {
            for name in names.into_iter().rev() {
                self.pending_inputs.push(InputEvent::Key {
                    name: name.into(),
                    pressed: false,
                });
            }
            self.next_slice_at = Instant::now();
        }
    }

    fn release_all_keys(&mut self) {
        let keys = std::mem::take(&mut self.pressed_keys);
        for names in keys.into_values() {
            for name in names.into_iter().rev() {
                self.pending_inputs.push(InputEvent::Key {
                    name: name.into(),
                    pressed: false,
                });
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

impl ApplicationHandler for DragonApp {
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
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key
                    && self.handle_shortcut(event_loop, code, pressed)
                {
                    return;
                }
                self.queue_key_state(event.physical_key, &event.logical_key, pressed);
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
        "Controls: Esc quit, F12 reset, Dragon keys: A-Z, 0-9, punctuation, shifted symbols, arrows, Enter, Clear, Break, Shift, Space."
    );

    let runner = DragonRunner::from_cli(&cli)?;
    let mut app = DragonApp::new(runner, cli.scale, cli.video)?;
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
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--snapshot" => cli.snapshot = Some(PathBuf::from(next_arg(&mut iter, "--snapshot"))),
            "--autoload" => cli.autoload = true,
            "--scale" => {
                cli.scale = next_arg(&mut iter, "--scale")
                    .parse()
                    .unwrap_or_else(|_| die("--scale requires a positive integer"));
            }
            "--video" => {
                cli.video = next_arg(&mut iter, "--video")
                    .parse()
                    .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragonAutoloadKind {
    Basic,
    MachineCode,
}

impl DragonAutoloadKind {
    fn load_command(self) -> &'static str {
        match self {
            Self::Basic => "CLOAD",
            Self::MachineCode => "CLOADM",
        }
    }

    fn start_command(self) -> &'static str {
        match self {
            Self::Basic => "RUN",
            Self::MachineCode => "EXEC",
        }
    }
}

fn autoload_kind(runtime: &DragonRuntime) -> Result<DragonAutoloadKind, AppError> {
    let summary = runtime.tape_summary().ok_or_else(|| AppError::Setup {
        reason: "--autoload requires a mounted CAS tape".to_owned(),
    })?;
    match summary.header_file_type {
        Some("basic") => Ok(DragonAutoloadKind::Basic),
        Some("machine-code") => Ok(DragonAutoloadKind::MachineCode),
        Some(file_type) => Err(AppError::Setup {
            reason: format!("--autoload does not support Dragon CAS file type {file_type}"),
        }),
        None => Err(AppError::Setup {
            reason: "--autoload requires a Dragon CAS namefile header".to_owned(),
        }),
    }
}

fn autoload_tape(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<(), AppError> {
    let kind = autoload_kind(session.machine())?;
    let boot = session.wait_for_boot(AUTOLOAD_BOOT_FRAMES)?;

    println!("Autoload: typing {}", kind.load_command());
    type_basic_command(session, kind.load_command())?;
    wait_for_tape_position_above(session, 0, 180)?;
    let load_wait_frames =
        load_wait_frame_budget(session.machine().machine().cassette_len_bits() as u64);
    wait_for_tape_load_stop(session, load_wait_frames)?;

    println!("Autoload: typing {}", kind.start_command());
    type_basic_command(session, kind.start_command())?;
    session.run_frames(AUTOLOAD_START_SETTLE_FRAMES)?;
    println!("Autoload complete after BASIC boot: {}", boot.reason);
    Ok(())
}

fn load_wait_frame_budget(tape_length_bits: u64) -> u32 {
    let scaled = tape_length_bits / 16;
    u32::try_from(scaled.clamp(4_500, 20_000)).map_or(20_000, |frames| frames)
}

fn wait_for_tape_position_above(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    position_bits: usize,
    max_frames: u32,
) -> Result<(), AppError> {
    for _ in 0..=max_frames {
        if session.machine().machine().cassette_position_bits() > position_bits {
            return Ok(());
        }
        session.run_frames(1)?;
    }
    Err(AppError::Setup {
        reason: format!("Dragon autoload did not start consuming tape within {max_frames} frames"),
    })
}

fn wait_for_tape_load_stop(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    max_frames: u32,
) -> Result<(), AppError> {
    for _ in 0..=max_frames {
        let machine = session.machine().machine();
        if !machine.cassette_motor_on() || machine.cassette_finished() {
            return Ok(());
        }
        session.run_frames(1)?;
    }
    Err(AppError::Setup {
        reason: format!("Dragon autoload did not finish loading within {max_frames} frames"),
    })
}

fn type_basic_command(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    command: &str,
) -> Result<(), AppError> {
    for ch in command.chars() {
        tap_key(session, &ch.to_ascii_lowercase().to_string())?;
    }
    tap_key(session, "enter")
}

fn tap_key(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &str,
) -> Result<(), AppError> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session.run_frames(AUTOLOAD_KEY_EDGE_FRAMES)?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session.run_frames(AUTOLOAD_KEY_EDGE_FRAMES)?;
    Ok(())
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

fn map_dragon_keys(logical_key: &Key, physical_key: PhysicalKey) -> Option<Vec<String>> {
    if let Some(names) = map_dragon_logical_keys(logical_key) {
        return Some(names);
    }

    let PhysicalKey::Code(code) = physical_key else {
        return None;
    };
    map_dragon_physical_fallback(code).map(|name| vec![name.to_owned()])
}

fn map_dragon_logical_keys(key: &Key) -> Option<Vec<String>> {
    match key {
        Key::Character(text) => map_dragon_character(text.as_str()).map(labels_to_names),
        Key::Named(named) => map_dragon_named_key(*named).map(|name| vec![name.to_owned()]),
        Key::Unidentified(_) | Key::Dead(_) => None,
    }
}

fn labels_to_names(labels: Vec<&'static str>) -> Vec<String> {
    labels.into_iter().map(str::to_owned).collect()
}

fn map_dragon_character(text: &str) -> Option<Vec<&'static str>> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    Some(match ch {
        '0' => vec!["0"],
        '1' => vec!["1"],
        '2' => vec!["2"],
        '3' => vec!["3"],
        '4' => vec!["4"],
        '5' => vec!["5"],
        '6' => vec!["6"],
        '7' => vec!["7"],
        '8' => vec!["8"],
        '9' => vec!["9"],
        'a' | 'A' => vec!["a"],
        'b' | 'B' => vec!["b"],
        'c' | 'C' => vec!["c"],
        'd' | 'D' => vec!["d"],
        'e' | 'E' => vec!["e"],
        'f' | 'F' => vec!["f"],
        'g' | 'G' => vec!["g"],
        'h' | 'H' => vec!["h"],
        'i' | 'I' => vec!["i"],
        'j' | 'J' => vec!["j"],
        'k' | 'K' => vec!["k"],
        'l' | 'L' => vec!["l"],
        'm' | 'M' => vec!["m"],
        'n' | 'N' => vec!["n"],
        'o' | 'O' => vec!["o"],
        'p' | 'P' => vec!["p"],
        'q' | 'Q' => vec!["q"],
        'r' | 'R' => vec!["r"],
        's' | 'S' => vec!["s"],
        't' | 'T' => vec!["t"],
        'u' | 'U' => vec!["u"],
        'v' | 'V' => vec!["v"],
        'w' | 'W' => vec!["w"],
        'x' | 'X' => vec!["x"],
        'y' | 'Y' => vec!["y"],
        'z' | 'Z' => vec!["z"],
        ' ' => vec!["space"],
        '@' => vec!["@"],
        ':' => vec![":"],
        ';' => vec![";"],
        ',' => vec![","],
        '-' => vec!["-"],
        '.' => vec!["."],
        '/' => vec!["/"],
        '!' => vec!["shift", "1"],
        '"' => vec!["shift", "2"],
        '#' => vec!["shift", "3"],
        '$' => vec!["shift", "4"],
        '%' => vec!["shift", "5"],
        '&' => vec!["shift", "6"],
        '\'' => vec!["shift", "7"],
        '(' => vec!["shift", "8"],
        ')' => vec!["shift", "9"],
        '*' => vec!["shift", ":"],
        '+' => vec!["shift", ";"],
        '<' => vec!["shift", ","],
        '=' => vec!["shift", "-"],
        '>' => vec!["shift", "."],
        '?' => vec!["shift", "/"],
        _ => return None,
    })
}

fn map_dragon_named_key(key: NamedKey) -> Option<&'static str> {
    Some(match key {
        NamedKey::ArrowUp => "up",
        NamedKey::ArrowDown => "down",
        NamedKey::ArrowLeft => "left",
        NamedKey::ArrowRight => "right",
        NamedKey::Space => "space",
        NamedKey::Enter => "enter",
        NamedKey::Backspace | NamedKey::Clear => "clear",
        NamedKey::F1 => "break",
        NamedKey::Shift => "shift",
        _ => return None,
    })
}

fn map_dragon_physical_fallback(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::Digit0 | KeyCode::Numpad0 => "0",
        KeyCode::Digit1 | KeyCode::Numpad1 => "1",
        KeyCode::Digit2 | KeyCode::Numpad2 => "2",
        KeyCode::Digit3 | KeyCode::Numpad3 => "3",
        KeyCode::Digit4 | KeyCode::Numpad4 => "4",
        KeyCode::Digit5 | KeyCode::Numpad5 => "5",
        KeyCode::Digit6 | KeyCode::Numpad6 => "6",
        KeyCode::Digit7 | KeyCode::Numpad7 => "7",
        KeyCode::Digit8 | KeyCode::Numpad8 => "8",
        KeyCode::Digit9 | KeyCode::Numpad9 => "9",
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
        KeyCode::ArrowUp => "up",
        KeyCode::ArrowDown => "down",
        KeyCode::ArrowLeft => "left",
        KeyCode::ArrowRight => "right",
        KeyCode::Space => "space",
        KeyCode::Enter | KeyCode::NumpadEnter => "enter",
        KeyCode::Backspace | KeyCode::Delete | KeyCode::NumpadBackspace | KeyCode::NumpadClear => {
            "clear"
        }
        KeyCode::F1 => "break",
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "shift",
        KeyCode::Comma | KeyCode::NumpadComma => ",",
        KeyCode::Minus | KeyCode::NumpadSubtract => "-",
        KeyCode::Period | KeyCode::NumpadDecimal => ".",
        KeyCode::Slash | KeyCode::NumpadDivide => "/",
        KeyCode::Semicolon => ";",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use machine_dragon_32::DragonKey;
    use std::env;

    use super::*;

    #[test]
    fn parse_cli_accepts_positional_rom_and_video() {
        let cli = parse_cli([
            "--scale".to_owned(),
            "3".to_owned(),
            "--video".to_owned(),
            "raw".to_owned(),
            "dragon32.rom".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("dragon32.rom")),
                tape: None,
                cart: None,
                snapshot: None,
                autoload: false,
                scale: 3,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--tape".to_owned(),
            "program.cas".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.tape, Some(PathBuf::from("program.cas")));
        assert!(!cli.autoload);
    }

    #[test]
    fn parse_cli_accepts_cart_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cart".to_owned(),
            "game.dgn".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.cart, Some(PathBuf::from("game.dgn")));
    }

    #[test]
    fn parse_cli_accepts_snapshot_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--snapshot".to_owned(),
            "game.pak".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.snapshot, Some(PathBuf::from("game.pak")));
    }

    #[test]
    fn parse_cli_accepts_autoload_flag() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--tape".to_owned(),
            "program.cas".to_owned(),
            "--autoload".to_owned(),
        ]);

        assert_eq!(cli.rom, Some(PathBuf::from("dragon32.rom")));
        assert_eq!(cli.tape, Some(PathBuf::from("program.cas")));
        assert!(cli.autoload);
    }

    #[test]
    fn autoload_kind_commands_match_dragon_basic() {
        assert_eq!(DragonAutoloadKind::Basic.load_command(), "CLOAD");
        assert_eq!(DragonAutoloadKind::Basic.start_command(), "RUN");
        assert_eq!(DragonAutoloadKind::MachineCode.load_command(), "CLOADM");
        assert_eq!(DragonAutoloadKind::MachineCode.start_command(), "EXEC");
    }

    #[test]
    fn gamepad_map_targets_dragon_joystick_fire() {
        assert_eq!(
            DRAGON_GAMEPAD_MAP.event(HostControl::South, true),
            Some(InputEvent::Button {
                port: 1,
                name: "fire".into(),
                pressed: true,
            })
        );
        assert_eq!(
            DRAGON_GAMEPAD_MAP.event(HostControl::Right, true),
            Some(InputEvent::Button {
                port: 1,
                name: "right".into(),
                pressed: true,
            })
        );
    }

    #[test]
    fn native_autoload_runs_real_textstar_when_available() {
        let Some(rom) = dragon32_rom_path() else {
            eprintln!("skipping native Dragon autoload smoke: missing Dragon 32 ROM");
            return;
        };
        let Some(tape) = dragon_textstar_cas_path() else {
            eprintln!("skipping native Dragon autoload smoke: missing Textstar CAS");
            return;
        };

        let runtime = runtime_from_cli(&Cli {
            rom: Some(rom),
            tape: Some(tape),
            cart: None,
            snapshot: None,
            autoload: true,
            scale: DEFAULT_SCALE,
            video: VideoFilter::Crt,
        })
        .expect("native Dragon autoload should run Textstar");

        assert!(
            runtime.machine().cassette_position_bits() > 0,
            "autoload should have consumed tape data"
        );
        assert!(runtime.time().0 > 0, "autoload should have advanced time");
    }

    #[test]
    fn maps_common_logical_keys_to_dragon_labels() {
        assert_eq!(
            map_dragon_keys(
                &Key::Character("a".into()),
                PhysicalKey::Code(KeyCode::KeyA)
            ),
            Some(vec!["a".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Character("A".into()),
                PhysicalKey::Code(KeyCode::KeyA)
            ),
            Some(vec!["a".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Character("1".into()),
                PhysicalKey::Code(KeyCode::Digit1)
            ),
            Some(vec!["1".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Character("@".into()),
                PhysicalKey::Code(KeyCode::Quote)
            ),
            Some(vec!["@".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Named(NamedKey::ArrowLeft),
                PhysicalKey::Code(KeyCode::ArrowLeft)
            ),
            Some(vec!["left".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Named(NamedKey::Enter),
                PhysicalKey::Code(KeyCode::Enter)
            ),
            Some(vec!["enter".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Named(NamedKey::Backspace),
                PhysicalKey::Code(KeyCode::Backspace)
            ),
            Some(vec!["clear".to_owned()])
        );
    }

    #[test]
    fn maps_every_dragon_key_to_a_host_event() {
        for key in DragonKey::ALL {
            let (logical, physical) = host_event_for_dragon_key(key);
            assert_eq!(
                map_dragon_keys(&logical, physical),
                Some(vec![key.label().to_owned()]),
                "missing native host mapping for Dragon key {key:?}",
            );
        }
    }

    #[test]
    fn physical_fallback_covers_numpad_and_platform_keys() {
        assert_eq!(
            map_dragon_keys(
                &Key::Unidentified(winit::keyboard::NativeKey::Unidentified),
                PhysicalKey::Code(KeyCode::Numpad1),
            ),
            Some(vec!["1".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Unidentified(winit::keyboard::NativeKey::Unidentified),
                PhysicalKey::Code(KeyCode::NumpadEnter),
            ),
            Some(vec!["enter".to_owned()])
        );
        assert_eq!(
            map_dragon_keys(
                &Key::Unidentified(winit::keyboard::NativeKey::Unidentified),
                PhysicalKey::Code(KeyCode::Delete),
            ),
            Some(vec!["clear".to_owned()])
        );
    }

    #[test]
    fn maps_shifted_printable_symbols_to_dragon_shift_combos() {
        let cases = [
            ("!", &["shift", "1"][..]),
            ("\"", &["shift", "2"]),
            ("#", &["shift", "3"]),
            ("$", &["shift", "4"]),
            ("%", &["shift", "5"]),
            ("&", &["shift", "6"]),
            ("'", &["shift", "7"]),
            ("(", &["shift", "8"]),
            (")", &["shift", "9"]),
            ("*", &["shift", ":"]),
            ("+", &["shift", ";"]),
            ("<", &["shift", ","]),
            ("=", &["shift", "-"]),
            (">", &["shift", "."]),
            ("?", &["shift", "/"]),
        ];

        for (text, expected) in cases {
            assert_eq!(
                map_dragon_keys(
                    &Key::Character(text.into()),
                    PhysicalKey::Code(KeyCode::ShiftLeft),
                ),
                Some(expected.iter().map(ToString::to_string).collect()),
                "missing shifted symbol mapping for {text:?}",
            );
        }
    }

    fn host_event_for_dragon_key(key: DragonKey) -> (Key, PhysicalKey) {
        match key {
            DragonKey::Digit0 => character("0", KeyCode::Digit0),
            DragonKey::Digit1 => character("1", KeyCode::Digit1),
            DragonKey::Digit2 => character("2", KeyCode::Digit2),
            DragonKey::Digit3 => character("3", KeyCode::Digit3),
            DragonKey::Digit4 => character("4", KeyCode::Digit4),
            DragonKey::Digit5 => character("5", KeyCode::Digit5),
            DragonKey::Digit6 => character("6", KeyCode::Digit6),
            DragonKey::Digit7 => character("7", KeyCode::Digit7),
            DragonKey::Digit8 => character("8", KeyCode::Digit8),
            DragonKey::Digit9 => character("9", KeyCode::Digit9),
            DragonKey::Colon => character(":", KeyCode::Semicolon),
            DragonKey::Semicolon => character(";", KeyCode::Semicolon),
            DragonKey::Comma => character(",", KeyCode::Comma),
            DragonKey::Minus => character("-", KeyCode::Minus),
            DragonKey::Period => character(".", KeyCode::Period),
            DragonKey::Slash => character("/", KeyCode::Slash),
            DragonKey::At => character("@", KeyCode::Quote),
            DragonKey::A => character("a", KeyCode::KeyA),
            DragonKey::B => character("b", KeyCode::KeyB),
            DragonKey::C => character("c", KeyCode::KeyC),
            DragonKey::D => character("d", KeyCode::KeyD),
            DragonKey::E => character("e", KeyCode::KeyE),
            DragonKey::F => character("f", KeyCode::KeyF),
            DragonKey::G => character("g", KeyCode::KeyG),
            DragonKey::H => character("h", KeyCode::KeyH),
            DragonKey::I => character("i", KeyCode::KeyI),
            DragonKey::J => character("j", KeyCode::KeyJ),
            DragonKey::K => character("k", KeyCode::KeyK),
            DragonKey::L => character("l", KeyCode::KeyL),
            DragonKey::M => character("m", KeyCode::KeyM),
            DragonKey::N => character("n", KeyCode::KeyN),
            DragonKey::O => character("o", KeyCode::KeyO),
            DragonKey::P => character("p", KeyCode::KeyP),
            DragonKey::Q => character("q", KeyCode::KeyQ),
            DragonKey::R => character("r", KeyCode::KeyR),
            DragonKey::S => character("s", KeyCode::KeyS),
            DragonKey::T => character("t", KeyCode::KeyT),
            DragonKey::U => character("u", KeyCode::KeyU),
            DragonKey::V => character("v", KeyCode::KeyV),
            DragonKey::W => character("w", KeyCode::KeyW),
            DragonKey::X => character("x", KeyCode::KeyX),
            DragonKey::Y => character("y", KeyCode::KeyY),
            DragonKey::Z => character("z", KeyCode::KeyZ),
            DragonKey::Up => named(NamedKey::ArrowUp, KeyCode::ArrowUp),
            DragonKey::Down => named(NamedKey::ArrowDown, KeyCode::ArrowDown),
            DragonKey::Left => named(NamedKey::ArrowLeft, KeyCode::ArrowLeft),
            DragonKey::Right => named(NamedKey::ArrowRight, KeyCode::ArrowRight),
            DragonKey::Space => named(NamedKey::Space, KeyCode::Space),
            DragonKey::Enter => named(NamedKey::Enter, KeyCode::Enter),
            DragonKey::Clear => named(NamedKey::Backspace, KeyCode::Backspace),
            DragonKey::Break => named(NamedKey::F1, KeyCode::F1),
            DragonKey::Shift => named(NamedKey::Shift, KeyCode::ShiftLeft),
        }
    }

    fn character(text: &'static str, code: KeyCode) -> (Key, PhysicalKey) {
        (Key::Character(text.into()), PhysicalKey::Code(code))
    }

    fn named(key: NamedKey, code: KeyCode) -> (Key, PhysicalKey) {
        (Key::Named(key), PhysicalKey::Code(code))
    }

    fn dragon32_rom_path() -> Option<PathBuf> {
        if let Ok(path) = env::var("EMU198X_DRAGON32_ROM") {
            return existing_file(path);
        }
        existing_file(home_path(".emu198x/roms/dragon/dragon32.rom")?).or_else(|| {
            existing_file(
                home_path("Projects/Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Firmware/Dragon Data Dragon 32 BIOS (1982)(Dragon Data).zip")?,
            )
        })
    }

    fn dragon_textstar_cas_path() -> Option<PathBuf> {
        if let Ok(path) = env::var("EMU198X_DRAGON_TEXTSTAR_CAS") {
            return existing_file(path);
        }
        existing_file(home_path(
            "Projects/Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Applications/[CAS]/Textstar (1982)(Personal Software Services).zip",
        )?)
    }

    fn home_path(relative: &str) -> Option<PathBuf> {
        Some(PathBuf::from(env::var("HOME").ok()?).join(relative))
    }

    fn existing_file(path: impl Into<PathBuf>) -> Option<PathBuf> {
        let path = path.into();
        path.is_file().then_some(path)
    }
}
