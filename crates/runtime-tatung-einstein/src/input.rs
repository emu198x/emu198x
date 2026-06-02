//! Tatung Einstein keyboard input mapping.
//!
//! The Einstein has an 8×8 matrix scanned via PSG port B. The matrix
//! layout below is sourced from the Einstein hardware reference; key
//! positions match the machine crate's internal scan convention.

use emu198x_shell::InputEvent;
use machine_tatung_einstein::Einstein;

pub(crate) fn apply_input_event(machine: &mut Einstein, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((row, col)) = key_to_matrix(name.as_ref()) {
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
        // Row 0: digits 0-7
        "0" => (0, 0),
        "1" => (0, 1),
        "2" => (0, 2),
        "3" => (0, 3),
        "4" => (0, 4),
        "5" => (0, 5),
        "6" => (0, 6),
        "7" => (0, 7),
        // Row 1: 8 9 - = ^ \ [ ]
        "8" => (1, 0),
        "9" => (1, 1),
        "-" | "minus" => (1, 2),
        "=" | "equals" => (1, 3),
        "^" | "caret" => (1, 4),
        "\\" | "backslash" => (1, 5),
        "[" | "leftbracket" => (1, 6),
        "]" | "rightbracket" => (1, 7),
        // Row 2
        "q" => (2, 0),
        "w" => (2, 1),
        "e" => (2, 2),
        "r" => (2, 3),
        "t" => (2, 4),
        "y" => (2, 5),
        "u" => (2, 6),
        "i" => (2, 7),
        // Row 3
        "o" => (3, 0),
        "p" => (3, 1),
        "@" | "at" => (3, 2),
        "return" | "enter" => (3, 3),
        "a" => (3, 4),
        "s" => (3, 5),
        "d" => (3, 6),
        "f" => (3, 7),
        // Row 4
        "g" => (4, 0),
        "h" => (4, 1),
        "j" => (4, 2),
        "k" => (4, 3),
        "l" => (4, 4),
        ";" | "semicolon" => (4, 5),
        ":" => (4, 6),
        "shift" | "lshift" | "rshift" => (4, 7),
        // Row 5
        "z" => (5, 0),
        "x" => (5, 1),
        "c" => (5, 2),
        "v" => (5, 3),
        "b" => (5, 4),
        "n" => (5, 5),
        "m" => (5, 6),
        "," | "comma" => (5, 7),
        // Row 6
        "." | "period" => (6, 0),
        "/" | "slash" => (6, 1),
        "space" | " " => (6, 2),
        "ctrl" | "control" => (6, 3),
        "tab" => (6, 4),
        "escape" | "esc" => (6, 5),
        "caps" | "capslock" => (6, 6),
        "delete" | "del" | "backspace" | "bs" => (6, 7),
        // Row 7
        "up" | "arrowup" => (7, 0),
        "down" | "arrowdown" => (7, 1),
        "left" | "arrowleft" => (7, 2),
        "right" | "arrowright" => (7, 3),
        "f1" => (7, 4),
        "f2" => (7, 5),
        "f3" => (7, 6),
        "f4" => (7, 7),
        _ => return None,
    })
}
