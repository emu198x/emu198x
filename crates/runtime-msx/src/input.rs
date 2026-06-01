//! MSX1 keyboard input mapping.
//!
//! MSX has an 11×8 PPI-driven keyboard matrix. Host `Key` events
//! arrive with a host-level key name (`"return"`, `"space"`, `"a"`,
//! ...); this module maps them to the (row, bit) matrix coordinate
//! and drives `Msx::press_key` / `Msx::release_key`.
//!
//! Standard MSX layout (rows 0-7 — rows 8-10 are joystick / function
//! keys which we hook later):
//!
//! ```text
//! Row 0: 0 1 2 3 4 5 6 7
//! Row 1: 8 9 - = \ [ ] ;
//! Row 2: ' ` , . / DEAD a b
//! Row 3: c d e f g h i j
//! Row 4: k l m n o p q r
//! Row 5: s t u v w x y z
//! Row 6: SHIFT CTRL GRAPH CAPS CODE F1 F2 F3
//! Row 7: F4 F5 ESC TAB STOP BS SELECT ENTER
//! Row 8: SPACE HOME INS DEL LEFT UP DOWN RIGHT
//! ```
//!
//! Names are lower-cased before lookup. `Button` events are not yet
//! mapped (MSX joystick support is a follow-up).

use emu198x_shell::InputEvent;
use machine_msx::Msx;

/// Apply one host input event to the MSX keyboard matrix.
pub(crate) fn apply_input_event(machine: &mut Msx, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event {
        if let Some((row, bit)) = key_to_matrix(name.as_ref()) {
            if *pressed {
                machine.press_key(row, bit);
            } else {
                machine.release_key(row, bit);
            }
        }
    }
}

/// Map a host-level key name to an MSX keyboard matrix (row, bit).
#[must_use]
pub(crate) fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
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
        // Row 1: 8 9 - = \ [ ] ;
        "8" => (1, 0),
        "9" => (1, 1),
        "-" | "minus" => (1, 2),
        "=" | "equals" => (1, 3),
        "\\" | "backslash" => (1, 4),
        "[" | "leftbracket" => (1, 5),
        "]" | "rightbracket" => (1, 6),
        ";" | "semicolon" => (1, 7),
        // Row 2: ' ` , . / dead a b
        "'" | "apostrophe" | "quote" => (2, 0),
        "`" | "backtick" | "grave" => (2, 1),
        "," | "comma" => (2, 2),
        "." | "period" => (2, 3),
        "/" | "slash" => (2, 4),
        "a" => (2, 6),
        "b" => (2, 7),
        // Row 3
        "c" => (3, 0),
        "d" => (3, 1),
        "e" => (3, 2),
        "f" => (3, 3),
        "g" => (3, 4),
        "h" => (3, 5),
        "i" => (3, 6),
        "j" => (3, 7),
        // Row 4
        "k" => (4, 0),
        "l" => (4, 1),
        "m" => (4, 2),
        "n" => (4, 3),
        "o" => (4, 4),
        "p" => (4, 5),
        "q" => (4, 6),
        "r" => (4, 7),
        // Row 5
        "s" => (5, 0),
        "t" => (5, 1),
        "u" => (5, 2),
        "v" => (5, 3),
        "w" => (5, 4),
        "x" => (5, 5),
        "y" => (5, 6),
        "z" => (5, 7),
        // Row 6: shift / ctrl / graph / caps / code / F1-F3
        "shift" | "lshift" | "rshift" => (6, 0),
        "ctrl" | "control" => (6, 1),
        "graph" => (6, 2),
        "caps" | "capslock" => (6, 3),
        "code" => (6, 4),
        "f1" => (6, 5),
        "f2" => (6, 6),
        "f3" => (6, 7),
        // Row 7
        "f4" => (7, 0),
        "f5" => (7, 1),
        "esc" | "escape" => (7, 2),
        "tab" => (7, 3),
        "stop" => (7, 4),
        "bs" | "backspace" => (7, 5),
        "select" => (7, 6),
        "enter" | "return" => (7, 7),
        // Row 8: space + arrows
        " " | "space" => (8, 0),
        "home" => (8, 1),
        "ins" | "insert" => (8, 2),
        "del" | "delete" => (8, 3),
        "left" | "arrowleft" => (8, 4),
        "up" | "arrowup" => (8, 5),
        "down" | "arrowdown" => (8, 6),
        "right" | "arrowright" => (8, 7),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_bios() -> Vec<u8> {
        let mut rom = vec![0u8; 32768];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    fn make_msx() -> Msx {
        Msx::new(trap_bios(), machine_msx::MsxRegion::Ntsc)
    }

    fn key(name: &str, pressed: bool) -> InputEvent {
        InputEvent::Key {
            name: std::borrow::Cow::Owned(name.to_string()),
            pressed,
        }
    }

    #[test]
    fn key_a_lands_on_row_2_bit_6() {
        assert_eq!(key_to_matrix("a"), Some((2, 6)));
    }

    #[test]
    fn return_lands_on_row_7_bit_7() {
        assert_eq!(key_to_matrix("return"), Some((7, 7)));
        assert_eq!(key_to_matrix("enter"), Some((7, 7)));
    }

    #[test]
    fn space_lands_on_row_8_bit_0() {
        assert_eq!(key_to_matrix("space"), Some((8, 0)));
        assert_eq!(key_to_matrix(" "), Some((8, 0)));
    }

    #[test]
    fn unknown_key_returns_none() {
        assert_eq!(key_to_matrix("frobozz"), None);
    }

    #[test]
    fn key_press_event_pulls_matrix_cell_low() {
        let mut msx = make_msx();
        apply_input_event(&mut msx, &key("a", true));
        // Row 2 bit 6 should be clear (active-low).
        assert_eq!(msx.keyboard_mut()[2] & (1 << 6), 0);
    }

    #[test]
    fn key_release_event_sets_matrix_cell_high() {
        let mut msx = make_msx();
        apply_input_event(&mut msx, &key("a", true));
        apply_input_event(&mut msx, &key("a", false));
        assert_eq!(msx.keyboard_mut()[2] & (1 << 6), 1 << 6);
    }
}
