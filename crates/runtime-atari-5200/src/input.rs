//! Atari 5200 analogue-joystick / fire input mapping.
//!
//! Unlike the digital Atari sticks, the 5200 controller is *analogue*: the
//! machine reads X / Y through POKEY pots (`set_joystick(x, y)`, each
//! `0..=228` with `114` centre) plus a GTIA fire trigger (`set_fire`). So
//! this consumer accepts three event shapes:
//!
//!   * [`InputEvent::Axis`] — the natural fit for the analogue stick:
//!     `x` / `horizontal` drives pot 0, `y` / `vertical` drives pot 1.
//!   * [`InputEvent::Button`] / [`InputEvent::Key`] — digital direction
//!     names (`up` / `down` / `left` / `right`) snap the pot to an extreme,
//!     and `fire` drives the trigger.
//!
//! Direction convention: `left` / `up` → pot minimum, `right` / `down` →
//! pot maximum, neither (or both) on an axis → centre. If a title reads
//! inverted, flip the two constants below — the wiring is otherwise
//! orientation-agnostic.
//!
//! The controller keypad (`start`, `pause`, `reset`, `0`-`9`, `*`, `#`) is
//! momentary, not a held state, so it bypasses the analog/fire cache: a press
//! latches the key's POKEY scan code, a release frees it.

use emu198x_shell::InputEvent;
use machine_atari_5200::Atari5200;

const POT_MIN: u8 = 0;
const POT_CENTRE: u8 = 114;
const POT_MAX: u8 = 228;

/// Host-side mirror of the analogue stick + fire.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ControllerCache {
    x: u8,
    y: u8,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

impl Default for ControllerCache {
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

impl ControllerCache {
    fn apply(self, machine: &mut Atari5200) {
        machine.set_joystick(self.x, self.y);
        machine.set_fire(self.fire);
    }

    /// Recompute the pot values from the held digital directions.
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

    /// Handle a digital direction / fire name. Returns `true` when it
    /// mapped to a control (machine needs a re-apply).
    fn set_digital(&mut self, name: &str, pressed: bool) -> bool {
        match name {
            "up" | "arrowup" => self.up = pressed,
            "down" | "arrowdown" => self.down = pressed,
            "left" | "arrowleft" => self.left = pressed,
            "right" | "arrowright" => self.right = pressed,
            "fire" | "fire1" | "button" => {
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
        let pot = axis_to_pot(value);
        match name {
            "x" | "horizontal" | "pot0" => self.x = pot,
            "y" | "vertical" | "pot1" => self.y = pot,
            _ => return false,
        }
        true
    }
}

/// Map a normalized signed axis value (`i16::MIN..=i16::MAX`) onto the
/// POKEY pot range (`0..=228`); `0` lands on centre (`114`).
fn axis_to_pot(value: i16) -> u8 {
    let shifted = i32::from(value) - i32::from(i16::MIN); // 0..=65535
    u8::try_from((shifted * i32::from(POT_MAX)) / 65535).unwrap_or(POT_MAX)
}

/// The POKEY keyboard scan code for a keypad key name, per MAME
/// `a5200_keypads`: the 4×4 matrix position encoded as `((row << 2) | col) << 1`.
fn keypad_code(name: &str) -> Option<u8> {
    Some(match name {
        // row 3
        "start" => 0x18,
        "3" => 0x1A,
        "2" => 0x1C,
        "1" => 0x1E,
        // row 2
        "pause" => 0x10,
        "6" => 0x12,
        "5" => 0x14,
        "4" => 0x16,
        // row 1
        "reset" => 0x08,
        "9" => 0x0A,
        "8" => 0x0C,
        "7" => 0x0E,
        // row 0
        "#" | "hash" | "pound" => 0x02,
        "0" => 0x04,
        "*" | "star" | "asterisk" => 0x06,
        _ => return None,
    })
}

pub(crate) fn apply_input_event(
    machine: &mut Atari5200,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    // Keypad keys are momentary and POKEY-latched, not part of the held cache.
    if let InputEvent::Button { name, pressed, .. } | InputEvent::Key { name, pressed } = event
        && let Some(code) = keypad_code(&name.to_ascii_lowercase())
    {
        machine.set_keypad(code, *pressed);
        return;
    }

    let changed = match event {
        InputEvent::Axis { name, value, .. } => cache.set_axis(&name.to_ascii_lowercase(), *value),
        InputEvent::Button { name, pressed, .. } => {
            cache.set_digital(&name.to_ascii_lowercase(), *pressed)
        }
        InputEvent::Key { name, pressed } => {
            cache.set_digital(&name.to_ascii_lowercase(), *pressed)
        }
        _ => false,
    };
    if changed {
        cache.apply(machine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_centre_and_extremes() {
        assert_eq!(axis_to_pot(0), POT_CENTRE);
        assert_eq!(axis_to_pot(i16::MIN), POT_MIN);
        assert_eq!(axis_to_pot(i16::MAX), POT_MAX);
    }

    #[test]
    fn digital_directions_snap_to_extremes() {
        let mut cache = ControllerCache::default();
        assert!(cache.set_digital("left", true));
        assert_eq!(cache.x, POT_MIN);
        assert!(cache.set_digital("left", false));
        assert_eq!(cache.x, POT_CENTRE);
        assert!(cache.set_digital("down", true));
        assert_eq!(cache.y, POT_MAX);
        assert!(cache.set_digital("fire", true));
        assert!(cache.fire);
        assert!(!cache.set_digital("keypad5", true));
    }

    #[test]
    fn keypad_codes_match_the_mame_matrix() {
        // ((row << 2) | col) << 1. Start = row 3, col 0 = 0x18.
        assert_eq!(keypad_code("start"), Some(0x18));
        assert_eq!(keypad_code("reset"), Some(0x08));
        assert_eq!(keypad_code("pause"), Some(0x10));
        assert_eq!(keypad_code("0"), Some(0x04));
        assert_eq!(keypad_code("1"), Some(0x1E));
        assert_eq!(keypad_code("*"), Some(0x06));
        assert_eq!(keypad_code("up"), None);
    }

    #[test]
    fn analog_axis_sets_pots_directly() {
        let mut cache = ControllerCache::default();
        assert!(cache.set_axis("x", i16::MAX));
        assert_eq!(cache.x, POT_MAX);
        assert!(cache.set_axis("vertical", i16::MIN));
        assert_eq!(cache.y, POT_MIN);
        assert!(!cache.set_axis("throttle", 0));
    }
}
