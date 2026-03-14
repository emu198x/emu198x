//! Keyboard-to-controller mapping for the Atari 800XL.
//!
//! Maps keyboard keys to joystick directions (PIA PORTA), fire button
//! (GTIA TRIG0), and console keys (GTIA CONSOL).

use winit::keyboard::KeyCode;

/// Controller input action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari800xlInput {
    /// Player 1 joystick up (PIA PORTA bit 0, active low).
    P0Up,
    /// Player 1 joystick down (PIA PORTA bit 1, active low).
    P0Down,
    /// Player 1 joystick left (PIA PORTA bit 2, active low).
    P0Left,
    /// Player 1 joystick right (PIA PORTA bit 3, active low).
    P0Right,
    /// Player 1 fire button (GTIA TRIG0).
    P0Fire,
    /// START console key (GTIA CONSOL bit 0).
    Start,
    /// SELECT console key (GTIA CONSOL bit 1).
    Select,
    /// OPTION console key (GTIA CONSOL bit 2).
    Option,
    /// BREAK key (POKEY).
    Break,
}

/// Map a keycode to an Atari 800XL input.
#[must_use]
pub fn map_keycode(keycode: KeyCode) -> Option<Atari800xlInput> {
    match keycode {
        KeyCode::ArrowUp => Some(Atari800xlInput::P0Up),
        KeyCode::ArrowDown => Some(Atari800xlInput::P0Down),
        KeyCode::ArrowLeft => Some(Atari800xlInput::P0Left),
        KeyCode::ArrowRight => Some(Atari800xlInput::P0Right),
        KeyCode::Space => Some(Atari800xlInput::P0Fire),
        KeyCode::F1 => Some(Atari800xlInput::Start),
        KeyCode::F2 => Some(Atari800xlInput::Select),
        KeyCode::F3 => Some(Atari800xlInput::Option),
        KeyCode::Escape => Some(Atari800xlInput::Break),
        _ => None,
    }
}
