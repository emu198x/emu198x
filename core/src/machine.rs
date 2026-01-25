//! Machine abstraction for emulated systems.
//!
//! This module defines the `Machine` trait which provides a common interface
//! for different emulated systems (Spectrum, C64, NES, etc.) to work with
//! the shared runner infrastructure.

/// Video output configuration for a machine.
#[derive(Debug, Clone, Copy)]
pub struct VideoConfig {
    /// Native display width in pixels.
    pub width: u32,
    /// Native display height in pixels.
    pub height: u32,
    /// Frame rate in frames per second.
    pub fps: f32,
}

/// Audio output configuration for a machine.
#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    /// Audio sample rate in Hz.
    pub sample_rate: u32,
    /// Number of audio samples per frame.
    pub samples_per_frame: usize,
}

/// Joystick state (generic for all systems).
///
/// Machines map this to their specific joystick format internally.
#[derive(Debug, Clone, Copy, Default)]
pub struct JoystickState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub fire: bool,
    pub fire2: bool,
}

/// Key codes supported by the emulator.
///
/// This is a subset of winit's KeyCode to avoid exposing winit in the core crate.
/// Machines handle mapping these to their native keyboard format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    // Letters
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    // Numbers
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,

    // Modifiers
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,

    // Special
    Enter,
    Space,
    Backspace,
    Tab,
    Escape,

    // Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Numpad (for joystick emulation)
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Punctuation
    Comma,
    Period,
    Slash,
    Semicolon,
    Quote,
    BracketLeft,
    BracketRight,
    Backslash,
    Minus,
    Equal,
    Backquote,
}

/// Trait for emulated machines.
///
/// Provides a common interface for the runner to interact with different
/// emulated systems without knowing their specific implementation details.
pub trait Machine {
    /// Get the video output configuration.
    fn video_config(&self) -> VideoConfig;

    /// Get the audio output configuration.
    fn audio_config(&self) -> AudioConfig;

    /// Execute one frame of emulation.
    fn run_frame(&mut self);

    /// Render the current display to an RGBA pixel buffer.
    ///
    /// The buffer size should be `width * height * 4` bytes.
    fn render(&mut self, buffer: &mut [u8]);

    /// Generate audio samples for the current frame.
    ///
    /// The buffer size should match `audio_config().samples_per_frame`.
    fn generate_audio(&mut self, buffer: &mut [f32]);

    /// Handle a key press event.
    fn key_down(&mut self, key: KeyCode);

    /// Handle a key release event.
    fn key_up(&mut self, key: KeyCode);

    /// Set the joystick state for a given port.
    fn set_joystick(&mut self, port: u8, state: JoystickState);

    /// Reset the machine to its initial state.
    fn reset(&mut self);

    /// Load a file into the machine.
    ///
    /// The machine should determine the file type from the extension.
    fn load_file(&mut self, path: &str, data: &[u8]) -> Result<(), String>;
}
