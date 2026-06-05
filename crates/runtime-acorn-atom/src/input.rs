//! Acorn Atom keyboard input mapping.
//!
//! The machine core already models the 8255-scanned matrix and exposes
//! press/release by [`AtomKey`] (whose `(row, col)` positions were probed
//! against the real MOS ROM). This module maps host key names to those
//! keys and is the bridge the runtime calls per host input event.

use emu198x_shell::InputEvent;
use machine_acorn_atom::{AcornAtom, AtomKey};

pub(crate) fn apply_input_event(machine: &mut AcornAtom, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some(key) = key_from_name(name.as_ref())
    {
        if *pressed {
            machine.press_key(key);
        } else {
            machine.release_key(key);
        }
    }
}

#[must_use]
fn key_from_name(name: &str) -> Option<AtomKey> {
    Some(match name.to_ascii_lowercase().as_str() {
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
        "," | "comma" => AtomKey::Comma,
        ";" | "semicolon" => AtomKey::Semicolon,
        ":" | "colon" => AtomKey::Colon,
        "." | "period" => AtomKey::Period,
        "/" | "slash" => AtomKey::Slash,
        "@" | "at" => AtomKey::At,
        "return" | "enter" => AtomKey::Return,
        "space" | " " => AtomKey::Space,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_digits_and_controls() {
        assert_eq!(key_from_name("a"), Some(AtomKey::A));
        assert_eq!(key_from_name("Z"), Some(AtomKey::Z)); // case-insensitive
        assert_eq!(key_from_name("3"), Some(AtomKey::Num3));
        assert_eq!(key_from_name("return"), Some(AtomKey::Return));
        assert_eq!(key_from_name(" "), Some(AtomKey::Space));
        assert_eq!(key_from_name("@"), Some(AtomKey::At));
    }

    #[test]
    fn unmapped_key_returns_none() {
        assert_eq!(key_from_name("f1"), None);
    }
}
