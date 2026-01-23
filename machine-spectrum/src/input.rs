//! Input handling for ZX Spectrum emulation.
//!
//! Handles mapping from generic KeyCode to the Spectrum's 8x5 keyboard matrix
//! and Kempston joystick format.

use emu_core::{JoystickState, KeyCode};

/// Kempston joystick bit positions (active high).
pub const KEMPSTON_RIGHT: u8 = 0x01;
pub const KEMPSTON_LEFT: u8 = 0x02;
pub const KEMPSTON_DOWN: u8 = 0x04;
pub const KEMPSTON_UP: u8 = 0x08;
pub const KEMPSTON_FIRE: u8 = 0x10;

/// Map a key to Spectrum keyboard matrix positions.
///
/// Returns a slice of (row, bit) pairs. Most keys map to one position,
/// but some (like Backspace = CAPS SHIFT + 0) require multiple.
pub fn map_key(key: KeyCode) -> &'static [(usize, u8)] {
    match key {
        // Row 0: CAPS SHIFT, Z, X, C, V
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &[(0, 0)],
        KeyCode::KeyZ => &[(0, 1)],
        KeyCode::KeyX => &[(0, 2)],
        KeyCode::KeyC => &[(0, 3)],
        KeyCode::KeyV => &[(0, 4)],

        // Row 1: A, S, D, F, G
        KeyCode::KeyA => &[(1, 0)],
        KeyCode::KeyS => &[(1, 1)],
        KeyCode::KeyD => &[(1, 2)],
        KeyCode::KeyF => &[(1, 3)],
        KeyCode::KeyG => &[(1, 4)],

        // Row 2: Q, W, E, R, T
        KeyCode::KeyQ => &[(2, 0)],
        KeyCode::KeyW => &[(2, 1)],
        KeyCode::KeyE => &[(2, 2)],
        KeyCode::KeyR => &[(2, 3)],
        KeyCode::KeyT => &[(2, 4)],

        // Row 3: 1, 2, 3, 4, 5
        KeyCode::Digit1 => &[(3, 0)],
        KeyCode::Digit2 => &[(3, 1)],
        KeyCode::Digit3 => &[(3, 2)],
        KeyCode::Digit4 => &[(3, 3)],
        KeyCode::Digit5 => &[(3, 4)],

        // Row 4: 0, 9, 8, 7, 6
        KeyCode::Digit0 => &[(4, 0)],
        KeyCode::Digit9 => &[(4, 1)],
        KeyCode::Digit8 => &[(4, 2)],
        KeyCode::Digit7 => &[(4, 3)],
        KeyCode::Digit6 => &[(4, 4)],

        // Row 5: P, O, I, U, Y
        KeyCode::KeyP => &[(5, 0)],
        KeyCode::KeyO => &[(5, 1)],
        KeyCode::KeyI => &[(5, 2)],
        KeyCode::KeyU => &[(5, 3)],
        KeyCode::KeyY => &[(5, 4)],

        // Row 6: ENTER, L, K, J, H
        KeyCode::Enter => &[(6, 0)],
        KeyCode::KeyL => &[(6, 1)],
        KeyCode::KeyK => &[(6, 2)],
        KeyCode::KeyJ => &[(6, 3)],
        KeyCode::KeyH => &[(6, 4)],

        // Row 7: SPACE, SYMBOL SHIFT, M, N, B
        KeyCode::Space => &[(7, 0)],
        KeyCode::ControlLeft | KeyCode::ControlRight => &[(7, 1)], // Symbol shift
        KeyCode::KeyM => &[(7, 2)],
        KeyCode::KeyN => &[(7, 3)],
        KeyCode::KeyB => &[(7, 4)],

        // Compound keys (accent keys that require CAPS SHIFT + another key)
        KeyCode::Backspace => &[(0, 0), (4, 0)], // CAPS SHIFT + 0 = DELETE
        KeyCode::ArrowLeft => &[(0, 0), (3, 4)], // CAPS SHIFT + 5
        KeyCode::ArrowDown => &[(0, 0), (4, 4)], // CAPS SHIFT + 6
        KeyCode::ArrowUp => &[(0, 0), (4, 3)],   // CAPS SHIFT + 7
        KeyCode::ArrowRight => &[(0, 0), (4, 2)], // CAPS SHIFT + 8

        _ => &[],
    }
}

/// Convert generic joystick state to Kempston format.
pub fn joystick_to_kempston(state: JoystickState) -> u8 {
    let mut kempston = 0u8;
    if state.right {
        kempston |= KEMPSTON_RIGHT;
    }
    if state.left {
        kempston |= KEMPSTON_LEFT;
    }
    if state.down {
        kempston |= KEMPSTON_DOWN;
    }
    if state.up {
        kempston |= KEMPSTON_UP;
    }
    if state.fire || state.fire2 {
        kempston |= KEMPSTON_FIRE;
    }
    kempston
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_keys_map_to_single_position() {
        assert_eq!(map_key(KeyCode::KeyZ), &[(0, 1)]);
        assert_eq!(map_key(KeyCode::KeyA), &[(1, 0)]);
        assert_eq!(map_key(KeyCode::KeyQ), &[(2, 0)]);
        assert_eq!(map_key(KeyCode::KeyP), &[(5, 0)]);
        assert_eq!(map_key(KeyCode::KeyM), &[(7, 2)]);
    }

    #[test]
    fn number_keys_map_correctly() {
        assert_eq!(map_key(KeyCode::Digit1), &[(3, 0)]);
        assert_eq!(map_key(KeyCode::Digit5), &[(3, 4)]);
        assert_eq!(map_key(KeyCode::Digit0), &[(4, 0)]);
        assert_eq!(map_key(KeyCode::Digit6), &[(4, 4)]);
    }

    #[test]
    fn backspace_maps_to_caps_shift_plus_0() {
        let mapping = map_key(KeyCode::Backspace);
        assert_eq!(mapping.len(), 2);
        assert!(mapping.contains(&(0, 0))); // CAPS SHIFT
        assert!(mapping.contains(&(4, 0))); // 0
    }

    #[test]
    fn joystick_conversion() {
        let state = JoystickState {
            up: true,
            right: true,
            fire: true,
            ..Default::default()
        };
        let kempston = joystick_to_kempston(state);
        assert_eq!(kempston, KEMPSTON_UP | KEMPSTON_RIGHT | KEMPSTON_FIRE);
    }
}
