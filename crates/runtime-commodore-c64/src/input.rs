//! C64 keyboard / joystick input mapping.
//!
//! Splits the keyboard-matrix lookup table out of `runtime.rs` so the
//! 70+ key entries don't dominate the file. The matrix is the
//! standard PAL breadbin layout (HRM Appendix C); shifted symbols
//! land on the right keycap on a UK/US keyboard.

use emu198x_shell::InputEvent;
use machine_commodore_c64::C64;

/// Apply one host input event to the machine: keys land in the
/// keyboard matrix, joystick buttons land on the named control of
/// the named port. Other event kinds (mouse motion, etc.) are
/// ignored — the C64 has no mouse input surface in this runtime.
pub(crate) fn apply_input_event(machine: &mut C64, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((row, col)) = c64_key_position(name.as_ref()) {
                machine.keyboard_mut().set_key(row, col, *pressed);
            }
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            let _ = machine.set_joystick_control(*port, name.as_ref(), *pressed);
        }
        _ => {}
    }
}

/// Look up `(row, col)` in the C64 keyboard matrix for a host-level
/// key name. Returns `None` for keys that don't have a C64 keycap;
/// the caller silently drops those events.
fn c64_key_position(name: &str) -> Option<(u8, u8)> {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "DELETE" | "DEL" | "BACKSPACE" => Some((0, 0)),
        "RETURN" | "ENTER" => Some((0, 1)),
        "RIGHT" | "CRSRRIGHT" => Some((0, 2)),
        "F7" => Some((0, 3)),
        "F1" => Some((0, 4)),
        "F3" => Some((0, 5)),
        "F5" => Some((0, 6)),
        "DOWN" | "CRSRDOWN" => Some((0, 7)),
        "3" => Some((1, 0)),
        "W" => Some((1, 1)),
        "A" => Some((1, 2)),
        "4" => Some((1, 3)),
        "Z" => Some((1, 4)),
        "S" => Some((1, 5)),
        "E" => Some((1, 6)),
        "LSHIFT" => Some((1, 7)),
        "5" => Some((2, 0)),
        "R" => Some((2, 1)),
        "D" => Some((2, 2)),
        "6" => Some((2, 3)),
        "C" => Some((2, 4)),
        "F" => Some((2, 5)),
        "T" => Some((2, 6)),
        "X" => Some((2, 7)),
        "7" => Some((3, 0)),
        "Y" => Some((3, 1)),
        "G" => Some((3, 2)),
        "8" => Some((3, 3)),
        "B" => Some((3, 4)),
        "H" => Some((3, 5)),
        "U" => Some((3, 6)),
        "V" => Some((3, 7)),
        "9" => Some((4, 0)),
        "I" => Some((4, 1)),
        "J" => Some((4, 2)),
        "0" => Some((4, 3)),
        "M" => Some((4, 4)),
        "K" => Some((4, 5)),
        "O" => Some((4, 6)),
        "N" => Some((4, 7)),
        "PLUS" => Some((5, 0)),
        "P" => Some((5, 1)),
        "L" => Some((5, 2)),
        "MINUS" => Some((5, 3)),
        "." | "PERIOD" => Some((5, 4)),
        ":" | "COLON" => Some((5, 5)),
        "@" | "AT" => Some((5, 6)),
        "," | "COMMA" => Some((5, 7)),
        "POUND" | "STERLING" => Some((6, 0)),
        "ASTERISK" | "STAR" => Some((6, 1)),
        "SEMICOLON" => Some((6, 2)),
        "HOME" => Some((6, 3)),
        "RSHIFT" => Some((6, 4)),
        "=" | "EQUALS" | "EQUAL" => Some((6, 5)),
        "UP" | "CRSRUP" => Some((6, 6)),
        "/" | "SLASH" => Some((6, 7)),
        "1" => Some((7, 0)),
        "LEFTARROW" => Some((7, 1)),
        "CTRL" | "CONTROL" => Some((7, 2)),
        "2" => Some((7, 3)),
        "SPACE" => Some((7, 4)),
        "COMMODORE" | "CBM" => Some((7, 5)),
        "Q" => Some((7, 6)),
        "RUNSTOP" | "RUN/STOP" => Some((7, 7)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::c64_key_position;

    /// Spec invariant: every key the native shell sends has a matrix
    /// position. Catches regressions where a rename or re-cased lookup
    /// silently stops mapping a host key. The C64 has no dedicated
    /// LEFT key — host LEFT is handled at the shell layer as
    /// RSHIFT+RIGHT — so it's deliberately absent from this list.
    #[test]
    fn input_mapping_covers_native_shell_keys() {
        for key in [
            "RETURN", "BACKSPACE", "SPACE", "LSHIFT", "RSHIFT", "CTRL", "RUNSTOP", "F1", "F3",
            "F5", "F7", "UP", "DOWN", "RIGHT", "A", "Z", "0", "9", ":", "@", ",", ".",
        ] {
            assert!(
                c64_key_position(key).is_some(),
                "native shell key {key:?} should map"
            );
        }
        // Case-insensitive lookups are part of the contract — the
        // native shell sends lowercase keycodes for character keys
        // and reserves uppercase for special names.
        assert_eq!(c64_key_position("delete"), Some((0, 0)));
        assert_eq!(c64_key_position("right"), Some((0, 2)));
        assert_eq!(c64_key_position("f1"), Some((0, 4)));
        assert_eq!(c64_key_position("f7"), Some((0, 3)));
        assert_eq!(c64_key_position("plus"), Some((5, 0)));
        assert_eq!(c64_key_position("home"), Some((6, 3)));
        assert_eq!(c64_key_position("equals"), Some((6, 5)));
        assert_eq!(c64_key_position("up"), Some((6, 6)));
        assert_eq!(c64_key_position("commodore"), Some((7, 5)));
        assert_eq!(c64_key_position("runstop"), Some((7, 7)));
        assert_eq!(c64_key_position("LEFT"), None);
        assert_eq!(c64_key_position("UNKNOWN"), None);
    }
}
