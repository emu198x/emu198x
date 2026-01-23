//! Keyboard input handling for ZX Spectrum emulation.
//!
//! Maps PC keyboard keys to the Spectrum's 8x5 keyboard matrix.
//! Some keys (like Backspace, arrows) map to key combinations.

use minifb::Key;

/// Map a PC key to Spectrum keyboard matrix positions.
///
/// Returns a slice of (row, bit) pairs. Most keys map to one position,
/// but some (like Backspace = CAPS SHIFT + 0) require multiple.
pub fn map_key(key: Key) -> &'static [(usize, u8)] {
    match key {
        // Row 0: CAPS SHIFT, Z, X, C, V
        Key::LeftShift | Key::RightShift => &[(0, 0)],
        Key::Z => &[(0, 1)],
        Key::X => &[(0, 2)],
        Key::C => &[(0, 3)],
        Key::V => &[(0, 4)],

        // Row 1: A, S, D, F, G
        Key::A => &[(1, 0)],
        Key::S => &[(1, 1)],
        Key::D => &[(1, 2)],
        Key::F => &[(1, 3)],
        Key::G => &[(1, 4)],

        // Row 2: Q, W, E, R, T
        Key::Q => &[(2, 0)],
        Key::W => &[(2, 1)],
        Key::E => &[(2, 2)],
        Key::R => &[(2, 3)],
        Key::T => &[(2, 4)],

        // Row 3: 1, 2, 3, 4, 5
        Key::Key1 => &[(3, 0)],
        Key::Key2 => &[(3, 1)],
        Key::Key3 => &[(3, 2)],
        Key::Key4 => &[(3, 3)],
        Key::Key5 => &[(3, 4)],

        // Row 4: 0, 9, 8, 7, 6
        Key::Key0 => &[(4, 0)],
        Key::Key9 => &[(4, 1)],
        Key::Key8 => &[(4, 2)],
        Key::Key7 => &[(4, 3)],
        Key::Key6 => &[(4, 4)],

        // Row 5: P, O, I, U, Y
        Key::P => &[(5, 0)],
        Key::O => &[(5, 1)],
        Key::I => &[(5, 2)],
        Key::U => &[(5, 3)],
        Key::Y => &[(5, 4)],

        // Row 6: ENTER, L, K, J, H
        Key::Enter => &[(6, 0)],
        Key::L => &[(6, 1)],
        Key::K => &[(6, 2)],
        Key::J => &[(6, 3)],
        Key::H => &[(6, 4)],

        // Row 7: SPACE, SYMBOL SHIFT, M, N, B
        Key::Space => &[(7, 0)],
        Key::LeftCtrl | Key::RightCtrl => &[(7, 1)], // Symbol shift
        Key::M => &[(7, 2)],
        Key::N => &[(7, 3)],
        Key::B => &[(7, 4)],

        // Compound keys (accent keys that require CAPS SHIFT + another key)
        Key::Backspace => &[(0, 0), (4, 0)], // CAPS SHIFT + 0 = DELETE
        Key::Left => &[(0, 0), (3, 4)],      // CAPS SHIFT + 5
        Key::Down => &[(0, 0), (4, 4)],      // CAPS SHIFT + 6
        Key::Up => &[(0, 0), (4, 3)],        // CAPS SHIFT + 7
        Key::Right => &[(0, 0), (4, 2)],     // CAPS SHIFT + 8

        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_keys_map_to_single_position() {
        // Sample letters from each row
        assert_eq!(map_key(Key::Z), &[(0, 1)]);
        assert_eq!(map_key(Key::A), &[(1, 0)]);
        assert_eq!(map_key(Key::Q), &[(2, 0)]);
        assert_eq!(map_key(Key::P), &[(5, 0)]);
        assert_eq!(map_key(Key::M), &[(7, 2)]);
    }

    #[test]
    fn number_keys_map_correctly() {
        // Row 3: 1-5
        assert_eq!(map_key(Key::Key1), &[(3, 0)]);
        assert_eq!(map_key(Key::Key5), &[(3, 4)]);
        // Row 4: 6-0 (note reversed order in matrix)
        assert_eq!(map_key(Key::Key0), &[(4, 0)]);
        assert_eq!(map_key(Key::Key6), &[(4, 4)]);
    }

    #[test]
    fn shift_keys_map_to_caps_shift() {
        assert_eq!(map_key(Key::LeftShift), &[(0, 0)]);
        assert_eq!(map_key(Key::RightShift), &[(0, 0)]);
    }

    #[test]
    fn ctrl_keys_map_to_symbol_shift() {
        assert_eq!(map_key(Key::LeftCtrl), &[(7, 1)]);
        assert_eq!(map_key(Key::RightCtrl), &[(7, 1)]);
    }

    #[test]
    fn enter_maps_correctly() {
        assert_eq!(map_key(Key::Enter), &[(6, 0)]);
    }

    #[test]
    fn space_maps_correctly() {
        assert_eq!(map_key(Key::Space), &[(7, 0)]);
    }

    #[test]
    fn backspace_maps_to_caps_shift_plus_0() {
        let mapping = map_key(Key::Backspace);
        assert_eq!(mapping.len(), 2);
        assert!(mapping.contains(&(0, 0))); // CAPS SHIFT
        assert!(mapping.contains(&(4, 0))); // 0
    }

    #[test]
    fn arrow_keys_map_to_caps_shift_combinations() {
        // Left = CAPS SHIFT + 5
        let left = map_key(Key::Left);
        assert_eq!(left.len(), 2);
        assert!(left.contains(&(0, 0)));
        assert!(left.contains(&(3, 4)));

        // Down = CAPS SHIFT + 6
        let down = map_key(Key::Down);
        assert_eq!(down.len(), 2);
        assert!(down.contains(&(0, 0)));
        assert!(down.contains(&(4, 4)));

        // Up = CAPS SHIFT + 7
        let up = map_key(Key::Up);
        assert_eq!(up.len(), 2);
        assert!(up.contains(&(0, 0)));
        assert!(up.contains(&(4, 3)));

        // Right = CAPS SHIFT + 8
        let right = map_key(Key::Right);
        assert_eq!(right.len(), 2);
        assert!(right.contains(&(0, 0)));
        assert!(right.contains(&(4, 2)));
    }

    #[test]
    fn unknown_keys_return_empty() {
        assert_eq!(map_key(Key::F1), &[]);
        assert_eq!(map_key(Key::Tab), &[]);
        assert_eq!(map_key(Key::Home), &[]);
    }

    #[test]
    fn all_rows_covered() {
        // Verify at least one key maps to each row 0-7
        assert_eq!(map_key(Key::Z)[0].0, 0);
        assert_eq!(map_key(Key::A)[0].0, 1);
        assert_eq!(map_key(Key::Q)[0].0, 2);
        assert_eq!(map_key(Key::Key1)[0].0, 3);
        assert_eq!(map_key(Key::Key0)[0].0, 4);
        assert_eq!(map_key(Key::P)[0].0, 5);
        assert_eq!(map_key(Key::Enter)[0].0, 6);
        assert_eq!(map_key(Key::Space)[0].0, 7);
    }
}
