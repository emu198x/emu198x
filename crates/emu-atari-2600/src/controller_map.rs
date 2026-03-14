//! Keyboard-to-joystick mapping for the Atari 2600.
//!
//! Maps keyboard keys to joystick directions (RIOT port A bits),
//! fire button (TIA INPT4), and console switches (RIOT port B bits).

use winit::keyboard::KeyCode;

/// Joystick direction or button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari2600Input {
    /// Player 0 joystick up (SWCHA bit 4, active low).
    P0Up,
    /// Player 0 joystick down (SWCHA bit 5, active low).
    P0Down,
    /// Player 0 joystick left (SWCHA bit 6, active low).
    P0Left,
    /// Player 0 joystick right (SWCHA bit 7, active low).
    P0Right,
    /// Player 0 fire button (INPT4 bit 7, active low).
    P0Fire,
    /// Console reset switch (SWCHB bit 0, active low).
    Reset,
    /// Console select switch (SWCHB bit 1, active low).
    Select,
}

/// Map a keycode to an Atari 2600 input.
#[must_use]
pub fn map_keycode(keycode: KeyCode) -> Option<Atari2600Input> {
    match keycode {
        KeyCode::ArrowUp => Some(Atari2600Input::P0Up),
        KeyCode::ArrowDown => Some(Atari2600Input::P0Down),
        KeyCode::ArrowLeft => Some(Atari2600Input::P0Left),
        KeyCode::ArrowRight => Some(Atari2600Input::P0Right),
        KeyCode::Space => Some(Atari2600Input::P0Fire),
        KeyCode::F1 => Some(Atari2600Input::Reset),
        KeyCode::F2 => Some(Atari2600Input::Select),
        _ => None,
    }
}

/// RIOT port A bit mask for a joystick direction.
///
/// Port A bits are active-low: 0 = pressed, 1 = released.
/// Player 0 uses bits 4-7.
#[must_use]
pub fn swcha_bit(input: Atari2600Input) -> Option<u8> {
    match input {
        Atari2600Input::P0Up => Some(0x10),
        Atari2600Input::P0Down => Some(0x20),
        Atari2600Input::P0Left => Some(0x40),
        Atari2600Input::P0Right => Some(0x80),
        _ => None,
    }
}

/// RIOT port B bit mask for a console switch.
#[must_use]
pub fn swchb_bit(input: Atari2600Input) -> Option<u8> {
    match input {
        Atari2600Input::Reset => Some(0x01),
        Atari2600Input::Select => Some(0x02),
        _ => None,
    }
}
