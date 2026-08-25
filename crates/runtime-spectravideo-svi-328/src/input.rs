//! Spectravideo SVI-328 keyboard input mapping.
//!
//! The SVI keyboard is an 11-row × 8-column matrix scanned through the
//! Intel 8255 PPI: port C selects the row, port B reads the eight
//! columns back active-low (a pressed key pulls its bit to 0). The
//! machine exposes press/release by `(row, bit)`; this module maps host
//! key names to that pair.
//!
//! The table is transcribed from MAME's `svi/svi318.cpp` `KEY.0`-`KEY.8`
//! port definitions and ground-truthed against the real SV-BASIC ROM
//! (pressing each cell and reading the echoed character from VRAM).
//!
//! Joystick input arrives as [`InputEvent::Button`] on a numbered port. The
//! SVI reads both control ports at once: the directions sit on PSG port A
//! (player 1 low nibble, player 2 high nibble) and the fire buttons on PPI
//! port A bits 4-5, all active low (MAME `svi318` `port_a_read` / `ppi_port_a_r`).
//! One [`ControllerCache`] mirror per port re-applies the whole state via
//! [`Svi328::set_joystick`] on each event.

use emu198x_shell::InputEvent;
use machine_spectravideo_svi_328::Svi328;

/// Host-side mirror of one SVI control port: four directions plus the fire
/// button, re-applied via [`Svi328::set_joystick`] on every event.
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

/// Host-side mirror of both SVI control ports (1 and 2), indexed `port - 1`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    ports: [JoystickCache; 2],
}

impl ControllerCache {
    /// Apply a `Button` event for `port` (1 or 2): record the control and push
    /// the whole port state to the machine. Out-of-range ports clamp to the
    /// valid pair, matching [`Svi328::set_joystick`].
    fn apply_button(&mut self, machine: &mut Svi328, port: u8, name: &str, pressed: bool) {
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

/// Apply one host input event. `Key` events drive the keyboard matrix;
/// `Button` events drive the joystick on their numbered port.
pub(crate) fn apply_input_event(
    machine: &mut Svi328,
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
        // KEY.0 — digits 0-7
        "0" => (0, 0),
        "1" => (0, 1),
        "2" => (0, 2),
        "3" => (0, 3),
        "4" => (0, 4),
        "5" => (0, 5),
        "6" => (0, 6),
        "7" => (0, 7),
        // KEY.1 — digits 8-9 and punctuation
        "8" => (1, 0),
        "9" => (1, 1),
        ":" | "colon" => (1, 2),
        ";" | "semicolon" => (1, 2),
        "'" | "quote" | "apostrophe" => (1, 3),
        "," | "comma" => (1, 4),
        "=" | "equals" | "equal" => (1, 5),
        "." | "period" | "stop" => (1, 6),
        "/" | "slash" => (1, 7),
        // KEY.2 — minus, A-G
        "-" | "minus" => (2, 0),
        "a" => (2, 1),
        "b" => (2, 2),
        "c" => (2, 3),
        "d" => (2, 4),
        "e" => (2, 5),
        "f" => (2, 6),
        "g" => (2, 7),
        // KEY.3 — H-O
        "h" => (3, 0),
        "i" => (3, 1),
        "j" => (3, 2),
        "k" => (3, 3),
        "l" => (3, 4),
        "m" => (3, 5),
        "n" => (3, 6),
        "o" => (3, 7),
        // KEY.4 — P-W
        "p" => (4, 0),
        "q" => (4, 1),
        "r" => (4, 2),
        "s" => (4, 3),
        "t" => (4, 4),
        "u" => (4, 5),
        "v" => (4, 6),
        "w" => (4, 7),
        // KEY.5 — X-Z, brackets, backspace, up
        "x" => (5, 0),
        "y" => (5, 1),
        "z" => (5, 2),
        "[" | "leftbracket" | "openbracket" => (5, 3),
        "\\" | "backslash" => (5, 4),
        "]" | "rightbracket" | "closebracket" => (5, 5),
        "backspace" | "bs" => (5, 6),
        "up" | "arrowup" => (5, 7),
        // KEY.6 — shift, control, alt, esc, return, left
        "shift" | "lshift" | "rshift" => (6, 0),
        "ctrl" | "control" | "lcontrol" => (6, 1),
        "alt" | "lalt" => (6, 2),
        "ralt" | "graph" | "code" => (6, 3),
        "escape" | "esc" => (6, 4),
        "end" | "stopkey" => (6, 5),
        "return" | "enter" => (6, 6),
        "left" | "arrowleft" => (6, 7),
        // KEY.7 — function keys, home/copy, insert, down
        "f1" => (7, 0),
        "f2" => (7, 1),
        "f3" => (7, 2),
        "f4" => (7, 3),
        "f5" => (7, 4),
        "home" | "copy" => (7, 5),
        "insert" | "ins" => (7, 6),
        "down" | "arrowdown" => (7, 7),
        // KEY.8 — space, tab, del, caps, pause, print, right
        "space" | " " => (8, 0),
        "tab" => (8, 1),
        "delete" | "del" => (8, 2),
        "caps" | "capslock" => (8, 3),
        "pause" => (8, 4),
        "print" | "printscreen" | "prtscr" => (8, 5),
        "right" | "arrowright" => (8, 7),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_to_ground_truthed_cells() {
        // Verified against the real SV-BASIC ROM (typing HELLO).
        assert_eq!(key_to_matrix("h"), Some((3, 0)));
        assert_eq!(key_to_matrix("e"), Some((2, 5)));
        assert_eq!(key_to_matrix("l"), Some((3, 4)));
        assert_eq!(key_to_matrix("o"), Some((3, 7)));
    }

    #[test]
    fn maps_control_and_symbol_keys() {
        assert_eq!(key_to_matrix("return"), Some((6, 6)));
        assert_eq!(key_to_matrix("space"), Some((8, 0)));
        assert_eq!(key_to_matrix("shift"), Some((6, 0)));
        assert_eq!(key_to_matrix("-"), Some((2, 0)));
        // Names are case-insensitive.
        assert_eq!(key_to_matrix("Return"), Some((6, 6)));
    }

    #[test]
    fn unmapped_key_returns_none() {
        assert_eq!(key_to_matrix("scroll_lock"), None);
    }

    #[test]
    fn event_presses_and_releases_matrix() {
        let rom = vec![0u8; 32 * 1024];
        let mut machine = Svi328::new(rom, machine_spectravideo_svi_328::SviRegion::Pal);
        let mut cache = ControllerCache::default();
        apply_input_event(
            &mut machine,
            &mut cache,
            &InputEvent::Key {
                name: "a".into(),
                pressed: true,
            },
        );
        // A = (row 2, bit 1): pressing pulls the column bit low.
        assert_eq!(machine.key_row(2) & 0b0000_0010, 0);
        apply_input_event(
            &mut machine,
            &mut cache,
            &InputEvent::Key {
                name: "a".into(),
                pressed: false,
            },
        );
        assert_eq!(machine.key_row(2) & 0b0000_0010, 0b0000_0010);
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: std::borrow::Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    #[test]
    fn button_events_drive_both_joystick_ports() {
        let rom = vec![0u8; 32 * 1024];
        let mut machine = Svi328::new(rom, machine_spectravideo_svi_328::SviRegion::Pal);
        let mut cache = ControllerCache::default();

        // Player 1 left → PSG port A low-nibble bit 2; player 2 right →
        // high-nibble bit 7. The two ports are independent.
        apply_input_event(&mut machine, &mut cache, &button(1, "left", true));
        apply_input_event(&mut machine, &mut cache, &button(2, "right", true));
        let dirs = machine.joystick_dirs();
        assert_eq!(dirs & 0x04, 0, "P1 left → bit 2 low");
        assert_eq!(dirs & 0x80, 0, "P2 right → bit 7 low");
        assert_eq!(dirs & 0x08, 0x08, "P1 right idle high");

        // Releasing P1 left restores its bit; P2 stays held.
        apply_input_event(&mut machine, &mut cache, &button(1, "left", false));
        let dirs = machine.joystick_dirs();
        assert_eq!(dirs & 0x04, 0x04, "P1 left released → bit 2 high");
        assert_eq!(dirs & 0x80, 0, "P2 right still held");
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
