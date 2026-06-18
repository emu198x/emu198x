//! Shared native-UI harness for Emu198x runners.
//!
//! Six runners (NES, Game Boy, Amiga, C64, Dragon, Spectrum) each carried a
//! near-identical winit `ApplicationHandler` + wgpu video + framed audio +
//! keyboard/gamepad-input loop. This crate factors that common spine out: a
//! runner supplies a small [`UiSystem`] descriptor (its runtime, framebuffer
//! size, frame timing, button map, and key map) and calls [`run`] to get a
//! native window with `raw`/`lcd`/`crt` video filters, framed audio, gamepad
//! input, and Esc-quit / F12-reset.
//!
//! This is the minimal first cut (no native menu, media UI, or save-state
//! dialogs yet); those land as the existing runners migrate onto the harness.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use emu198x_native_video::{PresentationProfile, VideoPresenterError, WgpuVideoPresenter};
use emu198x_shell::{
    CapturedFrame, HostIo, InputEvent, LatestFrameCapture, MachineCore, MachineError,
    NativeAudioError, NativeAudioOutput, NativeGamepadInput, NullTraceSink, ResetKind, RunResult,
};
use thiserror::Error;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowAttributes, WindowId};

// Re-exported so a `UiSystem` impl can describe itself without depending on
// winit / native-video / shell input types directly.
pub use emu198x_native_video::VideoFilter;
pub use emu198x_shell::{ButtonInputMap, ButtonTarget, HostControl};
pub use winit::keyboard::KeyCode;

const MAX_CATCH_UP_FRAMES: u32 = 4;
const MAX_AUDIO_BUFFER_MS: u32 = 250;

/// Per-system configuration the harness needs to host a machine in a window.
///
/// The runner builds its runtime (CLI parsing, cartridge/media loading) and
/// hands it to [`run`] alongside an implementor of this trait.
pub trait UiSystem {
    /// The machine runtime this system drives.
    type Runtime: MachineCore;

    /// Window title.
    fn window_title(&self) -> String;

    /// Default integer window scale when the CLI doesn't override it.
    fn default_scale(&self) -> u32 {
        3
    }

    /// Input sub-slices per frame. Use `1` for runtimes that advance in whole
    /// frames (so a sub-frame target would overshoot); higher values give
    /// finer input latency on runtimes that honour sub-frame targets.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    /// Framebuffer dimensions (pixels) the presenter renders at.
    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32);

    /// Machine ticks in one displayed frame.
    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64;

    /// Wall-clock duration of one displayed frame.
    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration;

    /// The host-control → (port, input-name) map shared by keyboard and gamepad.
    fn button_map(&self) -> &'static ButtonInputMap;

    /// Translate a physical key to a host control, or `None` to ignore it.
    fn map_key(&self, code: KeyCode) -> Option<HostControl>;

    /// Hook run after a hard reset re-initialises the runtime (e.g. re-insert
    /// media for runtimes that drop it on reset). Default: nothing.
    fn after_reset(&mut self, _runtime: &mut Self::Runtime) -> Result<(), MachineError> {
        Ok(())
    }
}

/// Errors that can end a UI session.
#[derive(Debug, Error)]
pub enum UiError {
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
}

/// Drives a [`MachineCore`] runtime a frame (or sub-frame) at a time, capturing
/// the latest frame and feeding framed audio.
struct Runner<R: MachineCore> {
    runtime: R,
    frame_capture: LatestFrameCapture,
    audio_output: NativeAudioOutput,
    last_run_result: Option<RunResult>,
}

impl<R: MachineCore> Runner<R> {
    fn new(runtime: R) -> Result<Self, UiError> {
        Ok(Self {
            runtime,
            frame_capture: LatestFrameCapture::default(),
            audio_output: NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?,
            last_run_result: None,
        })
    }

    fn run_ticks(&mut self, input_events: &[InputEvent], ticks: u64) -> Result<bool, UiError> {
        let previous = self.frame().map(|frame| frame.timestamp);
        let target = self.runtime.time().saturating_add(ticks);
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events,
            frame_sink: &mut self.frame_capture,
            audio_sink: &mut self.audio_output,
            trace_sink: &mut trace_sink,
        };
        self.last_run_result = Some(self.runtime.run_until(target, &mut host)?);
        Ok(self.frame().map(|frame| frame.timestamp) != previous)
    }

    fn frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
    }
}

/// The winit application: hosts the window, the wgpu presenter, the pacing
/// loop, and keyboard/gamepad input for one [`UiSystem`].
struct App<S: UiSystem> {
    system: S,
    runner: Runner<S::Runtime>,
    scale: u32,
    slice_ticks: u64,
    slice_duration: Duration,
    full_frame_ticks: u64,
    next_slice_at: Instant,
    pending_inputs: Vec<InputEvent>,
    pressed_keys: HashMap<KeyCode, HostControl>,
    gamepads: NativeGamepadInput,
    window: Option<Arc<Window>>,
    video: Option<WgpuVideoPresenter>,
    presentation: PresentationProfile,
    fatal_error: Option<UiError>,
}

impl<S: UiSystem> App<S> {
    fn new(system: S, runner: Runner<S::Runtime>, scale: u32, video: VideoFilter) -> Self {
        let slices = system.input_slices_per_frame().max(1);
        let frame_ticks = system.frame_ticks(&runner.runtime);
        let frame_duration = system.frame_duration(&runner.runtime);
        let slice_ticks = frame_ticks.div_ceil(u64::from(slices));
        let slice_duration =
            Duration::from_secs_f64(frame_duration.as_secs_f64() / f64::from(slices));
        Self {
            system,
            runner,
            scale,
            slice_ticks,
            slice_duration,
            full_frame_ticks: frame_ticks,
            next_slice_at: Instant::now(),
            pending_inputs: Vec::new(),
            pressed_keys: HashMap::new(),
            gamepads: NativeGamepadInput::new(),
            window: None,
            video: None,
            presentation: PresentationProfile::for_filter(video),
            fatal_error: None,
        }
    }

    fn take_error(&mut self) -> Option<UiError> {
        self.fatal_error.take()
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: UiError) {
        eprintln!("error: {err}");
        self.fatal_error = Some(err);
        event_loop.exit();
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), UiError> {
        if self.window.is_some() {
            return Ok(());
        }
        let (fb_width, fb_height) = self.system.framebuffer_size(&self.runner.runtime);
        let logical_width = f64::from(fb_width.saturating_mul(self.scale));
        let logical_height = f64::from(fb_height.saturating_mul(self.scale));
        let attributes = WindowAttributes::default()
            .with_title(self.system.window_title())
            .with_inner_size(LogicalSize::new(logical_width, logical_height))
            .with_min_inner_size(LogicalSize::new(f64::from(fb_width), f64::from(fb_height)));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let video = WgpuVideoPresenter::new(window.clone(), fb_width, fb_height)?;
        self.window = Some(window);
        self.video = Some(video);
        self.next_slice_at = Instant::now();
        Ok(())
    }

    fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn advance_machine(&mut self) -> Result<bool, UiError> {
        self.gamepads
            .drain_events(self.system.button_map(), &mut self.pending_inputs);

        if Instant::now() < self.next_slice_at {
            return Ok(false);
        }

        let mut ran = 0u32;
        let max_slices =
            MAX_CATCH_UP_FRAMES.saturating_mul(self.system.input_slices_per_frame().max(1));
        let mut frame_completed = false;
        while Instant::now() >= self.next_slice_at && ran < max_slices {
            let inputs = std::mem::take(&mut self.pending_inputs);
            frame_completed |= self.runner.run_ticks(&inputs, self.slice_ticks)?;
            self.next_slice_at += self.slice_duration;
            ran += 1;
        }
        if ran == max_slices && Instant::now() >= self.next_slice_at {
            self.next_slice_at = Instant::now() + self.slice_duration;
        }
        Ok(frame_completed)
    }

    fn render(&mut self) -> Result<(), UiError> {
        let (Some(frame), Some(video)) = (self.runner.frame(), self.video.as_mut()) else {
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
        let map = self.system.button_map();
        if pressed {
            let Some(control) = self.system.map_key(code) else {
                return;
            };
            if self.pressed_keys.contains_key(&code) {
                return;
            }
            self.pressed_keys.insert(code, control);
            if let Some(input) = map.event(control, true) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        } else if let Some(control) = self.pressed_keys.remove(&code) {
            if let Some(input) = map.event(control, false) {
                self.pending_inputs.push(input);
            }
            self.next_slice_at = Instant::now();
        }
    }

    fn release_all_keys(&mut self) {
        let map = self.system.button_map();
        for control in std::mem::take(&mut self.pressed_keys).into_values() {
            if let Some(input) = map.event(control, false) {
                self.pending_inputs.push(input);
            }
        }
        self.next_slice_at = Instant::now();
    }

    /// Hard-reset the machine: reset the runtime, run the system's re-init
    /// hook, clear capture/audio, and run one frame so a picture is ready.
    fn reset_machine(&mut self) -> Result<(), UiError> {
        self.release_all_keys();
        self.runner.runtime.reset(ResetKind::Hard);
        self.system.after_reset(&mut self.runner.runtime)?;
        self.runner.frame_capture = LatestFrameCapture::default();
        self.runner.audio_output.clear();
        self.runner.last_run_result = None;
        self.runner.run_ticks(&[], self.full_frame_ticks)?;
        Ok(())
    }

    /// Returns `true` if the key was consumed as a UI shortcut.
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
                if let Err(err) = self.reset_machine() {
                    self.fail(event_loop, err);
                }
                true
            }
            _ => false,
        }
    }
}

impl<S: UiSystem> ApplicationHandler for App<S> {
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
            WindowEvent::Resized(size) => self.resize_surface(size.width, size.height),
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

/// Open a native window for `system` driving `runtime`, and run the winit
/// event loop until the window closes or a fatal error surfaces.
///
/// The runtime should already be initialised (media loaded); the harness runs
/// one frame up front so the first redraw has a picture.
///
/// # Errors
///
/// Returns [`UiError`] for an invalid scale, audio/video init failure, a
/// machine error, or an event-loop error.
pub fn run<S: UiSystem>(
    system: S,
    runtime: S::Runtime,
    scale: u32,
    video: VideoFilter,
) -> Result<(), UiError> {
    if scale == 0 {
        return Err(UiError::InvalidScale { value: scale });
    }
    let mut runner = Runner::new(runtime)?;
    let frame_ticks = system.frame_ticks(&runner.runtime);
    runner.run_ticks(&[], frame_ticks)?;

    let mut app = App::new(system, runner, scale, video);
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    if let Some(err) = app.take_error() {
        return Err(err);
    }
    Ok(())
}
