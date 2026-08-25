//! Tatung Einstein keyboard input mapping.
//!
//! 8×8 matrix scanned through the AY-3-8910 I/O ports (row select on
//! port A, columns on port B). Every position below was probed against
//! the real X-TAL MOS ROM — press the cell, read the echoed character —
//! and cross-checked against MAME's `tatung/einstein.cpp` key matrix for
//! the non-printing keys (row 0). The donor's table was a placeholder and
//! did not match the hardware.
//!
//! The Einstein joysticks are **analogue**: X/Y are read through an ADC0844
//! and the fire buttons on port `$20`. So this consumer accepts
//! [`InputEvent::Axis`] for the true analogue axes (`x`/`y` per port → the
//! ADC channels) and [`InputEvent::Button`] for digital direction names
//! (which snap a pot to its extreme) and `fire`. One [`ControllerCache`]
//! mirror per port re-applies the whole stick via the machine's setters.

use emu198x_shell::InputEvent;
use machine_tatung_einstein::{Einstein, Modifier};

const POT_MIN: u8 = 0x00;
const POT_CENTRE: u8 = 0x80;
const POT_MAX: u8 = 0xFF;

/// Host-side mirror of one analogue stick: the two pot values plus the held
/// digital directions and fire button.
#[derive(Clone, Copy, Debug)]
struct Stick {
    x: u8,
    y: u8,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

impl Default for Stick {
    fn default() -> Self {
        Self {
            x: POT_CENTRE,
            y: POT_CENTRE,
            up: false,
            down: false,
            left: false,
            right: false,
            fire: false,
        }
    }
}

impl Stick {
    /// Recompute the pot values from the held digital directions: `left`/`up`
    /// snap to the minimum, `right`/`down` to the maximum, neither to centre.
    fn recompute_from_digital(&mut self) {
        self.x = match (self.left, self.right) {
            (true, false) => POT_MIN,
            (false, true) => POT_MAX,
            _ => POT_CENTRE,
        };
        self.y = match (self.up, self.down) {
            (true, false) => POT_MIN,
            (false, true) => POT_MAX,
            _ => POT_CENTRE,
        };
    }

    /// Handle a digital direction / fire name. Returns `true` when it mapped.
    fn set_digital(&mut self, name: &str, pressed: bool) -> bool {
        match name {
            "up" | "arrowup" => self.up = pressed,
            "down" | "arrowdown" => self.down = pressed,
            "left" | "arrowleft" => self.left = pressed,
            "right" | "arrowright" => self.right = pressed,
            "fire" | "fire1" | "trigger" | "button" => {
                self.fire = pressed;
                return true;
            }
            _ => return false,
        }
        self.recompute_from_digital();
        true
    }

    /// Handle an analogue axis. Returns `true` when the axis name is known.
    fn set_axis(&mut self, name: &str, value: i16) -> bool {
        let pot = axis_to_pot8(value);
        match name {
            "x" | "horizontal" | "pot0" => self.x = pot,
            "y" | "vertical" | "pot1" => self.y = pot,
            _ => return false,
        }
        true
    }
}

/// Host-side mirror of both Einstein joysticks (ports 1 and 2).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    ports: [Stick; 2],
}

impl ControllerCache {
    /// Push the stick for `port` (1 or 2) to the machine: the two pot values to
    /// its ADC channels and the fire button to its `$20` line.
    fn apply(&self, machine: &mut Einstein, port: u8) {
        let idx = usize::from(port.clamp(1, 2) - 1);
        let stick = self.ports[idx];
        let base = u8::try_from(idx).unwrap_or(0) * 2;
        machine.set_adc_channel(base, stick.x);
        machine.set_adc_channel(base + 1, stick.y);
        machine.set_fire_button(port, stick.fire);
    }

    /// Apply a `Button` (digital direction / fire) event on `port`.
    fn apply_button(&mut self, machine: &mut Einstein, port: u8, name: &str, pressed: bool) {
        let idx = usize::from(port.clamp(1, 2) - 1);
        if self.ports[idx].set_digital(name, pressed) {
            self.apply(machine, port);
        }
    }

    /// Apply an `Axis` event on `port`.
    fn apply_axis(&mut self, machine: &mut Einstein, port: u8, name: &str, value: i16) {
        let idx = usize::from(port.clamp(1, 2) - 1);
        if self.ports[idx].set_axis(name, value) {
            self.apply(machine, port);
        }
    }
}

/// Scale a normalized signed axis (`i16::MIN..=i16::MAX`) onto the ADC's 8-bit
/// range (`0..=255`); `0` lands near centre (`128`).
fn axis_to_pot8(value: i16) -> u8 {
    let shifted = i32::from(value) - i32::from(i16::MIN); // 0..=65535
    u8::try_from((shifted * 255) / 65535).unwrap_or(255)
}

pub(crate) fn apply_input_event(
    machine: &mut Einstein,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Key { name, pressed } => {
            // SHIFT/CTRL/GRAPH are not scanned matrix cells — the hardware reads
            // them as bits 5-7 of the `$20` status port — so they route to the
            // modifier register, not `key_to_matrix`.
            if let Some(modifier) = key_to_modifier(name.as_ref()) {
                machine.set_modifier(modifier, *pressed);
            } else if let Some((row, col)) = key_to_matrix(name.as_ref()) {
                if *pressed {
                    machine.press_key(row, col);
                } else {
                    machine.release_key(row, col);
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
        InputEvent::Axis { port, name, value } => {
            cache.apply_axis(machine, *port, &name.to_ascii_lowercase(), *value);
        }
        _ => {}
    }
}

/// Map a modifier key name to its [`Modifier`]. These keys are read on `$20`
/// bits 5-7, separate from the AY-scanned 8×8 matrix, so they never appear in
/// [`key_to_matrix`].
#[must_use]
fn key_to_modifier(name: &str) -> Option<Modifier> {
    Some(match name.to_ascii_lowercase().as_str() {
        "shift" | "lshift" | "rshift" => Modifier::Shift,
        "ctrl" | "control" | "lctrl" | "rctrl" | "lcontrol" | "rcontrol" => Modifier::Control,
        "graph" | "grph" => Modifier::Graph,
        _ => return None,
    })
}

#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, u8)> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0 — non-printing keys (MAME LINE0).
        "return" | "enter" => (0, 5),
        "space" | " " => (0, 6),
        "escape" | "esc" => (0, 7),
        // Letters.
        "a" => (6, 6),
        "b" => (7, 2),
        "c" => (7, 4),
        "d" => (6, 4),
        "e" => (5, 4),
        "f" => (6, 3),
        "g" => (6, 2),
        "h" => (6, 1),
        "i" => (1, 0),
        "j" => (6, 0),
        "k" => (2, 0),
        "l" => (2, 1),
        "m" => (7, 0),
        "n" => (7, 1),
        "o" => (1, 1),
        "p" => (1, 2),
        "q" => (5, 6),
        "r" => (5, 3),
        "s" => (6, 5),
        "t" => (5, 2),
        "u" => (5, 0),
        "v" => (7, 3),
        "w" => (5, 5),
        "x" => (7, 5),
        "y" => (5, 1),
        "z" => (7, 6),
        // Digits.
        "0" => (1, 7),
        "1" => (4, 6),
        "2" => (4, 5),
        "3" => (4, 4),
        "4" => (4, 3),
        "5" => (4, 2),
        "6" => (4, 1),
        "7" => (4, 0),
        "8" => (3, 3),
        "9" => (2, 6),
        // Punctuation (unshifted legends).
        ";" | "semicolon" => (2, 2),
        ":" | "colon" => (2, 3),
        "," | "comma" => (3, 0),
        "." | "period" => (3, 1),
        "/" | "slash" => (3, 2),
        "=" | "equals" | "equal" => (3, 5),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use machine_tatung_einstein::EinsteinRegion;
    use std::borrow::Cow;

    fn make_einstein() -> Einstein {
        let mut rom = vec![0u8; 0x2000];
        rom[0x0000] = 0x18; // JR -2 trap
        rom[0x0001] = 0xFE;
        Einstein::new(rom, EinsteinRegion::Pal)
    }

    fn axis(port: u8, name: &str, value: i16) -> InputEvent {
        InputEvent::Axis {
            port,
            name: Cow::Owned(name.to_owned()),
            value,
        }
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    fn key(name: &str, pressed: bool) -> InputEvent {
        InputEvent::Key {
            name: Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    #[test]
    fn modifier_keys_drive_the_status_register() {
        let mut m = make_einstein();
        let mut cache = ControllerCache::default();

        // Idle: no modifier held.
        assert!(!m.modifier_held(Modifier::Shift));

        // SHIFT registers while held and clears on release.
        apply_input_event(&mut m, &mut cache, &key("shift", true));
        assert!(m.modifier_held(Modifier::Shift), "shift held");
        apply_input_event(&mut m, &mut cache, &key("shift", false));
        assert!(!m.modifier_held(Modifier::Shift), "shift released");

        // CTRL and GRAPH resolve through their aliases too.
        apply_input_event(&mut m, &mut cache, &key("control", true));
        apply_input_event(&mut m, &mut cache, &key("grph", true));
        assert!(m.modifier_held(Modifier::Control), "ctrl held");
        assert!(m.modifier_held(Modifier::Graph), "graph held");
    }

    #[test]
    fn modifiers_and_matrix_keys_route_to_disjoint_paths() {
        // A modifier resolves to the status-port register and never to a matrix
        // cell; a letter is the reverse. The two never collide.
        assert_eq!(key_to_modifier("shift"), Some(Modifier::Shift));
        assert_eq!(key_to_modifier("ctrl"), Some(Modifier::Control));
        assert!(
            key_to_matrix("shift").is_none(),
            "shift is not a matrix cell"
        );

        assert!(key_to_modifier("a").is_none(), "'a' is not a modifier");
        assert_eq!(key_to_matrix("a"), Some((6, 6)), "'a' is a matrix cell");
    }

    #[test]
    fn axis_scales_and_routes_to_the_right_adc_channels() {
        assert_eq!(axis_to_pot8(i16::MIN), 0);
        assert_eq!(axis_to_pot8(i16::MAX), 255);

        let mut m = make_einstein();
        let mut cache = ControllerCache::default();
        // Port 1 X/Y → ADC channels 0/1; port 2 → channels 2/3.
        apply_input_event(&mut m, &mut cache, &axis(1, "x", i16::MAX));
        apply_input_event(&mut m, &mut cache, &axis(2, "y", i16::MIN));
        assert_eq!(m.adc_channel(0), 0xFF, "p1 X max → ch0");
        assert_eq!(m.adc_channel(3), 0x00, "p2 Y min → ch3");
        assert_eq!(m.adc_channel(2), 0x80, "p2 X untouched (centre)");
    }

    #[test]
    fn digital_directions_snap_the_pots_to_extremes() {
        let mut m = make_einstein();
        let mut cache = ControllerCache::default();
        apply_input_event(&mut m, &mut cache, &button(1, "left", true));
        assert_eq!(m.adc_channel(0), 0x00, "p1 left → X min");
        apply_input_event(&mut m, &mut cache, &button(1, "left", false));
        assert_eq!(m.adc_channel(0), 0x80, "released → X centre");
    }
}

/// Whether this machine's input layer can deliver `name`.
///
/// This is the same lookup [`apply_input_event`] performs before injecting a
/// keystroke, exposed so the shared keyboard can refuse a character the
/// machine cannot type instead of counting one it silently dropped (#1196).
pub(crate) fn knows_key_name(name: &str) -> bool {
    key_to_matrix(name).is_some() || key_to_modifier(name).is_some()
}
