//! Acorn Electron keyboard input mapping.
//!
//! The Electron has a 14-column × 4-row matrix that the ULA scans via
//! address lines. The machine exposes press/release by (col, row); the
//! runtime maps host-level key names to that pair.
//!
//! The mapping below is the standard Electron layout (sourced from the
//! BBC / Electron service manuals and the chip crate's internal
//! convention — the chip itself stores the matrix in (col, row) order).

use emu198x_shell::InputEvent;
use machine_acorn_electron::AcornElectron;

pub(crate) fn apply_input_event(machine: &mut AcornElectron, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((col, row)) = key_to_matrix(name.as_ref()) {
            if *pressed {
                machine.press_key(col, row);
            } else {
                machine.release_key(col, row);
            }
        }
}

#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, usize)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0 — control / function keys
        "shift" | "lshift" | "rshift" => (0, 0),
        "ctrl" | "control" => (1, 0),
        "return" | "enter" => (10, 0),
        "delete" | "del" | "backspace" | "bs" => (9, 0),
        "copy" => (11, 0),
        "down" | "arrowdown" => (12, 0),
        "right" | "arrowright" => (13, 0),
        // Row 1 — digits
        "1" => (0, 1),
        "2" => (1, 1),
        "3" => (2, 1),
        "4" => (3, 1),
        "5" => (4, 1),
        "6" => (5, 1),
        "7" => (6, 1),
        "8" => (7, 1),
        "9" => (8, 1),
        "0" => (9, 1),
        "-" | "minus" => (10, 1),
        "^" | "caret" => (11, 1),
        "up" | "arrowup" => (12, 1),
        "left" | "arrowleft" => (13, 1),
        // Row 2 — QWERTYUIOP
        "q" => (0, 2),
        "w" => (1, 2),
        "e" => (2, 2),
        "r" => (3, 2),
        "t" => (4, 2),
        "y" => (5, 2),
        "u" => (6, 2),
        "i" => (7, 2),
        "o" => (8, 2),
        "p" => (9, 2),
        "@" | "at" => (10, 2),
        "[" | "leftbracket" => (11, 2),
        "_" | "underscore" => (12, 2),
        // Row 3 — ASDFGHJKL
        "a" => (0, 3),
        "s" => (1, 3),
        "d" => (2, 3),
        "f" => (3, 3),
        "g" => (4, 3),
        "h" => (5, 3),
        "j" => (6, 3),
        "k" => (7, 3),
        "l" => (8, 3),
        ";" | "semicolon" => (9, 3),
        ":" => (10, 3),
        "]" | "rightbracket" => (11, 3),
        // Row 4 — ZXCVBNM
        "caps" | "capslock" => (0, 4),
        "z" => (1, 4),
        "x" => (2, 4),
        "c" => (3, 4),
        "v" => (4, 4),
        "b" => (5, 4),
        "n" => (6, 4),
        "m" => (7, 4),
        "," | "comma" => (8, 4),
        "." | "period" => (9, 4),
        "/" | "slash" => (10, 4),
        "space" | " " => (12, 4),
        _ => return None,
    })
}
