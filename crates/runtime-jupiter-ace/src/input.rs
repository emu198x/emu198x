//! Jupiter Ace keyboard input mapping.
//!
//! The machine core models the 8×5 matrix (identical to the ZX Spectrum)
//! and exposes press/release by [`JupiterAceKey`]. This module maps host
//! key names to those keys; the runtime calls it per host input event.

use emu198x_shell::InputEvent;
use machine_jupiter_ace::{JupiterAce, JupiterAceKey};

pub(crate) fn apply_input_event(machine: &mut JupiterAce, event: &InputEvent) {
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
fn key_from_name(name: &str) -> Option<JupiterAceKey> {
    use JupiterAceKey::*;
    Some(match name.to_ascii_lowercase().as_str() {
        "a" => A,
        "b" => B,
        "c" => C,
        "d" => D,
        "e" => E,
        "f" => F,
        "g" => G,
        "h" => H,
        "i" => I,
        "j" => J,
        "k" => K,
        "l" => L,
        "m" => M,
        "n" => N,
        "o" => O,
        "p" => P,
        "q" => Q,
        "r" => R,
        "s" => S,
        "t" => T,
        "u" => U,
        "v" => V,
        "w" => W,
        "x" => X,
        "y" => Y,
        "z" => Z,
        "0" => N0,
        "1" => N1,
        "2" => N2,
        "3" => N3,
        "4" => N4,
        "5" => N5,
        "6" => N6,
        "7" => N7,
        "8" => N8,
        "9" => N9,
        "enter" | "return" => Enter,
        "space" | " " => Space,
        // The Ace has two shift keys, matching the Spectrum: the red
        // Symbol Shift and the Caps/Shift.
        "shift" | "caps" | "capsshift" | "lshift" | "rshift" => Shift,
        "symbol" | "symbolshift" | "ctrl" | "control" => SymbolShift,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_digits_and_controls() {
        assert_eq!(key_from_name("a"), Some(JupiterAceKey::A));
        assert_eq!(key_from_name("Z"), Some(JupiterAceKey::Z)); // case-insensitive
        assert_eq!(key_from_name("5"), Some(JupiterAceKey::N5));
        assert_eq!(key_from_name("enter"), Some(JupiterAceKey::Enter));
        assert_eq!(key_from_name(" "), Some(JupiterAceKey::Space));
        assert_eq!(key_from_name("symbol"), Some(JupiterAceKey::SymbolShift));
    }

    #[test]
    fn unmapped_key_returns_none() {
        assert_eq!(key_from_name("f1"), None);
    }
}
