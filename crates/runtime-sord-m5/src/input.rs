//! Sord M5 keyboard input mapping.
//!
//! The M5 has a 7-row × 7-bit matrix scanned through I/O port `$30`.
//! Host-level key names map to (row, bit) via this table. The matrix
//! layout is sourced from the M5 service manual; key positions match
//! the machine crate's internal scan convention.
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

#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0: digits
        "1" => (0, 0),
        "2" => (0, 1),
        "3" => (0, 2),
        "4" => (0, 3),
        "5" => (0, 4),
        "6" => (0, 5),
        "7" => (0, 6),
        // Row 1
        "8" => (1, 0),
        "9" => (1, 1),
        "0" => (1, 2),
        "-" | "minus" => (1, 3),
        "^" | "caret" => (1, 4),
        "\\" | "yen" | "backslash" => (1, 5),
        "delete" | "del" | "backspace" | "bs" => (1, 6),
        // Row 2
        "q" => (2, 0),
        "w" => (2, 1),
        "e" => (2, 2),
        "r" => (2, 3),
        "t" => (2, 4),
        "y" => (2, 5),
        "u" => (2, 6),
        // Row 3
        "i" => (3, 0),
        "o" => (3, 1),
        "p" => (3, 2),
        "@" | "at" => (3, 3),
        "[" | "leftbracket" => (3, 4),
        "return" | "enter" => (3, 5),
        "a" => (3, 6),
        // Row 4
        "s" => (4, 0),
        "d" => (4, 1),
        "f" => (4, 2),
        "g" => (4, 3),
        "h" => (4, 4),
        "j" => (4, 5),
        "k" => (4, 6),
        // Row 5
        "l" => (5, 0),
        ";" | "semicolon" => (5, 1),
        ":" => (5, 2),
        "]" | "rightbracket" => (5, 3),
        "shift" | "lshift" | "rshift" => (5, 4),
        "z" => (5, 5),
        "x" => (5, 6),
        // Row 6
        "c" => (6, 0),
        "v" => (6, 1),
        "b" => (6, 2),
        "n" => (6, 3),
        "m" => (6, 4),
        "," | "comma" => (6, 5),
        "." | "period" => (6, 6),
        _ => return None,
    })
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
}
