//! `emu198x-c64` — minimal native Commodore 64 verification shell.
//!
//! This is intentionally narrow: one PAL/NTSC breadbin window, optional
//! startup snapshot/program/tape import, direct keyboard input, hard reset,
//! optional tape autoload, cycle-faithful tape turbo, and live audio/video
//! over the existing runtime. It does not introduce a parallel emulation stack
//! or fake media behavior.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common_commodore_c64::timing::{TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::query::query_value;
use emu198x_shell::{
    BootArtifacts, CapturedFrame, ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession,
    HostIo, InputEvent, LatestFrameCapture, MachineCore, MachineError, MediaImage, MediaKind,
    MediaSet, MediaTransportAction, MediaTransportCommand, NativeAudioError, NativeAudioOutput,
    NullTraceSink, PixelFormat, QueryError, QueryResult, ResetKind, RunResult,
    SessionQueryProvider, boot_machine, read_firmware_asset, read_media_asset, read_program_asset,
};
use pixels::{Pixels, SurfaceTexture, TextureError};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_DISK_AUTOLOAD_SLOT,
    DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, Model, autoload_basic_disk,
    autoload_basic_tape, file_loader::load_host_file,
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
const DRIVE1541_ID: &str = "commodore-1541-dos-rom";
const DEFAULT_SCALE: u32 = 2;
const DEFAULT_IMPORT_BOOT_FRAMES: u32 = 200;
const INPUT_SLICES_PER_FRAME: u32 = 8;
const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_TURBO_TAPE_FRAMES: u32 = 32;
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
    --disk PATH          insert one D64 image into drive-8 at startup
    --tape PATH          insert one TAP image into datasette slot at startup
    --autoload-disk      wait for READY. and type LOAD\"*\",8,1 for drive-8
    --autoload-tape      wait for READY., press SHIFT+RUN/STOP, and start tape-1
    --start-tape         start the inserted tape immediately at startup
    --turbo-tape         run unthrottled while the tape is playing
    --load-snapshot PATH restore a runtime snapshot before starting
    --scale N            integer window scale, default 2
    --help, -h           show this help

Controls:
    Esc                  quit
    F9                   start tape
    F10                  stop tape
    F11                  toggle tape turbo
    F12                  hard reset
    Arrow keys           C64 cursor keys
    F1-F8                C64 function keys
    Alt / Command        Commodore key
    Tab                  Run/Stop

Examples:
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --load demo.bas
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk game.d64
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk game.d64 --autoload-disk
    emu198x-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --tape game.tap --autoload-tape
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
    disk: Option<PathBuf>,
    tape: Option<PathBuf>,
    autoload_disk: bool,
    autoload_tape: bool,
    start_tape: bool,
    turbo_tape: bool,
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

    #[error(transparent)]
    Audio(#[from] NativeAudioError),

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
    audio_output: NativeAudioOutput,
    last_run_result: Option<RunResult>,
    native_frame_ticks: u64,
    frame_width: u32,
    frame_height: u32,
    title_base: String,
}

impl C64Runner {
    fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        if cli.autoload_disk && cli.autoload_tape {
            return Err(AppError::Setup {
                reason: "--autoload-disk conflicts with --autoload-tape".to_owned(),
            });
        }
        if cli.autoload_tape && cli.start_tape {
            return Err(AppError::Setup {
                reason: "--autoload-tape conflicts with --start-tape".to_owned(),
            });
        }
        if (cli.autoload_tape || cli.start_tape) && cli.tape.is_none() {
            return Err(AppError::Setup {
                reason: "--autoload-tape and --start-tape require --tape PATH".to_owned(),
            });
        }

        let machine = boot_runtime(cli).map_err(|reason| AppError::Setup { reason })?;
        let native_frame_ticks = cli.native_frame_ticks();
        let mut session = HeadlessSession::new_with_query_provider(
            machine,
            native_frame_ticks,
            C64SessionQueryProvider,
        );

        if let Some(path) = &cli.tape {
            let loaded =
                read_media_asset(path, MediaKind::Tape).map_err(|err| AppError::Setup {
                    reason: format!("failed to load tape asset {}: {err}", path.display()),
                })?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
            session.load_media(&media).map_err(|err| AppError::Setup {
                reason: format!("tape load failed: {err}"),
            })?;
        }

        if let Some(path) = &cli.disk {
            let loaded =
                read_media_asset(path, MediaKind::Disk).map_err(|err| AppError::Setup {
                    reason: format!("failed to load disk asset {}: {err}", path.display()),
                })?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new("drive-8", MediaKind::Disk, &loaded.bytes));
            session.load_media(&media).map_err(|err| AppError::Setup {
                reason: format!("disk load failed: {err}"),
            })?;
        }

        if cli.autoload_tape {
            autoload_basic_tape(
                &mut session,
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
            )
            .map_err(|err| AppError::Setup {
                reason: format!("tape autoload failed: {err}"),
            })?;
        } else if cli.autoload_disk {
            autoload_basic_disk(
                &mut session,
                DEFAULT_DISK_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
                DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
            )
            .map_err(|err| AppError::Setup {
                reason: format!("disk autoload failed: {err}"),
            })?;
        } else if cli.start_tape {
            session
                .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                    DEFAULT_TAPE_AUTOLOAD_SLOT,
                    MediaTransportAction::Start,
                )))
                .map_err(|err| AppError::Setup {
                    reason: format!("failed to start tape transport: {err}"),
                })?;
        }

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
        let audio_output = NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?;
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

    fn tape_loaded(&self) -> bool {
        self.runtime.machine().tape_is_loaded()
    }

    fn tape_playing(&self) -> bool {
        self.runtime.machine().tape_is_playing()
    }

    fn start_tape(&mut self) -> Result<(), AppError> {
        self.runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                MediaTransportAction::Start,
            )))?;
        self.run_frame(&[])?;
        Ok(())
    }

    fn stop_tape(&mut self) -> Result<(), AppError> {
        self.runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                MediaTransportAction::Stop,
            )))?;
        self.run_frame(&[])?;
        Ok(())
    }

    fn window_title(&self) -> String {
        let boot = if self.query_bool("boot.detected") {
            "booted"
        } else {
            "booting"
        };
        let tape = if self.tape_playing() {
            "tape playing"
        } else if self.tape_loaded() {
            "tape loaded"
        } else {
            "no tape"
        };
        let disk = if self.query_bool("c64.drive8.disk.inserted") {
            "disk loaded"
        } else {
            "no disk"
        };
        format!("{} | {} | {} | {}", self.title_base, boot, tape, disk)
    }
}

struct C64App {
    runner: C64Runner,
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

impl C64App {
    fn new(runner: C64Runner, scale: u32, turbo_tape: bool) -> Result<Self, AppError> {
        if scale == 0 {
            return Err(AppError::InvalidScale { value: scale });
        }

        let slice_ticks = subframe_ticks(runner.native_frame_ticks);
        let slice_duration = subframe_duration(c64_frame_duration_for_ticks(
            runner.native_frame_ticks,
            runner.runtime.profile().clock.rate.numerator_hz,
        ));
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

        let (frame_width, frame_height) = self.runner.frame_size();
        let logical_width = f64::from(frame_width.saturating_mul(self.scale));
        let logical_height = f64::from(frame_height.saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(self.window_title())
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
            self.next_slice_at = Instant::now();
        } else if let Some(names) = self.pressed_keys.remove(&code) {
            self.pending_inputs
                .extend(names.into_iter().map(|name| c64_key_event(name, false)));
            self.next_slice_at = Instant::now();
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

        let result = match code {
            KeyCode::Escape => {
                event_loop.exit();
                return true;
            }
            KeyCode::F9 => self.runner.start_tape(),
            KeyCode::F10 => self.runner.stop_tape(),
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

    let runner = C64Runner::from_cli(&cli)?;
    let mut app = C64App::new(runner, cli.scale, cli.turbo_tape)?;
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
            "--disk" => cli.disk = Some(PathBuf::from(next_arg(&mut iter, "--disk"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--autoload-disk" => cli.autoload_disk = true,
            "--autoload-tape" => cli.autoload_tape = true,
            "--start-tape" => cli.start_tape = true,
            "--turbo-tape" => cli.turbo_tape = true,
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
        (
            DRIVE1541_ID,
            resolve_rom_path(
                None,
                rom_dir.as_deref(),
                &["1541.rom", "dos1541.rom", "c1541.rom"],
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

fn c64_frame_duration_for_ticks(ticks: u64, clock_hz: u64) -> Duration {
    Duration::from_secs_f64(ticks as f64 / clock_hz as f64)
}

fn subframe_ticks(frame_ticks: u64) -> u64 {
    frame_ticks.div_ceil(u64::from(INPUT_SLICES_PER_FRAME))
}

fn subframe_duration(frame_duration: Duration) -> Duration {
    Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(INPUT_SLICES_PER_FRAME))
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
                disk: None,
                tape: None,
                autoload_disk: false,
                autoload_tape: false,
                start_tape: false,
                turbo_tape: false,
                load_snapshot: Some(PathBuf::from("ready.c64.pst")),
                scale: 3,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_flags() {
        let cli = parse_cli([
            "--tape".to_string(),
            "game.tap".to_string(),
            "--autoload-tape".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::Pal,
                rom_dir: None,
                kernal: None,
                basic: None,
                chargen: None,
                load: None,
                disk: None,
                tape: Some(PathBuf::from("game.tap")),
                autoload_disk: false,
                autoload_tape: true,
                start_tape: false,
                turbo_tape: false,
                load_snapshot: None,
                scale: DEFAULT_SCALE,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_turbo_flag() {
        let cli = parse_cli(["--turbo-tape".to_string()]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::Pal,
                rom_dir: None,
                kernal: None,
                basic: None,
                chargen: None,
                load: None,
                disk: None,
                tape: None,
                autoload_disk: false,
                autoload_tape: false,
                start_tape: false,
                turbo_tape: true,
                load_snapshot: None,
                scale: DEFAULT_SCALE,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_disk_flag() {
        let cli = parse_cli(["--disk".to_string(), "game.d64".to_string()]);

        assert_eq!(cli.disk, Some(PathBuf::from("game.d64")));
        assert_eq!(cli.tape, None);
    }

    #[test]
    fn parse_cli_accepts_disk_autoload_flag() {
        let cli = parse_cli(["--autoload-disk".to_string()]);

        assert!(cli.autoload_disk);
        assert!(!cli.autoload_tape);
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

    #[test]
    fn subframe_helpers_preserve_timing_budget() {
        let frame_ticks = u64::from(TIMING_PAL_BREADBIN.cycles_per_frame);
        let frame_duration = c64_frame_duration_for_ticks(frame_ticks, TIMING_PAL_BREADBIN.cpu_hz);
        let slice_ticks = subframe_ticks(frame_ticks);
        let slice_duration = subframe_duration(frame_duration);

        assert!(slice_ticks < frame_ticks);
        assert!(slice_ticks * u64::from(INPUT_SLICES_PER_FRAME) >= frame_ticks);
        assert!(slice_duration < frame_duration);
    }
}
