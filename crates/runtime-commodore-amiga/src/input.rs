//! Amiga keyboard / mouse / joystick input mapping.
//!
//! Splits the host-input → machine routing out of `runtime.rs`. Three
//! input kinds land here: `Key` events route through the keyboard's
//! raw matrix code lookup; `PointerMotion` / `PointerButton` events
//! drive the mouse on `JOY0DAT`; and `Button` events drive the joystick
//! on `JOY1DAT`.
//!
//! Control-port numbering follows the documented Amiga ports, not the
//! `JOYxDAT` register index. Per *Mapping the Amiga* (Thomson &
//! Anderson, 1993, p.460): "Register JOY0DAT handles port 1 and register
//! JOY1DAT handles port 2" — the mouse lives in **port 1**, the joystick
//! in **port 2**. So a `Button` event on **port 2** drives the joystick
//! (the hardware-faithful number); **port 0** is the cross-system
//! primary-stick alias (matches the C64); and other ports — including
//! the mouse's port 1 — are dropped, since a joystick on `JOY0DAT` isn't
//! modelled. See [`joystick_machine_port`].
//!
//! Generic over `M: AmigaMachine` — the four input methods on the
//! trait (`key_event`, `move_mouse_port0`, `set_mouse_button_port0`,
//! `set_joystick_control`) are the entire surface this module needs.

use emu198x_shell::InputEvent;

use crate::variants::AmigaMachine;

/// Apply one host input event to the machine. Unrecognised event
/// kinds and unknown key names are silently dropped — the runtime
/// loop iterates the whole event queue regardless.
pub(crate) fn apply_input_event<M: AmigaMachine>(machine: &mut M, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some(code) = key_name_to_raw_code(name.as_ref()) {
                machine.key_event(code, *pressed);
            }
        }
        InputEvent::PointerMotion { device, dx, dy } if device.as_ref() == "mouse-1" => {
            machine.move_mouse_port0(*dx, *dy);
        }
        InputEvent::PointerButton {
            device,
            button,
            pressed,
        } if device.as_ref() == "mouse-1" => {
            machine.set_mouse_button_port0(button.as_ref(), *pressed);
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            if let Some(machine_port) = joystick_machine_port(*port) {
                machine.set_joystick_control(machine_port, name.as_ref(), *pressed);
            }
        }
        _ => {}
    }
}

/// Map a Seam-2 input port onto the machine's `JOY1DAT` joystick, which
/// [`AmigaMachine::set_joystick_control`] addresses as its own port 1.
///
/// The joystick lives in Amiga control **port 2** (`JOY1DAT`) per
/// *Mapping the Amiga* p.460, so input port 2 is the hardware-faithful
/// number and port 0 is the cross-system primary-stick alias. Port 1
/// (the mouse's `JOY0DAT`) and higher ports are dropped — a joystick on
/// `JOY0DAT` isn't modelled.
fn joystick_machine_port(input_port: u8) -> Option<u8> {
    match input_port {
        2 => Some(1), // control port 2 = JOY1DAT = the joystick
        0 => Some(1), // cross-system primary-stick alias → the joystick
        _ => None,    // port 1 = mouse (JOY0DAT); no joystick destination
    }
}

/// Map a host-level key name to the Amiga keyboard's raw matrix code.
/// Names are matched case-insensitively. The `raw-XX` prefix (with
/// optional `0x`) lets host code address keys the lookup table doesn't
/// expose by name.
///
/// Codes follow the Amiga Hardware Reference Manual keyboard table
/// (Appendix F): the alphanumeric block, then `Return = 0x44`,
/// `Esc = 0x45`, the function row `F1..F10 = 0x50..0x59`, and the
/// modifier keys at `0x60+`.
fn key_name_to_raw_code(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    if let Some(raw) = lower.strip_prefix("raw-") {
        return u8::from_str_radix(raw.trim_start_matches("0x"), 16).ok();
    }
    Some(match lower.as_str() {
        "1" => 0x01,
        "2" => 0x02,
        "3" => 0x03,
        "4" => 0x04,
        "5" => 0x05,
        "6" => 0x06,
        "7" => 0x07,
        "8" => 0x08,
        "9" => 0x09,
        "0" => 0x0A,
        "minus" => 0x0B,
        "equals" => 0x0C,
        "backslash" => 0x0D,
        "q" => 0x10,
        "w" => 0x11,
        "e" => 0x12,
        "r" => 0x13,
        "t" => 0x14,
        "y" => 0x15,
        "u" => 0x16,
        "i" => 0x17,
        "o" => 0x18,
        "p" => 0x19,
        "lbracket" => 0x1A,
        "rbracket" => 0x1B,
        "a" => 0x20,
        "s" => 0x21,
        "d" => 0x22,
        "f" => 0x23,
        "g" => 0x24,
        "h" => 0x25,
        "j" => 0x26,
        "k" => 0x27,
        "l" => 0x28,
        "semicolon" => 0x29,
        "apostrophe" => 0x2A,
        "z" => 0x31,
        "x" => 0x32,
        "c" => 0x33,
        "v" => 0x34,
        "b" => 0x35,
        "n" => 0x36,
        "m" => 0x37,
        "comma" => 0x38,
        "period" => 0x39,
        "slash" => 0x3A,
        "backquote" => 0x00,
        "space" => 0x40,
        "backspace" => 0x41,
        "tab" => 0x42,
        "enter" | "return" => 0x44,
        "escape" | "esc" => 0x45,
        "delete" => 0x46,
        "f1" => 0x50,
        "f2" => 0x51,
        "f3" => 0x52,
        "f4" => 0x53,
        "f5" => 0x54,
        "f6" => 0x55,
        "f7" => 0x56,
        "f8" => 0x57,
        "f9" => 0x58,
        "f10" => 0x59,
        "lshift" | "shift" => 0x60,
        "rshift" => 0x61,
        "ctrl" | "control" => 0x63,
        "lalt" => 0x64,
        "ralt" => 0x65,
        _ => return None,
    })
}

/// Map one printable ASCII character to the key chord (in press order)
/// that produces it on the Amiga's US keyboard layout. Bare keys return
/// a single name; characters needing Shift return `["lshift", key]`.
/// Returns `None` for characters with no single-chord keycap.
///
/// The names are the same ones [`key_name_to_raw_code`] understands, so
/// the typing helper can route them straight back through the keyboard.
/// US layout is the physical-keyboard default; software that loads a
/// different keymap (e.g. AMOS Pro's `SetMap gb`) remaps a few shifted
/// symbols — build such disks with `SetMap usa` to match.
pub fn keys_for_char(ch: char) -> Option<Vec<&'static str>> {
    if ch.is_ascii_uppercase() {
        return Some(vec!["lshift", LETTER_KEYS[(ch as u8 - b'A') as usize]]);
    }
    if ch.is_ascii_lowercase() {
        return Some(vec![LETTER_KEYS[(ch as u8 - b'a') as usize]]);
    }
    Some(match ch {
        ' ' => vec!["space"],
        '\n' | '\r' => vec!["return"],
        '\t' => vec!["tab"],
        '1' => vec!["1"],
        '2' => vec!["2"],
        '3' => vec!["3"],
        '4' => vec!["4"],
        '5' => vec!["5"],
        '6' => vec!["6"],
        '7' => vec!["7"],
        '8' => vec!["8"],
        '9' => vec!["9"],
        '0' => vec!["0"],
        '!' => vec!["lshift", "1"],
        '@' => vec!["lshift", "2"],
        '#' => vec!["lshift", "3"],
        '$' => vec!["lshift", "4"],
        '%' => vec!["lshift", "5"],
        '^' => vec!["lshift", "6"],
        '&' => vec!["lshift", "7"],
        '*' => vec!["lshift", "8"],
        '(' => vec!["lshift", "9"],
        ')' => vec!["lshift", "0"],
        '-' => vec!["minus"],
        '_' => vec!["lshift", "minus"],
        '=' => vec!["equals"],
        '+' => vec!["lshift", "equals"],
        '[' => vec!["lbracket"],
        '{' => vec!["lshift", "lbracket"],
        ']' => vec!["rbracket"],
        '}' => vec!["lshift", "rbracket"],
        '\\' => vec!["backslash"],
        '|' => vec!["lshift", "backslash"],
        ';' => vec!["semicolon"],
        ':' => vec!["lshift", "semicolon"],
        '\'' => vec!["apostrophe"],
        '"' => vec!["lshift", "apostrophe"],
        ',' => vec!["comma"],
        '<' => vec!["lshift", "comma"],
        '.' => vec!["period"],
        '>' => vec!["lshift", "period"],
        '/' => vec!["slash"],
        '?' => vec!["lshift", "slash"],
        '`' => vec!["backquote"],
        '~' => vec!["lshift", "backquote"],
        _ => return None,
    })
}

/// Lowercase letter-key names indexed by `letter - 'a'`.
const LETTER_KEYS: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];

#[cfg(test)]
mod tests {
    use super::{joystick_machine_port, key_name_to_raw_code, keys_for_char};

    /// Port 2 is the documented joystick port (JOY1DAT) per *Mapping the
    /// Amiga*; port 0 is the cross-system primary-stick alias. Both reach
    /// the machine's joystick, which is addressed as its own port 1.
    #[test]
    fn joystick_ports_follow_documented_amiga_numbering() {
        assert_eq!(joystick_machine_port(2), Some(1)); // control port 2 = JOY1DAT
        assert_eq!(joystick_machine_port(0), Some(1)); // primary-stick alias
    }

    /// Port 1 is the mouse's JOY0DAT, not a joystick destination, and
    /// higher ports aren't modelled — both drop silently.
    #[test]
    fn non_joystick_ports_are_dropped() {
        assert_eq!(joystick_machine_port(1), None); // mouse port (JOY0DAT)
        assert_eq!(joystick_machine_port(3), None);
        assert_eq!(joystick_machine_port(255), None);
    }

    /// Spec invariant: named keys map to stable HRM raw matrix codes.
    /// One assert per region catches a regression where someone widens
    /// an arm and silently shifts a key.
    #[test]
    fn key_name_lookup_covers_documented_keys() {
        assert_eq!(key_name_to_raw_code("1"), Some(0x01));
        assert_eq!(key_name_to_raw_code("0"), Some(0x0A));
        assert_eq!(key_name_to_raw_code("q"), Some(0x10));
        assert_eq!(key_name_to_raw_code("a"), Some(0x20));
        assert_eq!(key_name_to_raw_code("z"), Some(0x31));
        assert_eq!(key_name_to_raw_code("space"), Some(0x40));
        assert_eq!(key_name_to_raw_code("backspace"), Some(0x41));
        assert_eq!(key_name_to_raw_code("tab"), Some(0x42));
        // Return is 0x44 and Escape is 0x45 (HRM Appendix F). A prior
        // table had Return on 0x45 — which is actually Escape, and put
        // AMOS Pro into Direct mode instead of inserting a newline.
        assert_eq!(key_name_to_raw_code("enter"), Some(0x44));
        assert_eq!(key_name_to_raw_code("return"), Some(0x44));
        assert_eq!(key_name_to_raw_code("escape"), Some(0x45));
        assert_eq!(key_name_to_raw_code("esc"), Some(0x45));
        // Function row and modifiers.
        assert_eq!(key_name_to_raw_code("f1"), Some(0x50));
        assert_eq!(key_name_to_raw_code("f10"), Some(0x59));
        assert_eq!(key_name_to_raw_code("lshift"), Some(0x60));
        // Symbol keys used by typed source.
        assert_eq!(key_name_to_raw_code("semicolon"), Some(0x29));
        assert_eq!(key_name_to_raw_code("apostrophe"), Some(0x2A));
        // Case-insensitive lookup is part of the contract.
        assert_eq!(key_name_to_raw_code("Space"), Some(0x40));
        assert_eq!(key_name_to_raw_code("ENTER"), Some(0x44));
        // raw-XX prefix accepts host-supplied raw codes the lookup
        // table doesn't otherwise expose by name.
        assert_eq!(key_name_to_raw_code("raw-50"), Some(0x50));
        assert_eq!(key_name_to_raw_code("raw-0x5F"), Some(0x5F));
        assert_eq!(key_name_to_raw_code("unknown"), None);
        assert_eq!(key_name_to_raw_code(""), None);
    }

    /// Lowercase is a bare keycap; uppercase and shifted symbols return
    /// a Shift chord; the colon and double-quote that AMOS source leans
    /// on resolve to real chords.
    #[test]
    fn keys_for_char_maps_letters_and_shifted_symbols() {
        assert_eq!(keys_for_char('a'), Some(vec!["a"]));
        assert_eq!(keys_for_char('A'), Some(vec!["lshift", "a"]));
        assert_eq!(keys_for_char('6'), Some(vec!["6"]));
        assert_eq!(keys_for_char(' '), Some(vec!["space"]));
        assert_eq!(keys_for_char('\n'), Some(vec!["return"]));
        assert_eq!(keys_for_char(':'), Some(vec!["lshift", "semicolon"]));
        assert_eq!(keys_for_char('"'), Some(vec!["lshift", "apostrophe"]));
        assert_eq!(keys_for_char('('), Some(vec!["lshift", "9"]));
        // Every chord resolves to a real raw code.
        for ch in "Print \"Hello\" : Cls 6".chars() {
            if let Some(keys) = keys_for_char(ch) {
                for k in keys {
                    assert!(key_name_to_raw_code(k).is_some(), "no raw code for {k}");
                }
            }
        }
        assert_eq!(keys_for_char('£'), None);
    }
}
