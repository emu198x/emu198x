//! Keyboard-to-controller mapping for the Atari 5200.
//!
//! Maps keyboard keys to joystick axes (via POKEY pots), fire button
//! (GTIA TRIG0), and console buttons (GTIA CONSOL).

use winit::keyboard::KeyCode;

/// Controller input action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari5200Input {
    /// Player 0 joystick up.
    P0Up,
    /// Player 0 joystick down.
    P0Down,
    /// Player 0 joystick left.
    P0Left,
    /// Player 0 joystick right.
    P0Right,
    /// Player 0 fire button (TRIG0).
    P0Fire,
    /// Start button (CONSOL).
    Start,
    /// Pause button.
    Pause,
    /// Reset.
    Reset,
}

/// Map a keycode to an Atari 5200 input.
#[must_use]
pub fn map_keycode(keycode: KeyCode) -> Option<Atari5200Input> {
    match keycode {
        KeyCode::ArrowUp => Some(Atari5200Input::P0Up),
        KeyCode::ArrowDown => Some(Atari5200Input::P0Down),
        KeyCode::ArrowLeft => Some(Atari5200Input::P0Left),
        KeyCode::ArrowRight => Some(Atari5200Input::P0Right),
        KeyCode::Space => Some(Atari5200Input::P0Fire),
        KeyCode::F1 => Some(Atari5200Input::Start),
        KeyCode::F2 => Some(Atari5200Input::Pause),
        KeyCode::F3 => Some(Atari5200Input::Reset),
        _ => None,
    }
}

/// Joystick centre value for POKEY pot registers (0-228 range).
pub const POT_CENTER: u8 = 114;

/// Joystick minimum value (fully left or fully up).
pub const POT_MIN: u8 = 0;

/// Joystick maximum value (fully right or fully down).
pub const POT_MAX: u8 = 228;
