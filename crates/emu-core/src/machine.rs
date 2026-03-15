//! Common interface for all system emulators.
//!
//! Every system implements `Machine`, providing a uniform API for
//! frame-level emulation, display output, audio output, and reset.
//! This enables generic tooling: save states, recording, WASM wrappers,
//! and windowed runners can all be written once against the trait.

/// Stereo audio frame: left and right channels.
pub type AudioFrame = [f32; 2];

/// Common interface for system emulators.
///
/// Captures the operations every emulated system shares: running a frame,
/// reading the framebuffer, draining audio, and resetting. System-specific
/// operations (key input, media insertion, controller state) live outside
/// this trait on each system's concrete type.
pub trait Machine {
    /// Run one complete frame of emulation.
    fn run_frame(&mut self);

    /// The current framebuffer as ARGB32 pixels.
    fn framebuffer(&self) -> &[u32];

    /// Framebuffer width in pixels.
    fn framebuffer_width(&self) -> u32;

    /// Framebuffer height in pixels.
    fn framebuffer_height(&self) -> u32;

    /// Drain the audio output buffer.
    ///
    /// Returns stereo sample pairs (left, right) at the system's output
    /// sample rate (typically 48 kHz). Mono systems duplicate the sample
    /// to both channels.
    fn take_audio_buffer(&mut self) -> Vec<AudioFrame>;

    /// Audio sample rate for `take_audio_buffer()`.
    ///
    /// Most current machines emit 48 kHz audio, so that is the default.
    fn audio_sample_rate(&self) -> u32 {
        48_000
    }

    /// Total number of completed frames since creation.
    fn frame_count(&self) -> u64;

    /// Reset the system (equivalent to pressing the reset button).
    fn reset(&mut self);
}
