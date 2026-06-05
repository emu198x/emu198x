//! Oric-1 / Atmos keyboard input mapping.
//!
//! The Oric keyboard is an 8×8 matrix: VIA port B bits 0-2 select the
//! column, the scan routine drives one row low on VIA port A, and the
//! sense returns on VIA PB3. The machine exposes press/release by
//! `(col, row)`; this module maps host key names to that pair.
//!
//! Every position was probed against the real Oric BASIC ROM (press the
//! cell, read the echoed glyph from screen RAM) and cross-checked against
//! MAME's `tangerine/oric.cpp` `ROW0`-`ROW7` ports.

use emu198x_shell::InputEvent;
use machine_oric_atmos::OricAtmos;

pub(crate) fn apply_input_event(machine: &mut OricAtmos, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((col, row)) = key_to_matrix(name.as_ref())
    {
        if *pressed {
            machine.press_key(col, row);
        } else {
            machine.release_key(col, row);
        }
    }
}

#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Letters.
        "a" => (6, 5),
        "b" => (2, 2),
        "c" => (2, 7),
        "d" => (1, 7),
        "e" => (6, 3),
        "f" => (1, 3),
        "g" => (6, 2),
        "h" => (6, 1),
        "i" => (5, 1),
        "j" => (1, 0),
        "k" => (3, 0),
        "l" => (7, 1),
        "m" => (2, 0),
        "n" => (0, 1),
        "o" => (5, 2),
        "p" => (5, 3),
        "q" => (1, 6),
        "r" => (1, 2),
        "s" => (6, 6),
        "t" => (1, 1),
        "u" => (5, 0),
        "v" => (0, 3),
        "w" => (6, 7),
        "x" => (0, 6),
        "y" => (6, 0),
        "z" => (2, 5),
        // Digits.
        "0" => (7, 2),
        "1" => (0, 5),
        "2" => (2, 6),
        "3" => (0, 7),
        "4" => (2, 3),
        "5" => (0, 2),
        "6" => (2, 1),
        "7" => (0, 0),
        "8" => (7, 0),
        "9" => (3, 1),
        // Punctuation (unshifted legends).
        "," | "comma" => (4, 1),
        "." | "period" | "stop" => (4, 2),
        ";" | "semicolon" | ":" | "colon" => (3, 2),
        "-" | "minus" => (3, 3),
        "'" | "quote" | "apostrophe" => (3, 7),
        "\\" | "backslash" => (3, 6),
        "/" | "slash" => (7, 3),
        "=" | "equals" | "equal" => (7, 7),
        "[" | "leftbracket" | "openbracket" => (5, 7),
        "]" | "rightbracket" | "closebracket" => (5, 6),
        // Control / editing.
        "space" | " " => (4, 0),
        "return" | "enter" => (7, 5),
        "shift" | "lshift" => (4, 4),
        "rshift" => (7, 4),
        "ctrl" | "control" => (2, 4),
        "delete" | "del" | "backspace" | "bs" => (5, 5),
        "left" | "arrowleft" => (4, 5),
        "right" | "arrowright" => (4, 7),
        "up" | "arrowup" => (4, 3),
        "down" | "arrowdown" => (4, 6),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ground_truthed_cells() {
        // Verified by typing HELLO against the real Oric ROM.
        assert_eq!(key_to_matrix("h"), Some((6, 1)));
        assert_eq!(key_to_matrix("e"), Some((6, 3)));
        assert_eq!(key_to_matrix("l"), Some((7, 1)));
        assert_eq!(key_to_matrix("o"), Some((5, 2)));
        assert_eq!(key_to_matrix("return"), Some((7, 5)));
        assert_eq!(key_to_matrix("SPACE"), Some((4, 0))); // case-insensitive
    }

    #[test]
    fn unmapped_key_returns_none() {
        assert_eq!(key_to_matrix("f1"), None);
    }
}
