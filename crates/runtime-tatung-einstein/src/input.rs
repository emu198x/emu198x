//! Tatung Einstein keyboard input mapping.
//!
//! 8×8 matrix scanned through the AY-3-8910 I/O ports (row select on
//! port A, columns on port B). Every position below was probed against
//! the real X-TAL MOS ROM — press the cell, read the echoed character —
//! and cross-checked against MAME's `tatung/einstein.cpp` key matrix for
//! the non-printing keys (row 0). The donor's table was a placeholder and
//! did not match the hardware.

use emu198x_shell::InputEvent;
use machine_tatung_einstein::Einstein;

pub(crate) fn apply_input_event(machine: &mut Einstein, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((row, col)) = key_to_matrix(name.as_ref())
    {
        if *pressed {
            machine.press_key(row, col);
        } else {
            machine.release_key(row, col);
        }
    }
}

#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0 — non-printing keys (MAME LINE0).
        "return" | "enter" => (0, 5),
        "space" | " " => (0, 6),
        "escape" | "esc" => (0, 7),
        // Letters.
        "a" => (6, 6),
        "b" => (7, 2),
        "c" => (7, 4),
        "d" => (6, 4),
        "e" => (5, 4),
        "f" => (6, 3),
        "g" => (6, 2),
        "h" => (6, 1),
        "i" => (1, 0),
        "j" => (6, 0),
        "k" => (2, 0),
        "l" => (2, 1),
        "m" => (7, 0),
        "n" => (7, 1),
        "o" => (1, 1),
        "p" => (1, 2),
        "q" => (5, 6),
        "r" => (5, 3),
        "s" => (6, 5),
        "t" => (5, 2),
        "u" => (5, 0),
        "v" => (7, 3),
        "w" => (5, 5),
        "x" => (7, 5),
        "y" => (5, 1),
        "z" => (7, 6),
        // Digits.
        "0" => (1, 7),
        "1" => (4, 6),
        "2" => (4, 5),
        "3" => (4, 4),
        "4" => (4, 3),
        "5" => (4, 2),
        "6" => (4, 1),
        "7" => (4, 0),
        "8" => (3, 3),
        "9" => (2, 6),
        // Punctuation (unshifted legends).
        ";" | "semicolon" => (2, 2),
        ":" | "colon" => (2, 3),
        "," | "comma" => (3, 0),
        "." | "period" => (3, 1),
        "/" | "slash" => (3, 2),
        "=" | "equals" | "equal" => (3, 5),
        _ => return None,
    })
}
