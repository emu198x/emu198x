//! Amstrad CPC keyboard and joystick input mapping.
//!
//! [`machine_amstrad_cpc::key_for_char`] already covers everything with a
//! printable legend. What it cannot cover is the keys that produce no
//! character — cursors, `ESC`, `COPY`, the function keypad — and the joystick,
//! which on a CPC is not a separate device but row 9 of the same matrix.
//! [`NAMED_KEYS`] names those cells; everything else falls through to the
//! machine's own character map, so there is one table per fact.
//!
//! Matrix positions are Caprice32's `InputMapper::cpc_kbd` table
//! (`emulators/amstrad-cpc/caprice32/src/keyboard.cpp`), whose scancodes encode
//! the cell as `0xRB` — row in the high nibble, bit in the low; Shift (`0x25`)
//! and Control (`0x27`) are hard-coded in its `applyKeypress`. The printable
//! keys there agree with the machine crate's own table, which came from MAME's
//! `cpc464` `kbrow.N` ports; two independent implementations reaching the same
//! matrix is the check worth having.

use emu198x_shell::{InputEvent, KeyTiming, KeyboardTarget, STANDARD_KEY_TIMING};
use machine_amstrad_cpc::{AmstradCpc, key_for_char};

use crate::runtime::AmstradCpcRuntime;

/// Keys with no printable legend, as `(name, row, bit)`.
///
/// Aliases sit next to their canonical name so a host toolkit's spelling
/// (`arrowup`, `cursorup`) reaches the same cell.
const NAMED_KEYS: &[(&str, usize, u8)] = &[
    // Rows 0-1: cursors and the function keypad.
    ("up", 0, 0),
    ("cursorup", 0, 0),
    ("arrowup", 0, 0),
    ("right", 0, 1),
    ("cursorright", 0, 1),
    ("arrowright", 0, 1),
    ("down", 0, 2),
    ("cursordown", 0, 2),
    ("arrowdown", 0, 2),
    ("f9", 0, 3),
    ("f6", 0, 4),
    ("f3", 0, 5),
    // The keypad's own ENTER, distinct from RETURN on the main block.
    ("enter", 0, 6),
    ("fperiod", 0, 7),
    ("left", 1, 0),
    ("cursorleft", 1, 0),
    ("arrowleft", 1, 0),
    ("copy", 1, 1),
    ("f7", 1, 2),
    ("f8", 1, 3),
    ("f5", 1, 4),
    ("f1", 1, 5),
    ("f2", 1, 6),
    ("f0", 1, 7),
    // Row 2: editing keys and the modifiers.
    ("clr", 2, 0),
    ("clear", 2, 0),
    ("return", 2, 2),
    ("f4", 2, 4),
    ("shift", 2, 5),
    ("lshift", 2, 5),
    ("rshift", 2, 5),
    ("control", 2, 7),
    ("ctrl", 2, 7),
    ("lctrl", 2, 7),
    ("rctrl", 2, 7),
    // The two keys in the alphanumeric block with no printable legend.
    ("escape", 8, 2),
    ("esc", 8, 2),
    ("tab", 8, 4),
    ("capslock", 8, 6),
    ("caps", 8, 6),
    ("space", 5, 7),
    // Row 9: joystick 0 and DEL share a row, which is why a CPC game can read
    // the stick without scanning the keyboard.
    ("joy1up", 9, 0),
    ("joyup", 9, 0),
    ("joy1down", 9, 1),
    ("joydown", 9, 1),
    ("joy1left", 9, 2),
    ("joyleft", 9, 2),
    ("joy1right", 9, 3),
    ("joyright", 9, 3),
    ("joy1fire1", 9, 4),
    ("joyfire1", 9, 4),
    ("fire", 9, 4),
    ("joy1fire2", 9, 5),
    ("joyfire2", 9, 5),
    ("delete", 9, 7),
    ("del", 9, 7),
    ("backspace", 9, 7),
];

/// The name `press_key` uses for Shift.
const SHIFT_NAME: &str = "shift";

/// The matrix cell a key name selects, as `(row, bit)`.
///
/// A single printable character falls through to the machine's own character
/// map, so `"a"` and `"["` work here without being repeated — but only where
/// the legend is unshifted. `"!"` resolves to nothing, because pressing row 8
/// bit 0 alone types a `1`; [`keys_for_char`](CpcKeyboard::keys_for_char)
/// turns it into a `shift` + `1` chord instead.
#[must_use]
pub fn key_for_name(name: &str) -> Option<(usize, u8)> {
    let lower = name.to_ascii_lowercase();
    if let Some(&(_, row, bit)) = NAMED_KEYS.iter().find(|&&(n, _, _)| n == lower) {
        return Some((row, bit));
    }
    let (row, bit, shift) = key_for_char(sole_char(name)?)?;
    (!shift).then_some((row, bit))
}

/// The name for a cell, preferring a word over a bare character.
///
/// `(5, 7)` is `"space"` rather than `" "`, and `(2, 2)` is `"return"` rather
/// than `"\r"` — both are the same key, but only one reads as a key name.
fn name_for_cell(row: usize, bit: u8) -> Option<String> {
    if let Some(&(name, _, _)) = NAMED_KEYS.iter().find(|&&(_, r, b)| (r, b) == (row, bit)) {
        return Some(name.to_owned());
    }
    // Otherwise the unshifted printable legend on that key.
    (' '..='~')
        .find(|&c| key_for_char(c) == Some((row, bit, false)))
        .map(String::from)
}

pub(crate) fn apply_input_event(machine: &mut AmstradCpc, event: &InputEvent) {
    let InputEvent::Key { name, pressed } = event else {
        return;
    };
    // A shifted legend has to go through `press_char`, which presses Shift
    // too; anything else is a plain matrix cell.
    let name = name.as_ref();
    if let Some(c) = single_shifted_char(name) {
        if *pressed {
            machine.press_char(c);
        } else {
            machine.release_char(c);
        }
    } else if let Some((row, bit)) = key_for_name(name) {
        if *pressed {
            machine.press_key(row, bit);
        } else {
            machine.release_key(row, bit);
        }
    }
}

/// The single character a name consists of, or `None` if it is a word.
fn sole_char(name: &str) -> Option<char> {
    let mut chars = name.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

/// A one-character name whose legend needs Shift, e.g. `"!"`.
fn single_shifted_char(name: &str) -> Option<char> {
    let c = sole_char(name)?;
    let (_, _, shift) = key_for_char(c)?;
    shift.then_some(c)
}

/// The CPC's keyboard, for the shared `press_key` / `type_string` tools.
///
/// Hand-written rather than [`emu198x_shell::STANDARD_KEYBOARD`] because the
/// CPC reaches `! " # $ % & ' ( ) * + < > = ? _` through Shift. The standard
/// keyboard passes a symbol through as its own one-character name and lets the
/// input layer drop what it does not recognise, which is how the C64 typed one
/// string and ran another (#916, #919). Here every character the machine can
/// produce resolves to a chord, and every one it cannot returns `None` so
/// `type_string` refuses the step outright.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpcKeyboard;

impl KeyboardTarget for CpcKeyboard {
    fn key_name_is_valid(&self, name: &str) -> bool {
        key_for_name(name).is_some() || single_shifted_char(name).is_some()
    }

    fn key_names_hint(&self) -> &'static str {
        "letters and digits, the punctuation on a CPC464 keyboard, \
         space, return, enter, escape, tab, capslock, shift, control, clr, del, copy, \
         up/down/left/right, f0-f9, fperiod, and joyup/joydown/joyleft/joyright/fire"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        let (row, bit, shift) = key_for_char(ch)?;
        let base = name_for_cell(row, bit)?;
        Some(if shift {
            vec![SHIFT_NAME.to_owned(), base]
        } else {
            vec![base]
        })
    }

    fn key_timing(&self) -> KeyTiming {
        // The CPC firmware scans the keyboard once per frame from its 300 Hz
        // ticker interrupt, which the conservative shared default already
        // accommodates. Re-tune against a real boot before changing.
        STANDARD_KEY_TIMING
    }
}

/// The shared instance; the keyboard carries no state.
pub(crate) static CPC_KEYBOARD: CpcKeyboard = CpcKeyboard;

impl AmstradCpcRuntime {
    /// The CPC's keyboard description, or `None` before firmware is loaded.
    pub(crate) fn cpc_keyboard(&self) -> Option<&'static dyn KeyboardTarget> {
        self.machine()
            .is_some()
            .then_some(&CPC_KEYBOARD as &dyn KeyboardTarget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_land_on_their_caprice32_cells() {
        // Scancodes from `cpc_kbd`, read as `0xRB`.
        for (name, scancode) in [
            ("up", 0x00_u8),
            ("right", 0x01),
            ("down", 0x02),
            ("f9", 0x03),
            ("f6", 0x04),
            ("f3", 0x05),
            ("enter", 0x06),
            ("fperiod", 0x07),
            ("left", 0x10),
            ("copy", 0x11),
            ("f7", 0x12),
            ("f8", 0x13),
            ("f5", 0x14),
            ("f1", 0x15),
            ("f2", 0x16),
            ("f0", 0x17),
            ("clr", 0x20),
            ("return", 0x22),
            ("f4", 0x24),
            ("shift", 0x25),
            ("control", 0x27),
            ("escape", 0x82),
            ("tab", 0x84),
            ("capslock", 0x86),
            ("space", 0x57),
            ("del", 0x97),
            ("joyup", 0x90),
            ("joydown", 0x91),
            ("joyleft", 0x92),
            ("joyright", 0x93),
            ("fire", 0x94),
            ("joyfire2", 0x95),
        ] {
            let expected = (usize::from(scancode >> 4), scancode & 0x0F);
            assert_eq!(key_for_name(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn names_are_case_insensitive() {
        assert_eq!(key_for_name("Escape"), key_for_name("escape"));
        assert_eq!(key_for_name("F1"), key_for_name("f1"));
    }

    #[test]
    fn a_printable_name_falls_through_to_the_machines_own_map() {
        // Not in `NAMED_KEYS`, so this only passes by delegating.
        assert_eq!(key_for_name("a"), Some((8, 5)));
        assert_eq!(key_for_name("0"), Some((4, 0)));
        assert_eq!(key_for_name("["), Some((2, 1)));
    }

    #[test]
    fn a_shifted_legend_is_not_a_bare_cell() {
        // `!` is Shift+1. Returning row 8 bit 0 would type a `1`.
        assert_eq!(key_for_name("!"), None);
        assert_eq!(single_shifted_char("!"), Some('!'));
        assert_eq!(single_shifted_char("a"), None);
    }

    #[test]
    fn an_unknown_name_is_ignored_rather_than_guessed() {
        assert_eq!(key_for_name("meta"), None);
        assert_eq!(key_for_name("f11"), None);
        assert_eq!(key_for_name(""), None);
    }

    #[test]
    fn every_printable_ascii_the_cpc_can_type_resolves_to_a_chord() {
        // The C64's `type_string` dropped characters it could not map and
        // typed the rest (#916). Here the whole printable set is accounted
        // for: each character either yields a chord this machine can press, or
        // is named below as one the CPC464 keyboard genuinely lacks. Only the
        // tilde qualifies.
        const UNREACHABLE: &str = "~";
        let kb = CpcKeyboard;
        for ch in ' '..='~' {
            let chord = kb.keys_for_char(ch);
            if UNREACHABLE.contains(ch) {
                assert_eq!(chord, None, "{ch:?} was expected to be unreachable");
                continue;
            }
            let chord = chord.unwrap_or_else(|| panic!("{ch:?} has no chord"));
            assert!(!chord.is_empty(), "{ch:?} produced an empty chord");
            // Every name in the chord must be one `apply_input_event` can act
            // on, or the chord is a promise the input layer will not keep.
            for name in &chord {
                assert!(
                    key_for_name(name).is_some(),
                    "{ch:?} chord names {name:?}, which no cell matches"
                );
            }
        }
    }

    #[test]
    fn a_shifted_character_types_shift_plus_its_base_key() {
        let kb = CpcKeyboard;
        assert_eq!(
            kb.keys_for_char('!'),
            Some(vec!["shift".into(), "1".into()])
        );
        assert_eq!(
            kb.keys_for_char('"'),
            Some(vec!["shift".into(), "2".into()])
        );
        // An upper-case letter is the same key with Shift.
        assert_eq!(
            kb.keys_for_char('A'),
            Some(vec!["shift".into(), "a".into()])
        );
        assert_eq!(kb.keys_for_char('a'), Some(vec!["a".into()]));
    }

    #[test]
    fn word_names_beat_bare_characters_for_the_same_cell() {
        let kb = CpcKeyboard;
        assert_eq!(kb.keys_for_char(' '), Some(vec!["space".into()]));
        assert_eq!(kb.keys_for_char('\r'), Some(vec!["return".into()]));
        assert_eq!(kb.keys_for_char('\n'), Some(vec!["return".into()]));
    }

    #[test]
    fn the_keyboard_validates_the_names_it_advertises() {
        let kb = CpcKeyboard;
        for (name, _, _) in NAMED_KEYS {
            assert!(kb.key_name_is_valid(name), "{name}");
        }
        assert!(kb.key_name_is_valid("!"), "a shifted legend is pressable");
        assert!(!kb.key_name_is_valid("meta"));
    }
}
