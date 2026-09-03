//! Generic browser host layer for Emu198x family runtimes.
//!
//! Supplies browser implementations of the host contracts `emu198x-shell`
//! already defines, and drives any [`FamilyRuntime`] from a page's animation
//! callback. Nothing here is specific to one machine: a per-system binding
//! crate names the runtime and the model, and this crate does the rest.
//!
//! The generality is close to free because every runtime in the workspace
//! already implements the same two traits. It is not speculative
//! abstraction — the abstraction exists, and this crate consumes it.

#![doc(html_no_source)]

pub mod audio;
pub mod frame;
pub mod input;
pub mod pacing;

use std::borrow::Cow;

use emu198x_shell::control::ControlCommand;
use emu198x_shell::machine::{RunResult, StopReason};
use emu198x_shell::query::{
    NoAdditionalQueries, QueryError, QueryResult, SessionQueryProvider, SessionView, query_value,
};
use emu198x_shell::session::SessionError;
use emu198x_shell::{
    FamilyRuntime, HostIo, InputEvent, MachineError, MachineTime, MediaImage, MediaKind, MediaSet,
    NullTraceSink, SessionDriver,
};

pub use audio::WebAudioOutput;
pub use frame::RgbaFrame;
pub use input::dom_code_to_key_name;
pub use pacing::Pacer;

/// A family runtime driven from a browser page.
///
/// Owns the machine, the pacing clock, and the most recent frame. The page
/// calls [`advance`](Self::advance) once per animation callback with the time
/// elapsed since the last one, then reads [`frame_rgba`](Self::frame_rgba).
/// Half a second at 48 kHz mono.
///
/// Deep enough to ride out a slow animation frame, shallow enough that a page
/// which stops draining drops audio rather than accruing latency the viewer
/// hears as the machine running behind the picture.
pub const DEFAULT_AUDIO_CAPACITY: usize = 24_000;

/// Sample rate assumed until the page says otherwise. Web Audio contexts are
/// commonly 48 kHz, and [`WebMachine::configure_audio`] corrects it.
pub const DEFAULT_AUDIO_RATE: u32 = 48_000;

pub struct WebMachine<R: FamilyRuntime, Q = NoAdditionalQueries> {
    runtime: R,
    frame: RgbaFrame,
    audio: WebAudioOutput,
    pacer: Pacer,
    frame_ticks: u64,
    pending_input: Vec<InputEvent>,
    query_provider: Q,
    last_run_result: Option<RunResult>,
}

impl<R: FamilyRuntime> WebMachine<R> {
    /// Wraps a runtime, taking its frame length from its own profile.
    ///
    /// The machine answers only the query paths every session shares. A page
    /// that needs a family's own paths — the Spectrum's `boot.detected`, say,
    /// which tape autoload waits on — uses
    /// [`new_with_query_provider`](Self::new_with_query_provider).
    #[must_use]
    pub fn new(runtime: R) -> Self {
        Self::new_with_query_provider(runtime, NoAdditionalQueries)
    }
}

impl<R: FamilyRuntime, Q: SessionQueryProvider<R>> WebMachine<R, Q> {
    /// Wraps a runtime alongside the provider for its family's query paths.
    #[must_use]
    pub fn new_with_query_provider(runtime: R, query_provider: Q) -> Self {
        let frame_ticks = runtime.native_frame_ticks();
        let frame_ms = frame_duration_ms(&runtime, frame_ticks);
        Self {
            runtime,
            frame: RgbaFrame::new(),
            audio: WebAudioOutput::new(DEFAULT_AUDIO_RATE, 1, DEFAULT_AUDIO_CAPACITY),
            pacer: Pacer::new(frame_ms),
            frame_ticks,
            pending_input: Vec::new(),
            query_provider,
            last_run_result: None,
        }
    }

    /// Runs whole machine frames to consume `elapsed_ms`, returning how many
    /// ran.
    ///
    /// Zero is a normal answer: a 60 Hz display driving a 50 Hz machine has
    /// nothing to do on roughly one tick in six.
    ///
    /// # Errors
    ///
    /// Returns [`MachineError`] if the machine rejects a run.
    pub fn advance(&mut self, elapsed_ms: f64) -> Result<u32, MachineError> {
        let owed = self.pacer.frames_owed(elapsed_ms);
        for _ in 0..owed {
            self.run_one_frame()?;
        }
        Ok(owed)
    }

    /// Runs exactly one machine frame, ignoring the pacing clock.
    ///
    /// For stepping and for tests. A page should call [`advance`](Self::advance).
    ///
    /// # Errors
    ///
    /// Returns [`MachineError`] if the machine rejects the run.
    pub fn run_one_frame(&mut self) -> Result<(), MachineError> {
        let target = MachineTime::new(self.runtime.time().get().saturating_add(self.frame_ticks));

        let mut trace = NullTraceSink;
        let mut host = HostIo {
            input_events: &self.pending_input,
            frame_sink: &mut self.frame,
            audio_sink: &mut self.audio,
            trace_sink: &mut trace,
        };
        let result = self.runtime.run_until(target, &mut host)?;
        self.last_run_result = Some(result);

        // Drained after the run, not before: the events belong to the frame
        // that just executed, and leaving them queued would replay every
        // keypress on every subsequent frame.
        self.pending_input.clear();
        Ok(())
    }

    /// Queues an input event for the next frame.
    pub fn queue_input(&mut self, event: InputEvent) {
        self.pending_input.push(event);
    }

    /// Queues a press or release for a DOM `KeyboardEvent.code`.
    ///
    /// Returns `false` when the code has no machine-neutral name, or when the
    /// machine does not recognise the name it maps to — in both cases nothing
    /// is queued and the page should let the browser keep the keystroke.
    ///
    /// Modifiers are not mapped here. A per-system binding calls
    /// [`queue_key`](Self::queue_key) with its own names for those.
    pub fn key_event(&mut self, dom_code: &str, pressed: bool) -> bool {
        let Some(name) = dom_code_to_key_name(dom_code) else {
            return false;
        };
        self.queue_key(name, pressed)
    }

    /// Queues a press or release for a machine key name.
    ///
    /// The machine validates the name, so a binding cannot silently inject a
    /// key its own layout does not have. Returns `false` when the name is
    /// rejected, having queued nothing.
    ///
    /// Compound names are expanded into the chord that produces them, because
    /// several keycaps a viewer expects are not single keys on the hardware:
    /// the Spectrum's cursor keys are `CapsShift` plus a digit, and queueing
    /// the bare name `"Up"` would be a dead key.
    pub fn queue_key(&mut self, name: impl Into<Cow<'static, str>>, pressed: bool) -> bool {
        let name = name.into();
        let Some(mut chord) = self.resolve_key(&name) else {
            return false;
        };

        // Press modifiers first and release them last, as the hardware would.
        if !pressed {
            chord.reverse();
        }
        for key in chord {
            self.queue_input(InputEvent::Key { name: key, pressed });
        }
        true
    }

    /// Whether the machine can produce `name`, as a key or as a chord.
    #[must_use]
    pub fn accepts_key(&self, name: &str) -> bool {
        self.resolve_key(name).is_some()
    }

    /// The keys that together produce `name` on this machine.
    ///
    /// A machine that exposes no keyboard description accepts anything: it has
    /// not told us otherwise, and refusing every key would be worse than
    /// forwarding one it ignores.
    fn resolve_key(&self, name: &str) -> Option<Vec<Cow<'static, str>>> {
        let Some(keyboard) = self.runtime.keyboard_target() else {
            return Some(vec![Cow::Owned(name.to_owned())]);
        };
        if keyboard.key_name_is_valid(name) {
            return Some(vec![Cow::Owned(name.to_owned())]);
        }
        keyboard
            .expand_named_key(name)
            .map(|chord| chord.into_iter().map(Cow::Owned).collect())
    }

    /// Events queued but not yet handed to the machine.
    #[must_use]
    pub fn pending_input(&self) -> &[InputEvent] {
        &self.pending_input
    }

    /// Loads media into `slot` from bytes the page supplies.
    ///
    /// This is how a lesson runs the program a learner just assembled: the
    /// page fetches or builds the bytes and hands them straight over. No
    /// filesystem is involved at any point, which is why the browser needed no
    /// new loading path — the runtimes already take bytes rather than paths.
    ///
    /// `kind` is explicit rather than sniffed from the bytes. The page knows
    /// what it fetched, and guessing wrong would mount a snapshot as a tape.
    ///
    /// # Errors
    ///
    /// Returns [`MachineError`] if the machine has no such slot or rejects the
    /// image.
    pub fn load_media_bytes(
        &mut self,
        slot: &str,
        kind: MediaKind,
        bytes: &[u8],
    ) -> Result<(), MachineError> {
        if !self.has_slot(slot) {
            return Err(MachineError::UnsupportedOperation {
                operation: "load_media_bytes: unknown slot",
            });
        }
        let mut media = MediaSet::new();
        media.push(MediaImage::new(slot.to_owned(), kind, bytes));
        self.runtime.load_media(&media)
    }

    /// Slot identifiers this machine's profile declares, such as `tape-1`.
    #[must_use]
    pub fn media_slots(&self) -> Vec<&str> {
        self.runtime
            .profile()
            .media_slots
            .iter()
            .map(|slot| slot.id.as_ref())
            .collect()
    }

    /// Whether the machine declares `slot`.
    #[must_use]
    pub fn has_slot(&self, slot: &str) -> bool {
        self.runtime
            .profile()
            .media_slots
            .iter()
            .any(|declared| declared.id == slot)
    }

    /// Matches the buffer to the page's Web Audio graph.
    ///
    /// Call once the page has an `AudioContext`, because its sample rate is
    /// the browser's choice rather than ours. Discards anything buffered: it
    /// was converted for the old rate and would play at the wrong pitch.
    pub fn configure_audio(&mut self, output_rate: u32, output_channels: u16, capacity: usize) {
        let enabled = self.audio.is_enabled();
        self.audio = WebAudioOutput::new(output_rate, output_channels, capacity);
        self.audio.set_enabled(enabled);
    }

    /// Takes the buffered audio for the page to feed its worklet.
    #[must_use]
    pub fn audio_drain(&mut self) -> Vec<f32> {
        self.audio.drain()
    }

    /// Takes at most `count` buffered samples.
    #[must_use]
    pub fn audio_drain_at_most(&mut self, count: usize) -> Vec<f32> {
        self.audio.drain_at_most(count)
    }

    /// Starts or stops buffering machine audio.
    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.audio.set_enabled(enabled);
    }

    /// The audio buffer, for its fill level and drop count.
    #[must_use]
    pub const fn audio(&self) -> &WebAudioOutput {
        &self.audio
    }

    /// The most recent frame, for a consumer that presents it itself.
    #[must_use]
    pub const fn frame(&self) -> &RgbaFrame {
        &self.frame
    }

    /// RGBA bytes of the most recent frame, empty before the first one.
    #[must_use]
    pub fn frame_rgba(&self) -> &[u8] {
        self.frame.pixels()
    }

    /// Width and height of the most recent frame.
    #[must_use]
    pub fn frame_size(&self) -> (u32, u32) {
        self.frame.size()
    }

    /// Runs the machine ahead of the clock, or stops doing so.
    ///
    /// For fast-loading a tape. The caller decides when, because whether a
    /// tape is playing is a per-family question this crate deliberately does
    /// not ask — a binding that knows its machine sets this each tick.
    pub const fn set_turbo(&mut self, turbo: bool) {
        self.pacer.set_turbo(turbo);
    }

    /// Whether the machine is currently running ahead of the clock.
    #[must_use]
    pub const fn is_turbo(&self) -> bool {
        self.pacer.is_turbo()
    }

    /// The machine's frame duration in milliseconds.
    #[must_use]
    pub fn frame_ms(&self) -> f64 {
        self.pacer.frame_ms()
    }

    /// Resolves one query path against the machine's current state.
    ///
    /// Shared session paths first, then this machine's family provider —
    /// the same order [`HeadlessSession`] uses, so a helper written against
    /// one host answers identically on the other.
    ///
    /// [`HeadlessSession`]: emu198x_shell::HeadlessSession
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if neither knows the path.
    pub fn query(&self, path: &str) -> Result<QueryResult, QueryError> {
        let view = SessionView {
            profile: self.runtime.profile(),
            display: self.runtime.display(),
            time: self.runtime.time(),
            native_frame_ticks: self.frame_ticks,
            has_frame: self.frame.captured().is_some(),
            framebuffer: self
                .frame
                .captured()
                .map(|frame| (frame.width, frame.height)),
            has_audio: !self.audio.is_empty(),
            last_run_result: self.last_run_result,
        };
        match query_value(&view, path) {
            Ok(result) => Ok(result),
            Err(QueryError::UnknownPath { .. }) => self
                .query_provider
                .query(&self.runtime, path)?
                .ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                }),
            Err(err) => Err(err),
        }
    }

    /// The wrapped runtime.
    pub const fn runtime(&self) -> &R {
        &self.runtime
    }

    /// The wrapped runtime, mutably, for media loading and control.
    pub const fn runtime_mut(&mut self) -> &mut R {
        &mut self.runtime
    }
}

impl<R, Q> SessionDriver for WebMachine<R, Q>
where
    R: FamilyRuntime,
    Q: SessionQueryProvider<R>,
{
    fn time(&self) -> MachineTime {
        self.runtime.time()
    }

    fn query(&self, path: &str) -> Result<QueryResult, QueryError> {
        Self::query(self, path)
    }

    fn queue_input(&mut self, event: InputEvent) {
        Self::queue_input(self, event);
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), SessionError> {
        self.runtime.command(command)?;
        Ok(())
    }

    /// Frame at a time with a stall check, matching the session rather than
    /// running straight to a multi-frame target: a helper that waits on a
    /// query between frames needs each frame's sink output, and a machine that
    /// stops advancing must end the run instead of spinning out the count.
    fn run_frames(&mut self, count: u32) -> Result<RunResult, SessionError> {
        let mut last = RunResult::new(self.runtime.time(), StopReason::ReachedTarget);
        for _ in 0..count {
            let before = self.runtime.time();
            self.run_one_frame()?;
            last = self.last_run_result.unwrap_or(last);
            if last.stop_reason != StopReason::ReachedTarget || self.runtime.time() <= before {
                break;
            }
        }
        Ok(last)
    }
}

/// One frame's duration in milliseconds, from the machine's own clock.
///
/// Derived rather than hardcoded so a 50.08 Hz Spectrum and a 60.10 Hz NES
/// are both paced from what their profile actually declares.
fn frame_duration_ms<R: FamilyRuntime>(runtime: &R, frame_ticks: u64) -> f64 {
    let rate = runtime.profile().clock.rate;
    if rate.numerator_hz == 0 {
        return 0.0;
    }
    // ticks / (num/den) Hz * 1000 ms
    (frame_ticks as f64) * 1000.0 * (rate.denominator_hz as f64) / (rate.numerator_hz as f64)
}
