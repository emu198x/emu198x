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

pub mod frame;
pub mod pacing;

use emu198x_shell::{
    FamilyRuntime, HostIo, InputEvent, MachineError, MachineTime, NullAudioSink, NullTraceSink,
};

pub use frame::RgbaFrame;
pub use pacing::Pacer;

/// A family runtime driven from a browser page.
///
/// Owns the machine, the pacing clock, and the most recent frame. The page
/// calls [`advance`](Self::advance) once per animation callback with the time
/// elapsed since the last one, then reads [`frame_rgba`](Self::frame_rgba).
pub struct WebMachine<R: FamilyRuntime> {
    runtime: R,
    frame: RgbaFrame,
    pacer: Pacer,
    frame_ticks: u64,
    pending_input: Vec<InputEvent>,
}

impl<R: FamilyRuntime> WebMachine<R> {
    /// Wraps a runtime, taking its frame length from its own profile.
    #[must_use]
    pub fn new(runtime: R) -> Self {
        let frame_ticks = runtime.native_frame_ticks();
        let frame_ms = frame_duration_ms(&runtime, frame_ticks);
        Self {
            runtime,
            frame: RgbaFrame::new(),
            pacer: Pacer::new(frame_ms),
            frame_ticks,
            pending_input: Vec::new(),
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

        let mut audio = NullAudioSink;
        let mut trace = NullTraceSink;
        let mut host = HostIo {
            input_events: &self.pending_input,
            frame_sink: &mut self.frame,
            audio_sink: &mut audio,
            trace_sink: &mut trace,
        };
        self.runtime.run_until(target, &mut host)?;

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

    /// Events queued but not yet handed to the machine.
    #[must_use]
    pub fn pending_input(&self) -> &[InputEvent] {
        &self.pending_input
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

    /// The machine's frame duration in milliseconds.
    #[must_use]
    pub fn frame_ms(&self) -> f64 {
        self.pacer.frame_ms()
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
