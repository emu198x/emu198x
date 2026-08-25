//! Acorn Electron keyboard input mapping.
//!
//! The Electron has a 14-column × 4-row matrix that the ULA scans via
//! address lines. The machine exposes press/release by (col, row); the
//! runtime maps host-level key names to that pair.
//!
//! The matrix is column-major: `(col, row)` = `(LINE, bit)`, where the
//! ULA selects column `LINE` by driving address bit `LINE` low and reads
//! the four rows back on D0-D3. The table below is transcribed from MAME's
//! `acorn/electron.cpp` `LINE.0`-`LINE.13` port definitions.

use emu198x_shell::{InputEvent, KeyTiming, KeyboardTarget, STANDARD_KEY_TIMING};
use machine_acorn_electron::AcornElectron;

pub(crate) fn apply_input_event(machine: &mut AcornElectron, event: &InputEvent) {
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
fn key_to_matrix(name: &str) -> Option<(usize, usize)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // LINE.0
        "right" | "arrowright" | "\\" | "backslash" => (0, 0),
        "copy" => (0, 1),
        "space" | " " => (0, 3),
        // LINE.1
        "left" | "arrowleft" => (1, 0),
        "down" | "arrowdown" => (1, 1),
        "return" | "enter" => (1, 2),
        "delete" | "del" | "backspace" | "bs" => (1, 3),
        // LINE.2
        "-" | "minus" => (2, 0),
        "up" | "arrowup" => (2, 1),
        ":" | "colon" => (2, 2),
        // LINE.3
        "0" => (3, 0),
        "p" => (3, 1),
        ";" | "semicolon" => (3, 2),
        "/" | "slash" => (3, 3),
        // LINE.4
        "9" => (4, 0),
        "o" => (4, 1),
        "l" => (4, 2),
        "." | "period" => (4, 3),
        // LINE.5
        "8" => (5, 0),
        "i" => (5, 1),
        "k" => (5, 2),
        "," | "comma" => (5, 3),
        // LINE.6
        "7" => (6, 0),
        "u" => (6, 1),
        "j" => (6, 2),
        "m" => (6, 3),
        // LINE.7
        "6" => (7, 0),
        "y" => (7, 1),
        "h" => (7, 2),
        "n" => (7, 3),
        // LINE.8
        "5" => (8, 0),
        "t" => (8, 1),
        "g" => (8, 2),
        "b" => (8, 3),
        // LINE.9
        "4" => (9, 0),
        "r" => (9, 1),
        "f" => (9, 2),
        "v" => (9, 3),
        // LINE.10
        "3" => (10, 0),
        "e" => (10, 1),
        "d" => (10, 2),
        "c" => (10, 3),
        // LINE.11
        "2" => (11, 0),
        "w" => (11, 1),
        "s" => (11, 2),
        "x" => (11, 3),
        // LINE.12
        "1" => (12, 0),
        "q" => (12, 1),
        "a" => (12, 2),
        "z" => (12, 3),
        // LINE.13 — modifiers / function
        "escape" | "esc" => (13, 0),
        "func" => (13, 1),
        "ctrl" | "control" => (13, 2),
        "shift" | "lshift" | "rshift" => (13, 3),
        _ => return None,
    })
}

/// The character an Electron keycap produces with SHIFT held, paired with the
/// key's own name.
///
/// The digit row carries `!"#$%&'()` above `1`-`9`, and the punctuation keys
/// carry a second legend each, as on the rest of the Acorn line. The standard
/// keyboard could reach none of them: it offers a symbol as its own
/// one-character name, the matrix has no key called `"`, and the input layer
/// dropped it after `type_string` had already counted it (#1196). `"` is how
/// you load a tape (`CHAIN""`).
///
/// Each pairing is checked on the machine — see the `shifted_legends_type_*`
/// tests, which put the characters through BASIC and read them back off the
/// screen.
const SHIFTED_LEGENDS: &[(char, &str)] = &[
    ('!', "1"),
    ('"', "2"),
    ('#', "3"),
    ('$', "4"),
    ('%', "5"),
    ('&', "6"),
    ('\'', "7"),
    ('(', "8"),
    (')', "9"),
    ('=', "-"),
    ('+', ";"),
    ('*', ":"),
    ('<', ","),
    ('>', "."),
    ('?', "/"),
];

/// The Electron's keyboard for the shared `press_key` / `type_string` tools.
///
/// Hand-written rather than the shared standard keyboard because the Electron
/// reaches much of printable ASCII through SHIFT, and because a character it
/// cannot type has to refuse rather than be counted and dropped (#916, #1196).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElectronKeyboard;

impl KeyboardTarget for ElectronKeyboard {
    fn key_name_is_valid(&self, name: &str) -> bool {
        key_to_matrix(name).is_some()
    }

    fn key_names_hint(&self) -> &'static str {
        "A-Z, 0-9, the punctuation on an Electron keyboard (including the \
         shifted legends such as \" * + = ? < >), space, enter, escape, \
         delete, shift, ctrl, func, and the arrow keys"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        let base = match ch {
            'a'..='z' | 'A'..='Z' => ch.to_ascii_lowercase().to_string(),
            '0'..='9' => ch.to_string(),
            ' ' => "space".to_owned(),
            '\n' | '\r' => "enter".to_owned(),
            _ => {
                if let Some((_, key)) = SHIFTED_LEGENDS.iter().find(|(c, _)| *c == ch) {
                    let key = (*key).to_owned();
                    return key_to_matrix(&key)
                        .is_some()
                        .then(|| vec!["shift".to_owned(), key]);
                }
                ch.to_string()
            }
        };
        key_to_matrix(&base).is_some().then(|| vec![base])
    }

    fn key_timing(&self) -> KeyTiming {
        STANDARD_KEY_TIMING
    }
}
