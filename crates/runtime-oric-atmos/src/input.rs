//! Oric-1 / Atmos keyboard input mapping.
//!
//! The Oric keyboard is an 8×8 matrix: VIA port B bits 0-2 select the
//! column, the scan routine drives one row low on VIA port A, and the
//! sense returns on VIA PB3. The machine exposes press/release by
//! `(col, row)`; this module maps host key names to that pair.
//!
//! Every position was probed against the real Oric BASIC ROM (press the
//! cell, read the echoed glyph from screen RAM) and cross-checked against
//! MAME's `tangerine/oric.cpp` `ROW0`-`ROW7` ports.
//!
//! Joystick input arrives as [`InputEvent::Button`] on a numbered port and
//! drives the IJK interface — the de-facto Oric joystick — through
//! [`OricAtmos::set_joystick`]. Port 1 is the left stick, port 2 the right.
//! One [`ControllerCache`] mirror per port re-applies the whole state on each
//! event.

use emu198x_shell::InputEvent;
use machine_oric_atmos::OricAtmos;

/// Host-side mirror of one IJK stick: four directions plus the fire button,
/// re-applied via [`OricAtmos::set_joystick`] on every event.
#[derive(Clone, Copy, Debug, Default)]
struct JoystickCache {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

impl JoystickCache {
    /// Record a digital control by name. Returns `true` when the name maps to
    /// a joystick direction or the fire button.
    fn set_control(&mut self, name: &str, pressed: bool) -> bool {
        match name {
            "up" | "arrowup" => self.up = pressed,
            "down" | "arrowdown" => self.down = pressed,
            "left" | "arrowleft" => self.left = pressed,
            "right" | "arrowright" => self.right = pressed,
            "fire" | "fire1" | "trigger" | "button" => self.fire = pressed,
            _ => return false,
        }
        true
    }
}

/// Host-side mirror of both IJK joystick ports (1 = left, 2 = right).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    ports: [JoystickCache; 2],
}

impl ControllerCache {
    /// Apply a `Button` event for `port` (1 or 2): record the control and push
    /// the whole port state to the machine. Out-of-range ports clamp to the
    /// valid pair, matching [`OricAtmos::set_joystick`].
    fn apply_button(&mut self, machine: &mut OricAtmos, port: u8, name: &str, pressed: bool) {
        let port = port.clamp(1, 2);
        let cache = &mut self.ports[usize::from(port - 1)];
        if cache.set_control(name, pressed) {
            machine.set_joystick(
                port,
                cache.up,
                cache.down,
                cache.left,
                cache.right,
                cache.fire,
            );
        }
    }
}

pub(crate) fn apply_input_event(
    machine: &mut OricAtmos,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((col, row)) = key_to_matrix(name.as_ref()) {
                if *pressed {
                    machine.press_key(col, row);
                } else {
                    machine.release_key(col, row);
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
        // Letters.
        "a" => (6, 5),
        "b" => (2, 2),
        "c" => (2, 7),
        "d" => (1, 7),
        "e" => (6, 3),
        "f" => (1, 3),
        "g" => (6, 2),
        "h" => (6, 1),
        "i" => (5, 1),
        "j" => (1, 0),
        "k" => (3, 0),
        "l" => (7, 1),
        "m" => (2, 0),
        "n" => (0, 1),
        "o" => (5, 2),
        "p" => (5, 3),
        "q" => (1, 6),
        "r" => (1, 2),
        "s" => (6, 6),
        "t" => (1, 1),
        "u" => (5, 0),
        "v" => (0, 3),
        "w" => (6, 7),
        "x" => (0, 6),
        "y" => (6, 0),
        "z" => (2, 5),
        // Digits.
        "0" => (7, 2),
        "1" => (0, 5),
        "2" => (2, 6),
        "3" => (0, 7),
        "4" => (2, 3),
        "5" => (0, 2),
        "6" => (2, 1),
        "7" => (0, 0),
        "8" => (7, 0),
        "9" => (3, 1),
        // Punctuation (unshifted legends).
        "," | "comma" => (4, 1),
        "." | "period" | "stop" => (4, 2),
        ";" | "semicolon" | ":" | "colon" => (3, 2),
        "-" | "minus" => (3, 3),
        "'" | "quote" | "apostrophe" => (3, 7),
        "\\" | "backslash" => (3, 6),
        "/" | "slash" => (7, 3),
        "=" | "equals" | "equal" => (7, 7),
        "[" | "leftbracket" | "openbracket" => (5, 7),
        "]" | "rightbracket" | "closebracket" => (5, 6),
        // Control / editing.
        "space" | " " => (4, 0),
        "return" | "enter" => (7, 5),
        "shift" | "lshift" => (4, 4),
        "rshift" => (7, 4),
        "ctrl" | "control" => (2, 4),
        "delete" | "del" | "backspace" | "bs" => (5, 5),
        "left" | "arrowleft" => (4, 5),
        "right" | "arrowright" => (4, 7),
        "up" | "arrowup" => (4, 3),
        "down" | "arrowdown" => (4, 6),
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

    #[test]
    fn maps_ground_truthed_cells() {
        // Verified by typing HELLO against the real Oric ROM.
        assert_eq!(key_to_matrix("h"), Some((6, 1)));
        assert_eq!(key_to_matrix("e"), Some((6, 3)));
        assert_eq!(key_to_matrix("l"), Some((7, 1)));
        assert_eq!(key_to_matrix("o"), Some((5, 2)));
        assert_eq!(key_to_matrix("return"), Some((7, 5)));
        assert_eq!(key_to_matrix("SPACE"), Some((4, 0))); // case-insensitive
    }

    #[test]
    fn unmapped_key_returns_none() {
        assert_eq!(key_to_matrix("f1"), None);
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: std::borrow::Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    #[test]
    fn button_events_drive_the_ijk_sticks() {
        use machine_oric_atmos::OricModel;
        let mut m = OricAtmos::new(vec![0u8; 0x4000], OricModel::Atmos);
        let mut cache = ControllerCache::default();

        // Left stick (port 1) up + fire: bit 4 (up) and bit 2 (fire) low.
        apply_input_event(&mut m, &mut cache, &button(1, "up", true));
        apply_input_event(&mut m, &mut cache, &button(1, "fire", true));
        let mask = m.joystick_port_mask(1);
        assert_eq!(mask & 0x10, 0, "left up → bit 4 low");
        assert_eq!(mask & 0x04, 0, "left fire → bit 2 low");
        assert_eq!(mask & 0x01, 0x01, "left right idle high");

        // Right stick (port 2) is independent.
        apply_input_event(&mut m, &mut cache, &button(2, "right", true));
        assert_eq!(
            m.joystick_port_mask(2) & 0x01,
            0,
            "right stick right → bit 0 low"
        );
        assert_eq!(m.joystick_port_mask(1) & 0x10, 0, "left stick still held");

        // Releasing left up restores its bit; fire stays held.
        apply_input_event(&mut m, &mut cache, &button(1, "up", false));
        let mask = m.joystick_port_mask(1);
        assert_eq!(mask & 0x10, 0x10, "left up released → bit 4 high");
        assert_eq!(mask & 0x04, 0, "left fire still held");
    }
}
