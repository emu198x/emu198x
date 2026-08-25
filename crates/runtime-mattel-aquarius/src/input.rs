//! Mattel Aquarius keyboard input mapping.
//!
//! The Aquarius has an 8×6 keyboard matrix. Host `Key` events are
//! mapped to (row, col) via a name table and dispatched through
//! `Aquarius::set_key`. The matrix is active-low; the machine handles
//! state internally.
//!
//! Key naming follows the lowercase host convention used by Spectrum /
//! MSX runtimes (`return`, `space`, `shift`, letter / digit names).
//!
//! Hand-controller input arrives as [`InputEvent::Button`] on a numbered port.
//! The eight host directions choose one of the disc's 16 positions and `fire`
//! is the first side button; the machine composes the controller code read
//! through the Mini Expander's AY. One [`ControllerCache`] mirror per port
//! re-applies the whole state via [`Aquarius::set_joystick`] on each event.

use emu198x_shell::InputEvent;
use machine_mattel_aquarius::Aquarius;

/// Host-side mirror of one Aquarius hand controller: four directions plus the
/// fire button, re-applied via [`Aquarius::set_joystick`] on every event.
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
    /// a direction or the fire button.
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

/// Host-side mirror of both Aquarius hand controllers (ports 1 and 2).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    ports: [JoystickCache; 2],
}

impl ControllerCache {
    /// Apply a `Button` event for `port` (1 or 2): record the control and push
    /// the whole port state to the machine. Out-of-range ports clamp to the
    /// valid pair, matching [`Aquarius::set_joystick`].
    fn apply_button(&mut self, machine: &mut Aquarius, port: u8, name: &str, pressed: bool) {
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
    machine: &mut Aquarius,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((row, col)) = key_to_matrix(name.as_ref()) {
                machine.set_key(row, col, *pressed);
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

/// Map a host-level key name to an Aquarius matrix (row, column).
///
/// Reference: Mattel Aquarius keyboard scancode table. Rows 0-7, columns
/// 0-5. Specific layout sourced from the machine crate's own internal
/// table; mirrored here for the host-facing translation.
#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0
        "=" | "equals" => (0, 0),
        "backspace" | "bs" => (0, 1),
        ":" => (0, 2),
        "return" | "enter" => (0, 3),
        ";" | "semicolon" => (0, 4),
        "." | "period" => (0, 5),
        // Row 1
        "-" | "minus" => (1, 0),
        "/" | "slash" => (1, 1),
        "0" => (1, 2),
        "p" => (1, 3),
        "l" => (1, 4),
        "," | "comma" => (1, 5),
        // Row 2
        "9" => (2, 0),
        "o" => (2, 1),
        "k" => (2, 2),
        "m" => (2, 3),
        "n" => (2, 4),
        "j" => (2, 5),
        // Row 3
        "8" => (3, 0),
        "i" => (3, 1),
        "7" => (3, 2),
        "u" => (3, 3),
        "h" => (3, 4),
        "b" => (3, 5),
        // Row 4
        "6" => (4, 0),
        "y" => (4, 1),
        "g" => (4, 2),
        "v" => (4, 3),
        "c" => (4, 4),
        "f" => (4, 5),
        // Row 5
        "5" => (5, 0),
        "t" => (5, 1),
        "4" => (5, 2),
        "r" => (5, 3),
        "d" => (5, 4),
        "x" => (5, 5),
        // Row 6
        "3" => (6, 0),
        "e" => (6, 1),
        "s" => (6, 2),
        "z" => (6, 3),
        "space" | " " => (6, 4),
        "a" => (6, 5),
        // Row 7
        "2" => (7, 0),
        "w" => (7, 1),
        "1" => (7, 2),
        "q" => (7, 3),
        "shift" | "lshift" | "rshift" => (7, 4),
        "ctrl" | "control" => (7, 5),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use machine_mattel_aquarius::AquariusRegion;
    use std::borrow::Cow;

    fn make_aquarius() -> Aquarius {
        Aquarius::new(vec![0u8; 0x4000], 0, AquariusRegion::Ntsc)
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    #[test]
    fn button_events_compose_the_controller_codes() {
        let mut m = make_aquarius();
        let mut cache = ControllerCache::default();

        // Player 1 (port B = controller_bytes[1]) up → disc 12:00 (0xFB).
        apply_input_event(&mut m, &mut cache, &button(1, "up", true));
        assert_eq!(m.controller_bytes()[1], 0xFB, "P1 up → disc 12:00");

        // Adding fire ANDs in side button 1 (0xBF).
        apply_input_event(&mut m, &mut cache, &button(1, "fire", true));
        assert_eq!(m.controller_bytes()[1], 0xFB & 0xBF, "P1 up + fire");

        // Player 2 (port A = controller_bytes[0]) left → disc 09:00 (0xF7).
        apply_input_event(&mut m, &mut cache, &button(2, "left", true));
        assert_eq!(m.controller_bytes()[0], 0xF7, "P2 left → disc 09:00");

        // Releasing P1 up leaves fire held → button-only code 0xBF.
        apply_input_event(&mut m, &mut cache, &button(1, "up", false));
        assert_eq!(m.controller_bytes()[1], 0xBF, "P1 fire only");
    }
}

/// Whether this machine's input layer can deliver `name`.
///
/// This is the same lookup [`apply_input_event`] performs before injecting a
/// keystroke, exposed so the shared keyboard can refuse a character the
/// machine cannot type instead of counting one it silently dropped (#1196).
pub(crate) fn knows_key_name(name: &str) -> bool {
    key_to_matrix(name).is_some()
}
