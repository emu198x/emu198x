//! Keyboard-to-controller mapping for the Atari 7800.
//!
//! Maps keyboard keys to joystick directions, fire buttons, and
//! console buttons via RIOT port A/B and TIA input.

use winit::keyboard::KeyCode;

/// Controller input action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari7800Input {
    /// Player 0 joystick up.
    P0Up,
    /// Player 0 joystick down.
    P0Down,
    /// Player 0 joystick left.
    P0Left,
    /// Player 0 joystick right.
    P0Right,
    /// Player 0 left fire button.
    P0Fire,
    /// Player 0 right fire button.
    P0Fire2,
    /// Pause button.
    Pause,
    /// Reset console switch.
    Reset,
    /// Select console switch.
    Select,
}

/// Map a keycode to an Atari 7800 input.
#[must_use]
pub fn map_keycode(keycode: KeyCode) -> Option<Atari7800Input> {
    match keycode {
        KeyCode::ArrowUp => Some(Atari7800Input::P0Up),
        KeyCode::ArrowDown => Some(Atari7800Input::P0Down),
        KeyCode::ArrowLeft => Some(Atari7800Input::P0Left),
        KeyCode::ArrowRight => Some(Atari7800Input::P0Right),
        KeyCode::Space => Some(Atari7800Input::P0Fire),
        KeyCode::ShiftLeft => Some(Atari7800Input::P0Fire2),
        KeyCode::F1 => Some(Atari7800Input::Reset),
        KeyCode::F2 => Some(Atari7800Input::Select),
        KeyCode::F3 => Some(Atari7800Input::Pause),
        _ => None,
    }
}
