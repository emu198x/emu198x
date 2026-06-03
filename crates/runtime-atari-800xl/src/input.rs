//! Atari 800XL keyboard input mapping.
//!
//! Unlike a scanned key matrix, the Atari keyboard is read by POKEY, which
//! presents a single KBCODE register (bits 0-5 = key, bit 6 = Ctrl, bit 7 =
//! Shift) plus a keyboard interrupt. So a host key name maps to one complete
//! scan code, with Shift baked in only for the shifted *symbols*. The OS
//! converts KBCODE to ATASCII, applying SHFLOK (the caps-lock shadow) to set
//! letter case — so letters arrive uppercase on a freshly-booted machine,
//! which is the normal Atari BASIC experience, and both `"A"` and `"a"` map
//! to the same shift-less code.
//!
//! Scan codes are taken from the OS keyboard table (XL OS ROM, $FB51): the
//! unshifted table for the base codes, the shift table for the shifted
//! symbols. Key names are single characters (the literal character to type)
//! plus a few multi-letter names for non-printing keys.

use emu198x_shell::InputEvent;
use machine_atari_800xl::Atari800xl;

/// Shift modifier bit in a POKEY scan code.
const SHIFT: u8 = 0x80;

/// Apply one host input event. Key presses latch a POKEY scan code and raise
/// the keyboard interrupt; releases clear the "key down" status. Joystick and
/// other event kinds are handled elsewhere / ignored here.
pub(crate) fn apply_input_event(machine: &mut Atari800xl, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event {
        let Some(code) = key_scancode(name.as_ref()) else {
            return;
        };
        if *pressed {
            machine.press_key(code);
        } else {
            machine.release_key();
        }
    }
}

/// Map a host key name to a complete POKEY scan code (with Shift baked in).
/// Single-character names are the literal character to type; multi-letter
/// names cover the non-printing keys. Returns `None` for unmapped names.
fn key_scancode(name: &str) -> Option<u8> {
    // Non-printing keys (case-insensitive).
    match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => return Some(0x0C),
        "space" => return Some(0x21),
        "esc" | "escape" => return Some(0x1C),
        "tab" => return Some(0x2C),
        "delete" | "backspace" | "del" => return Some(0x34),
        "caps" | "capslower" => return Some(0x3C),
        _ => {}
    }
    // Printing characters: exactly one char.
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None; // multi-char name we don't recognise
    }
    char_scancode(c)
}

/// POKEY scan code for a single printable character, Shift baked in where the
/// character lives in the shift table.
fn char_scancode(c: char) -> Option<u8> {
    // Letters carry no Shift: the OS applies SHFLOK (caps lock) to set the
    // case, so the bare key code is correct for both `a` and `A`. On a
    // freshly-booted machine caps lock is on, so letters arrive uppercase —
    // which is what Atari BASIC keywords need. (Pressing Shift with a letter
    // would *invert* the lock and yield lowercase, so we never do it here.)
    if c.is_ascii_alphabetic() {
        return letter_scancode(c.to_ascii_lowercase());
    }
    Some(match c {
        '0' => 0x32,
        '1' => 0x1F,
        '2' => 0x1E,
        '3' => 0x1A,
        '4' => 0x18,
        '5' => 0x1D,
        '6' => 0x1B,
        '7' => 0x33,
        '8' => 0x35,
        '9' => 0x30,
        // Unshifted symbols.
        ';' => 0x02,
        '+' => 0x06,
        '*' => 0x07,
        '-' => 0x0E,
        '=' => 0x0F,
        ',' => 0x20,
        ' ' => 0x21,
        '.' => 0x22,
        '/' => 0x26,
        '<' => 0x36,
        '>' => 0x37,
        // Shifted symbols (base code + Shift).
        ':' => 0x02 | SHIFT,
        '\\' => 0x06 | SHIFT,
        '^' => 0x07 | SHIFT,
        '_' => 0x0E | SHIFT,
        '|' => 0x0F | SHIFT,
        '[' => 0x20 | SHIFT,
        ']' => 0x22 | SHIFT,
        '?' => 0x26 | SHIFT,
        '$' => 0x18 | SHIFT,
        '#' => 0x1A | SHIFT,
        '&' => 0x1B | SHIFT,
        '%' => 0x1D | SHIFT,
        '"' => 0x1E | SHIFT,
        '!' => 0x1F | SHIFT,
        '(' => 0x30 | SHIFT,
        ')' => 0x32 | SHIFT,
        '\'' => 0x33 | SHIFT,
        '@' => 0x35 | SHIFT,
        '}' => 0x36 | SHIFT,
        _ => return None,
    })
}

/// Base scan code for a lowercase letter (XL OS unshifted keyboard table).
fn letter_scancode(c: char) -> Option<u8> {
    Some(match c {
        'a' => 0x3F,
        'b' => 0x15,
        'c' => 0x12,
        'd' => 0x3A,
        'e' => 0x2A,
        'f' => 0x38,
        'g' => 0x3D,
        'h' => 0x39,
        'i' => 0x0D,
        'j' => 0x01,
        'k' => 0x05,
        'l' => 0x00,
        'm' => 0x25,
        'n' => 0x23,
        'o' => 0x08,
        'p' => 0x0A,
        'q' => 0x2F,
        'r' => 0x28,
        's' => 0x3E,
        't' => 0x2D,
        'u' => 0x0B,
        'v' => 0x10,
        'w' => 0x2E,
        'x' => 0x16,
        'y' => 0x2B,
        'z' => 0x17,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{SHIFT, char_scancode, key_scancode};

    #[test]
    fn letters_use_bare_code_regardless_of_case() {
        // Case is set by the machine's caps-lock (SHFLOK), not the Shift bit,
        // so both cases of a letter share the same shift-less scan code.
        assert_eq!(char_scancode('a'), Some(0x3F));
        assert_eq!(char_scancode('A'), Some(0x3F));
        assert_eq!(char_scancode('z'), Some(0x17));
        assert_eq!(char_scancode('Z'), Some(0x17));
    }

    #[test]
    fn digits_and_unshifted_symbols() {
        assert_eq!(char_scancode('0'), Some(0x32));
        assert_eq!(char_scancode('7'), Some(0x33));
        assert_eq!(char_scancode('*'), Some(0x07));
        assert_eq!(char_scancode(' '), Some(0x21));
    }

    #[test]
    fn shifted_symbols_carry_shift() {
        assert_eq!(char_scancode('"'), Some(0x1E | SHIFT));
        assert_eq!(char_scancode('!'), Some(0x1F | SHIFT));
        assert_eq!(char_scancode('?'), Some(0x26 | SHIFT));
    }

    #[test]
    fn named_keys() {
        assert_eq!(key_scancode("Return"), Some(0x0C));
        assert_eq!(key_scancode("ENTER"), Some(0x0C));
        assert_eq!(key_scancode("space"), Some(0x21));
        assert_eq!(key_scancode("Esc"), Some(0x1C));
        assert_eq!(key_scancode("Delete"), Some(0x34));
    }

    #[test]
    fn single_char_names_round_trip() {
        assert_eq!(key_scancode("A"), Some(0x3F));
        assert_eq!(key_scancode("5"), Some(0x1D));
        assert_eq!(key_scancode("unknownlongname"), None);
        assert_eq!(key_scancode("`"), None);
    }
}
