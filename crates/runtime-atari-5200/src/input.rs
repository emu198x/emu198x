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
//! orientation-agnostic. The keypad (0-9, `*`, `#`, start/pause/reset) is
//! not yet exposed by `machine-atari-5200`; wiring it is deferred.

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

pub(crate) fn apply_input_event(
    machine: &mut Atari5200,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
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
    fn analog_axis_sets_pots_directly() {
        let mut cache = ControllerCache::default();
        assert!(cache.set_axis("x", i16::MAX));
        assert_eq!(cache.x, POT_MAX);
        assert!(cache.set_axis("vertical", i16::MIN));
        assert_eq!(cache.y, POT_MIN);
        assert!(!cache.set_axis("throttle", 0));
    }
}
