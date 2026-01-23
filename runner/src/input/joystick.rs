//! Kempston joystick input handling.
//!
//! The Kempston interface is read from port 0x1F (31).
//! Unlike the keyboard matrix, it uses active-high logic.
//!
//! Supports both keyboard (numpad) and gamepad input.

use gilrs::{Axis, Button, GamepadId, Gilrs};
use minifb::Key;

/// Kempston joystick bit positions (active high).
pub const KEMPSTON_RIGHT: u8 = 0x01;
pub const KEMPSTON_LEFT: u8 = 0x02;
pub const KEMPSTON_DOWN: u8 = 0x04;
pub const KEMPSTON_UP: u8 = 0x08;
pub const KEMPSTON_FIRE: u8 = 0x10;

/// Threshold for analog stick to register as a direction.
const AXIS_THRESHOLD: f32 = 0.5;

/// Map PC keyboard keys to Kempston joystick state.
pub fn map_keyboard(keys: &[Key]) -> u8 {
    let mut state = 0u8;

    for key in keys {
        state |= match key {
            Key::NumPad6 => KEMPSTON_RIGHT,
            Key::NumPad4 => KEMPSTON_LEFT,
            Key::NumPad2 => KEMPSTON_DOWN,
            Key::NumPad8 => KEMPSTON_UP,
            Key::NumPad0 | Key::LeftAlt | Key::RightAlt => KEMPSTON_FIRE,
            _ => 0,
        };
    }

    state
}

/// Map gamepad state to Kempston joystick state.
pub fn map_gamepad(gilrs: &Gilrs, gamepad_id: Option<GamepadId>) -> u8 {
    let Some(id) = gamepad_id else {
        return 0;
    };

    let Some(gamepad) = gilrs.connected_gamepad(id) else {
        return 0;
    };

    let mut state = 0u8;

    // D-pad
    if gamepad.is_pressed(Button::DPadRight) {
        state |= KEMPSTON_RIGHT;
    }
    if gamepad.is_pressed(Button::DPadLeft) {
        state |= KEMPSTON_LEFT;
    }
    if gamepad.is_pressed(Button::DPadDown) {
        state |= KEMPSTON_DOWN;
    }
    if gamepad.is_pressed(Button::DPadUp) {
        state |= KEMPSTON_UP;
    }

    // Left analog stick
    if let Some(axis) = gamepad.axis_data(Axis::LeftStickX) {
        if axis.value() > AXIS_THRESHOLD {
            state |= KEMPSTON_RIGHT;
        } else if axis.value() < -AXIS_THRESHOLD {
            state |= KEMPSTON_LEFT;
        }
    }
    if let Some(axis) = gamepad.axis_data(Axis::LeftStickY) {
        if axis.value() > AXIS_THRESHOLD {
            state |= KEMPSTON_UP;
        } else if axis.value() < -AXIS_THRESHOLD {
            state |= KEMPSTON_DOWN;
        }
    }

    // Fire buttons (A, B, X, Y, triggers, shoulder buttons)
    if gamepad.is_pressed(Button::South)      // A / Cross
        || gamepad.is_pressed(Button::East)   // B / Circle
        || gamepad.is_pressed(Button::West)   // X / Square
        || gamepad.is_pressed(Button::North)  // Y / Triangle
        || gamepad.is_pressed(Button::LeftTrigger)
        || gamepad.is_pressed(Button::RightTrigger)
        || gamepad.is_pressed(Button::LeftTrigger2)
        || gamepad.is_pressed(Button::RightTrigger2)
    {
        state |= KEMPSTON_FIRE;
    }

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_keys_returns_zero() {
        assert_eq!(map_keyboard(&[]), 0);
    }

    #[test]
    fn numpad_directions() {
        assert_eq!(map_keyboard(&[Key::NumPad6]), KEMPSTON_RIGHT);
        assert_eq!(map_keyboard(&[Key::NumPad4]), KEMPSTON_LEFT);
        assert_eq!(map_keyboard(&[Key::NumPad2]), KEMPSTON_DOWN);
        assert_eq!(map_keyboard(&[Key::NumPad8]), KEMPSTON_UP);
    }

    #[test]
    fn numpad_fire_buttons() {
        assert_eq!(map_keyboard(&[Key::NumPad0]), KEMPSTON_FIRE);
        assert_eq!(map_keyboard(&[Key::LeftAlt]), KEMPSTON_FIRE);
        assert_eq!(map_keyboard(&[Key::RightAlt]), KEMPSTON_FIRE);
    }

    #[test]
    fn diagonal_movement() {
        // Up-right
        assert_eq!(
            map_keyboard(&[Key::NumPad8, Key::NumPad6]),
            KEMPSTON_UP | KEMPSTON_RIGHT
        );
        // Down-left
        assert_eq!(
            map_keyboard(&[Key::NumPad2, Key::NumPad4]),
            KEMPSTON_DOWN | KEMPSTON_LEFT
        );
    }

    #[test]
    fn direction_with_fire() {
        assert_eq!(
            map_keyboard(&[Key::NumPad8, Key::NumPad0]),
            KEMPSTON_UP | KEMPSTON_FIRE
        );
    }

    #[test]
    fn all_directions_and_fire() {
        let all_keys = [
            Key::NumPad8,
            Key::NumPad2,
            Key::NumPad4,
            Key::NumPad6,
            Key::NumPad0,
        ];
        assert_eq!(
            map_keyboard(&all_keys),
            KEMPSTON_UP | KEMPSTON_DOWN | KEMPSTON_LEFT | KEMPSTON_RIGHT | KEMPSTON_FIRE
        );
    }

    #[test]
    fn irrelevant_keys_ignored() {
        // Keys that aren't joystick controls should not affect state
        assert_eq!(map_keyboard(&[Key::A, Key::Space, Key::Enter]), 0);
    }

    #[test]
    fn mixed_relevant_and_irrelevant_keys() {
        assert_eq!(
            map_keyboard(&[Key::A, Key::NumPad8, Key::Space]),
            KEMPSTON_UP
        );
    }

    #[test]
    fn kempston_bits_are_correct() {
        // Verify the bit positions match Kempston spec
        assert_eq!(KEMPSTON_RIGHT, 0b00001);
        assert_eq!(KEMPSTON_LEFT, 0b00010);
        assert_eq!(KEMPSTON_DOWN, 0b00100);
        assert_eq!(KEMPSTON_UP, 0b01000);
        assert_eq!(KEMPSTON_FIRE, 0b10000);
    }
}
