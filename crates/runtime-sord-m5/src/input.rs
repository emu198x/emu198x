//! Sord M5 keyboard input mapping.
//!
//! The M5 has a 7-row × 8-bit keyboard (Y0-Y6) read directly at I/O ports
//! `$30`-`$36`, active-high. Host-level key names map to (row, bit) via this
//! table, sourced from MAME `sord/m5.cpp` (`PORT_START("Y0")`..`"Y6"`).
//!
//! Joystick input arrives as [`InputEvent::Button`] on a numbered port. The
//! M5 reads both control ports' directions at `$37` (active high, no separate
//! fire line — action buttons are on the keyboard). One [`ControllerCache`]
//! mirror per port re-applies the whole state via [`SordM5::set_joystick`].

use emu198x_shell::InputEvent;
use machine_sord_m5::SordM5;

/// Host-side mirror of one M5 control port's four directions, re-applied via
/// [`SordM5::set_joystick`] on every event.
#[derive(Clone, Copy, Debug, Default)]
struct JoystickCache {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

impl JoystickCache {
    /// Record a direction by name. Returns `true` when it mapped (the M5 has
    /// no joystick fire line, so fire / button names are ignored here).
    fn set_control(&mut self, name: &str, pressed: bool) -> bool {
        match name {
            "up" | "arrowup" => self.up = pressed,
            "down" | "arrowdown" => self.down = pressed,
            "left" | "arrowleft" => self.left = pressed,
            "right" | "arrowright" => self.right = pressed,
            _ => return false,
        }
        true
    }
}

/// Host-side mirror of both M5 control ports (1 and 2), indexed `port - 1`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    ports: [JoystickCache; 2],
}

impl ControllerCache {
    /// Apply a `Button` event for `port` (1 or 2): record the direction and
    /// push the whole port state to the machine. Out-of-range ports clamp to
    /// the valid pair, matching [`SordM5::set_joystick`].
    fn apply_button(&mut self, machine: &mut SordM5, port: u8, name: &str, pressed: bool) {
        let port = port.clamp(1, 2);
        let cache = &mut self.ports[usize::from(port - 1)];
        if cache.set_control(name, pressed) {
            machine.set_joystick(port, cache.up, cache.down, cache.left, cache.right);
        }
    }
}

/// Apply one host input event. `Key` events drive the keyboard matrix;
/// `Button` events drive the joystick directions on their numbered port.
pub(crate) fn apply_input_event(
    machine: &mut SordM5,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((row, bit)) = key_to_matrix(name.as_ref()) {
                if *pressed {
                    machine.press_key(row, bit);
                } else {
                    machine.release_key(row, bit);
                }
            }
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            cache.apply_button(machine, *port, &name.to_ascii_lowercase(), *pressed);
        }
        _ => {}
    }
}

/// Maps a host key name to its `(row, bit)` cell in the M5 matrix, where `bit`
/// is the bit index (`0`-`7`) within row `Y{row}`. Layout per MAME
/// `sord/m5.cpp`. The M5 keyboard has no cursor keys — direction is the
/// joystick — so arrow names deliberately do not appear here.
#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Y0: modifiers, space, enter.
        "ctrl" | "control" | "lcontrol" => (0, 0),
        "func" | "function" | "tab" => (0, 1),
        "shift" | "lshift" => (0, 2),
        "rshift" => (0, 3),
        "space" | " " => (0, 6),
        "enter" | "return" => (0, 7),
        // Y1: digits 1-8.
        "1" => (1, 0),
        "2" => (1, 1),
        "3" => (1, 2),
        "4" => (1, 3),
        "5" => (1, 4),
        "6" => (1, 5),
        "7" => (1, 6),
        "8" => (1, 7),
        // Y2: Q W E R T Y U I.
        "q" => (2, 0),
        "w" => (2, 1),
        "e" => (2, 2),
        "r" => (2, 3),
        "t" => (2, 4),
        "y" => (2, 5),
        "u" => (2, 6),
        "i" => (2, 7),
        // Y3: A S D F G H J K.
        "a" => (3, 0),
        "s" => (3, 1),
        "d" => (3, 2),
        "f" => (3, 3),
        "g" => (3, 4),
        "h" => (3, 5),
        "j" => (3, 6),
        "k" => (3, 7),
        // Y4: Z X C V B N M ,.
        "z" => (4, 0),
        "x" => (4, 1),
        "c" => (4, 2),
        "v" => (4, 3),
        "b" => (4, 4),
        "n" => (4, 5),
        "m" => (4, 6),
        "," | "comma" => (4, 7),
        // Y5: 9 0 - ^ . / _ \\ — a JIS layout, so `^` and `\\` are their
        // own keycaps rather than shifted legends of `-` and `/`.
        "9" => (5, 0),
        "0" => (5, 1),
        "-" | "minus" => (5, 2),
        "^" | "caret" => (5, 3),
        "." | "period" | "stop" => (5, 4),
        "/" | "slash" => (5, 5),
        "_" | "underscore" | "triangle" => (5, 6),
        "\\" | "backslash" | "yen" => (5, 7),
        // Y6: O P [ ] L : ' \.
        "o" => (6, 0),
        "p" => (6, 1),
        "@" | "at" => (6, 2),
        "[" | "leftbracket" | "openbrace" => (6, 3),
        "l" => (6, 4),
        ";" | "semicolon" => (6, 5),
        ":" | "colon" => (6, 6),
        "]" | "rightbracket" | "closebrace" => (6, 7),
        _ => return None,
    })
}

/// Whether this machine's input layer can deliver `name`.
///
/// This is the same lookup [`apply_input_event`] performs before injecting a
/// keystroke, exposed so the shared keyboard can refuse a character the
/// machine cannot type instead of counting one it silently dropped (#1196).
pub(crate) fn knows_key_name(name: &str) -> bool {
    key_to_matrix(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use machine_sord_m5::M5Region;
    use std::borrow::Cow;

    fn make_m5() -> SordM5 {
        let mut rom = vec![0u8; 0x2000];
        rom[0x0008] = 0x18; // JR -2 trap
        rom[0x0009] = 0xFE;
        SordM5::new(rom, Vec::new(), M5Region::Ntsc)
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    #[test]
    fn button_events_drive_both_joystick_ports_active_high() {
        let mut m = make_m5();
        let mut cache = ControllerCache::default();

        // Player 1 up → bit 1; player 2 left → bit 6 (active high).
        apply_input_event(&mut m, &mut cache, &button(1, "up", true));
        apply_input_event(&mut m, &mut cache, &button(2, "left", true));
        let v = m.joystick_byte();
        assert_eq!(v & 0x02, 0x02, "P1 up → bit 1 high");
        assert_eq!(v & 0x40, 0x40, "P2 left → bit 6 high");
        assert_eq!(v & 0x01, 0, "P1 right idle low");

        // Releasing P1 up clears its bit; P2 stays held.
        apply_input_event(&mut m, &mut cache, &button(1, "up", false));
        let v = m.joystick_byte();
        assert_eq!(v & 0x02, 0, "P1 up released → bit 1 low");
        assert_eq!(v & 0x40, 0x40, "P2 left still held");
    }

    #[test]
    fn fire_button_is_ignored_no_joystick_trigger_line() {
        let mut m = make_m5();
        let mut cache = ControllerCache::default();
        apply_input_event(&mut m, &mut cache, &button(1, "fire", true));
        assert_eq!(m.joystick_byte(), 0x00, "M5 JOY port carries no fire line");
    }

    /// The M5 has a JIS keyboard and this table had been written as though it
    /// were a US one, so seven keycaps were named after the wrong character.
    /// Each typed something plausible, which is why none of it surfaced:
    /// asking for `[` gave `@`, `]` gave `[`, `:` gave `;`, `'` gave `:`,
    /// `=` gave `^`, `\` gave `]`, and `backspace` gave `\`.
    ///
    /// Established by sweeping all 64 cells against the Monitor ROM plus the
    /// BASIC-I cartridge and reading the echoed character out of VRAM.
    #[test]
    fn the_jis_keycaps_are_named_after_what_they_type() {
        assert_eq!(key_to_matrix("^"), Some((5, 3)));
        assert_eq!(key_to_matrix("\\"), Some((5, 7)));
        assert_eq!(key_to_matrix("@"), Some((6, 2)));
        assert_eq!(key_to_matrix("["), Some((6, 3)));
        assert_eq!(key_to_matrix(";"), Some((6, 5)));
        assert_eq!(key_to_matrix(":"), Some((6, 6)));
        assert_eq!(key_to_matrix("]"), Some((6, 7)));
    }

    /// `-` and `/` keep their own keycaps, so the JIS correction must not have
    /// moved them while shuffling their neighbours.
    #[test]
    fn the_keys_that_were_already_right_did_not_move() {
        assert_eq!(key_to_matrix("-"), Some((5, 2)));
        assert_eq!(key_to_matrix("."), Some((5, 4)));
        assert_eq!(key_to_matrix("/"), Some((5, 5)));
        assert_eq!(key_to_matrix("_"), Some((5, 6)));
        assert_eq!(key_to_matrix(","), Some((4, 7)));
    }

    /// `backspace` used to name `(5, 7)`, which types a `\`. No cell in this
    /// matrix was observed to delete a character — pressing `(0, 4)` and
    /// `(0, 5)` after typing `AB` left the `B` in place — so the name is gone
    /// rather than left pointing at a key that types a backslash. Refusing an
    /// unknown name is the behaviour #1196 asked for.
    #[test]
    fn backspace_is_refused_rather_than_typing_a_backslash() {
        for name in ["backspace", "bs", "delete", "del"] {
            assert_eq!(key_to_matrix(name), None, "{name} still resolves");
        }
    }

    /// Two names on one cell means one of them types the wrong character.
    #[test]
    fn no_two_key_names_share_a_cell() {
        const NAMES: &[&str] = &[
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "a", "b", "c", "d", "e", "f", "g",
            "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x",
            "y", "z", "-", "^", ".", "/", "_", ",", "@", "[", ";", ":", "]", "\\", "space",
            "enter", "shift", "ctrl", "func",
        ];
        let mut seen: Vec<((usize, u8), &str)> = Vec::new();
        for name in NAMES {
            let cell = key_to_matrix(name).unwrap_or_else(|| panic!("{name} does not resolve"));
            if let Some((_, other)) = seen.iter().find(|(c, _)| *c == cell) {
                panic!("{name:?} and {other:?} both claim cell {cell:?}");
            }
            seen.push((cell, name));
        }
    }
}
