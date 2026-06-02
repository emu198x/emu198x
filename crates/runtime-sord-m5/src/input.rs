//! Sord M5 keyboard input mapping.
//!
//! The M5 has a 7-row × 7-bit matrix scanned through I/O port `$30`.
//! Host-level key names map to (row, bit) via this table. The matrix
//! layout is sourced from the M5 service manual; key positions match
//! the machine crate's internal scan convention.

use emu198x_shell::InputEvent;
use machine_sord_m5::SordM5;

pub(crate) fn apply_input_event(machine: &mut SordM5, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((row, bit)) = key_to_matrix(name.as_ref())
    {
        if *pressed {
            machine.press_key(row, bit);
        } else {
            machine.release_key(row, bit);
        }
    }
}

#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0: digits
        "1" => (0, 0),
        "2" => (0, 1),
        "3" => (0, 2),
        "4" => (0, 3),
        "5" => (0, 4),
        "6" => (0, 5),
        "7" => (0, 6),
        // Row 1
        "8" => (1, 0),
        "9" => (1, 1),
        "0" => (1, 2),
        "-" | "minus" => (1, 3),
        "^" | "caret" => (1, 4),
        "\\" | "yen" | "backslash" => (1, 5),
        "delete" | "del" | "backspace" | "bs" => (1, 6),
        // Row 2
        "q" => (2, 0),
        "w" => (2, 1),
        "e" => (2, 2),
        "r" => (2, 3),
        "t" => (2, 4),
        "y" => (2, 5),
        "u" => (2, 6),
        // Row 3
        "i" => (3, 0),
        "o" => (3, 1),
        "p" => (3, 2),
        "@" | "at" => (3, 3),
        "[" | "leftbracket" => (3, 4),
        "return" | "enter" => (3, 5),
        "a" => (3, 6),
        // Row 4
        "s" => (4, 0),
        "d" => (4, 1),
        "f" => (4, 2),
        "g" => (4, 3),
        "h" => (4, 4),
        "j" => (4, 5),
        "k" => (4, 6),
        // Row 5
        "l" => (5, 0),
        ";" | "semicolon" => (5, 1),
        ":" => (5, 2),
        "]" | "rightbracket" => (5, 3),
        "shift" | "lshift" | "rshift" => (5, 4),
        "z" => (5, 5),
        "x" => (5, 6),
        // Row 6
        "c" => (6, 0),
        "v" => (6, 1),
        "b" => (6, 2),
        "n" => (6, 3),
        "m" => (6, 4),
        "," | "comma" => (6, 5),
        "." | "period" => (6, 6),
        _ => return None,
    })
}
