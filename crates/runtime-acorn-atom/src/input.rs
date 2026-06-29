//! Acorn Atom keyboard input mapping.
//!
//! The machine core models the 8255-scanned matrix and exposes press/release by
//! [`AtomKey`] (whose `(row, col)` positions were probed against the real MOS
//! ROM). This module maps host key names to those keys and is the bridge the
//! runtime calls per host input event.
//!
//! The Atom places its symbols on shifted keys, like a typewriter — `*` is
//! SHIFT+`:`, `+` is SHIFT+`;`, `"` is SHIFT+`2`, and so on (all probed against
//! the MOS). [`key_from_name`] resolves each name to a base key plus whether
//! SHIFT is held, and [`apply_input_event`] brackets the base key with SHIFT.

use emu198x_shell::InputEvent;
use machine_acorn_atom::{AcornAtom, AtomKey};

pub(crate) fn apply_input_event(machine: &mut AcornAtom, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some((key, shifted)) = key_from_name(name.as_ref())
    {
        if *pressed {
            if shifted {
                machine.press_key(AtomKey::Shift);
            }
            machine.press_key(key);
        } else {
            machine.release_key(key);
            if shifted {
                machine.release_key(AtomKey::Shift);
            }
        }
    }
}

/// Resolve a host key name to the Atom key and whether SHIFT is held.
#[must_use]
fn key_from_name(name: &str) -> Option<(AtomKey, bool)> {
    let unshifted = match name.to_ascii_lowercase().as_str() {
        "a" => AtomKey::A,
        "b" => AtomKey::B,
        "c" => AtomKey::C,
        "d" => AtomKey::D,
        "e" => AtomKey::E,
        "f" => AtomKey::F,
        "g" => AtomKey::G,
        "h" => AtomKey::H,
        "i" => AtomKey::I,
        "j" => AtomKey::J,
        "k" => AtomKey::K,
        "l" => AtomKey::L,
        "m" => AtomKey::M,
        "n" => AtomKey::N,
        "o" => AtomKey::O,
        "p" => AtomKey::P,
        "q" => AtomKey::Q,
        "r" => AtomKey::R,
        "s" => AtomKey::S,
        "t" => AtomKey::T,
        "u" => AtomKey::U,
        "v" => AtomKey::V,
        "w" => AtomKey::W,
        "x" => AtomKey::X,
        "y" => AtomKey::Y,
        "z" => AtomKey::Z,
        "0" => AtomKey::Num0,
        "1" => AtomKey::Num1,
        "2" => AtomKey::Num2,
        "3" => AtomKey::Num3,
        "4" => AtomKey::Num4,
        "5" => AtomKey::Num5,
        "6" => AtomKey::Num6,
        "7" => AtomKey::Num7,
        "8" => AtomKey::Num8,
        "9" => AtomKey::Num9,
        // unshifted punctuation (base keys)
        "," | "comma" => AtomKey::Comma,
        ";" | "semicolon" => AtomKey::Semicolon,
        ":" | "colon" => AtomKey::Colon,
        "." | "period" => AtomKey::Period,
        "/" | "slash" => AtomKey::Slash,
        "@" | "at" => AtomKey::At,
        "-" | "minus" => AtomKey::Minus,
        "[" | "leftbracket" => AtomKey::LeftBracket,
        "]" | "rightbracket" => AtomKey::RightBracket,
        "\\" | "backslash" => AtomKey::Backslash,
        // The Atom draws ASCII 0x5E as `↑`; modern keyboards type it as `^`.
        "^" | "caret" | "uparrow" => AtomKey::UpArrow,
        "return" | "enter" => AtomKey::Return,
        "space" | " " => AtomKey::Space,
        "delete" | "backspace" | "del" => AtomKey::Delete,
        // The unshifted directions of the two bidirectional cursor keys.
        "up" | "cursorup" | "arrowup" => AtomKey::CursorUpDown,
        "right" | "cursorright" | "arrowright" => AtomKey::CursorLeftRight,
        "escape" | "esc" => AtomKey::Escape,
        "lock" | "capslock" | "shiftlock" => AtomKey::Lock,
        "shift" | "lshift" | "rshift" => AtomKey::Shift,
        "ctrl" | "control" | "lctrl" | "rctrl" => AtomKey::Ctrl,
        "rept" | "repeat" => AtomKey::Rept,
        // Symbols on shifted keys → SHIFT + base key (probed against the MOS).
        "*" | "asterisk" | "star" => return Some((AtomKey::Colon, true)),
        "+" | "plus" => return Some((AtomKey::Semicolon, true)),
        "<" | "less" => return Some((AtomKey::Comma, true)),
        "=" | "equals" => return Some((AtomKey::Minus, true)),
        ">" | "greater" => return Some((AtomKey::Period, true)),
        "?" | "question" => return Some((AtomKey::Slash, true)),
        "!" => return Some((AtomKey::Num1, true)),
        "\"" | "doublequote" => return Some((AtomKey::Num2, true)),
        "#" | "hash" => return Some((AtomKey::Num3, true)),
        "$" => return Some((AtomKey::Num4, true)),
        "%" => return Some((AtomKey::Num5, true)),
        "&" => return Some((AtomKey::Num6, true)),
        "'" | "apostrophe" => return Some((AtomKey::Num7, true)),
        "(" => return Some((AtomKey::Num8, true)),
        ")" => return Some((AtomKey::Num9, true)),
        // DOWN and LEFT are the SHIFT-reversed directions of the cursor keys.
        "down" | "cursordown" | "arrowdown" => return Some((AtomKey::CursorUpDown, true)),
        "left" | "cursorleft" | "arrowleft" => return Some((AtomKey::CursorLeftRight, true)),
        _ => return None,
    };
    Some((unshifted, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_digits_and_controls() {
        assert_eq!(key_from_name("a"), Some((AtomKey::A, false)));
        assert_eq!(key_from_name("Z"), Some((AtomKey::Z, false))); // case-insensitive
        assert_eq!(key_from_name("3"), Some((AtomKey::Num3, false)));
        assert_eq!(key_from_name("return"), Some((AtomKey::Return, false)));
        assert_eq!(key_from_name(" "), Some((AtomKey::Space, false)));
        assert_eq!(key_from_name("@"), Some((AtomKey::At, false)));
    }

    #[test]
    fn maps_new_base_keys() {
        assert_eq!(key_from_name("-"), Some((AtomKey::Minus, false)));
        assert_eq!(key_from_name("["), Some((AtomKey::LeftBracket, false)));
        assert_eq!(key_from_name("]"), Some((AtomKey::RightBracket, false)));
        assert_eq!(key_from_name("\\"), Some((AtomKey::Backslash, false)));
        assert_eq!(key_from_name("^"), Some((AtomKey::UpArrow, false)));
    }

    #[test]
    fn shifted_symbols_resolve_to_shift_plus_base() {
        // `*` is SHIFT+`:` — the key needed for COS commands like *LOAD.
        assert_eq!(key_from_name("*"), Some((AtomKey::Colon, true)));
        assert_eq!(key_from_name("star"), Some((AtomKey::Colon, true)));
        assert_eq!(key_from_name("+"), Some((AtomKey::Semicolon, true)));
        assert_eq!(key_from_name("\""), Some((AtomKey::Num2, true)));
        assert_eq!(key_from_name("="), Some((AtomKey::Minus, true)));
        assert_eq!(key_from_name("?"), Some((AtomKey::Slash, true)));
    }

    #[test]
    fn cursor_directions_map_to_two_bidirectional_keys() {
        // Up/right are unshifted; down/left are the SHIFT-reversed directions.
        assert_eq!(key_from_name("up"), Some((AtomKey::CursorUpDown, false)));
        assert_eq!(key_from_name("down"), Some((AtomKey::CursorUpDown, true)));
        assert_eq!(
            key_from_name("right"),
            Some((AtomKey::CursorLeftRight, false))
        );
        assert_eq!(
            key_from_name("left"),
            Some((AtomKey::CursorLeftRight, true))
        );
        assert_eq!(key_from_name("delete"), Some((AtomKey::Delete, false)));
    }

    #[test]
    fn editing_keys_map() {
        assert_eq!(key_from_name("escape"), Some((AtomKey::Escape, false)));
        assert_eq!(key_from_name("lock"), Some((AtomKey::Lock, false)));
        assert_eq!(key_from_name("rept"), Some((AtomKey::Rept, false)));
    }

    #[test]
    fn unmapped_key_returns_none() {
        assert_eq!(key_from_name("f1"), None);
    }
}
