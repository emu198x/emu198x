//! Mattel Aquarius keyboard input mapping.
//!
//! The Aquarius has an 8×6 keyboard matrix. Host `Key` events are
//! mapped to (row, col) via a name table and dispatched through
//! `Aquarius::set_key`. The matrix is active-low; the machine handles
//! state internally.
//!
//! Key naming follows the lowercase host convention used by Spectrum /
//! MSX runtimes (`return`, `space`, `shift`, letter / digit names).

use emu198x_shell::InputEvent;
use machine_mattel_aquarius::Aquarius;

pub(crate) fn apply_input_event(machine: &mut Aquarius, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((row, col)) = key_to_matrix(name.as_ref()) {
            machine.set_key(row, col, *pressed);
        }
}

/// Map a host-level key name to an Aquarius matrix (row, column).
///
/// Reference: Mattel Aquarius keyboard scancode table. Rows 0-7, columns
/// 0-5. Specific layout sourced from the machine crate's own internal
/// table; mirrored here for the host-facing translation.
#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0
        "=" | "equals" => (0, 0),
        "backspace" | "bs" => (0, 1),
        ":" => (0, 2),
        "return" | "enter" => (0, 3),
        ";" | "semicolon" => (0, 4),
        "." | "period" => (0, 5),
        // Row 1
        "-" | "minus" => (1, 0),
        "/" | "slash" => (1, 1),
        "0" => (1, 2),
        "p" => (1, 3),
        "l" => (1, 4),
        "," | "comma" => (1, 5),
        // Row 2
        "9" => (2, 0),
        "o" => (2, 1),
        "k" => (2, 2),
        "m" => (2, 3),
        "n" => (2, 4),
        "j" => (2, 5),
        // Row 3
        "8" => (3, 0),
        "i" => (3, 1),
        "7" => (3, 2),
        "u" => (3, 3),
        "h" => (3, 4),
        "b" => (3, 5),
        // Row 4
        "6" => (4, 0),
        "y" => (4, 1),
        "g" => (4, 2),
        "v" => (4, 3),
        "c" => (4, 4),
        "f" => (4, 5),
        // Row 5
        "5" => (5, 0),
        "t" => (5, 1),
        "4" => (5, 2),
        "r" => (5, 3),
        "d" => (5, 4),
        "x" => (5, 5),
        // Row 6
        "3" => (6, 0),
        "e" => (6, 1),
        "s" => (6, 2),
        "z" => (6, 3),
        "space" | " " => (6, 4),
        "a" => (6, 5),
        // Row 7
        "2" => (7, 0),
        "w" => (7, 1),
        "1" => (7, 2),
        "q" => (7, 3),
        "shift" | "lshift" | "rshift" => (7, 4),
        "ctrl" | "control" => (7, 5),
        _ => return None,
    })
}
