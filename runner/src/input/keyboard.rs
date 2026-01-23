//! Keyboard input handling for ZX Spectrum emulation.
//!
//! Maps PC keyboard keys to the Spectrum's 8x5 keyboard matrix.
//! Some keys (like Backspace, arrows) map to key combinations.

use winit::keyboard::KeyCode;

/// Map a PC key to Spectrum keyboard matrix positions.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_keys_map_to_single_position() {
        // Sample letters from each row
        assert_eq!(map_key(KeyCode::KeyZ), &[(0, 1)]);
        assert_eq!(map_key(KeyCode::KeyA), &[(1, 0)]);
        assert_eq!(map_key(KeyCode::KeyQ), &[(2, 0)]);
        assert_eq!(map_key(KeyCode::KeyP), &[(5, 0)]);
        assert_eq!(map_key(KeyCode::KeyM), &[(7, 2)]);
    }

    #[test]
    fn number_keys_map_correctly() {
        // Row 3: 1-5
        assert_eq!(map_key(KeyCode::Digit1), &[(3, 0)]);
        assert_eq!(map_key(KeyCode::Digit5), &[(3, 4)]);
        // Row 4: 6-0 (note reversed order in matrix)
        assert_eq!(map_key(KeyCode::Digit0), &[(4, 0)]);
        assert_eq!(map_key(KeyCode::Digit6), &[(4, 4)]);
    }

    #[test]
    fn shift_keys_map_to_caps_shift() {
        assert_eq!(map_key(KeyCode::ShiftLeft), &[(0, 0)]);
        assert_eq!(map_key(KeyCode::ShiftRight), &[(0, 0)]);
    }

    #[test]
    fn ctrl_keys_map_to_symbol_shift() {
        assert_eq!(map_key(KeyCode::ControlLeft), &[(7, 1)]);
        assert_eq!(map_key(KeyCode::ControlRight), &[(7, 1)]);
    }

    #[test]
    fn enter_maps_correctly() {
        assert_eq!(map_key(KeyCode::Enter), &[(6, 0)]);
    }

    #[test]
    fn space_maps_correctly() {
        assert_eq!(map_key(KeyCode::Space), &[(7, 0)]);
    }

    #[test]
    fn backspace_maps_to_caps_shift_plus_0() {
        let mapping = map_key(KeyCode::Backspace);
        assert_eq!(mapping.len(), 2);
        assert!(mapping.contains(&(0, 0))); // CAPS SHIFT
        assert!(mapping.contains(&(4, 0))); // 0
    }

    #[test]
    fn arrow_keys_map_to_caps_shift_combinations() {
        // Left = CAPS SHIFT + 5
        let left = map_key(KeyCode::ArrowLeft);
        assert_eq!(left.len(), 2);
        assert!(left.contains(&(0, 0)));
        assert!(left.contains(&(3, 4)));

        // Down = CAPS SHIFT + 6
        let down = map_key(KeyCode::ArrowDown);
        assert_eq!(down.len(), 2);
        assert!(down.contains(&(0, 0)));
        assert!(down.contains(&(4, 4)));

        // Up = CAPS SHIFT + 7
        let up = map_key(KeyCode::ArrowUp);
        assert_eq!(up.len(), 2);
        assert!(up.contains(&(0, 0)));
        assert!(up.contains(&(4, 3)));

        // Right = CAPS SHIFT + 8
        let right = map_key(KeyCode::ArrowRight);
        assert_eq!(right.len(), 2);
        assert!(right.contains(&(0, 0)));
        assert!(right.contains(&(4, 2)));
    }

    #[test]
    fn unknown_keys_return_empty() {
        assert_eq!(map_key(KeyCode::F1), &[]);
        assert_eq!(map_key(KeyCode::Tab), &[]);
        assert_eq!(map_key(KeyCode::Home), &[]);
    }

    #[test]
    fn all_rows_covered() {
        // Verify at least one key maps to each row 0-7
        assert_eq!(map_key(KeyCode::KeyZ)[0].0, 0);
        assert_eq!(map_key(KeyCode::KeyA)[0].0, 1);
        assert_eq!(map_key(KeyCode::KeyQ)[0].0, 2);
        assert_eq!(map_key(KeyCode::Digit1)[0].0, 3);
        assert_eq!(map_key(KeyCode::Digit0)[0].0, 4);
        assert_eq!(map_key(KeyCode::KeyP)[0].0, 5);
        assert_eq!(map_key(KeyCode::Enter)[0].0, 6);
        assert_eq!(map_key(KeyCode::Space)[0].0, 7);
    }
}
