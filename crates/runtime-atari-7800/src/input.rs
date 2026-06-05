//! Atari 7800 joystick / console-switch input mapping.
//!
//! `machine-atari-7800` exposes *full-state* setters — `set_joystick(up,
//! down, left, right)` (P0, active-low on RIOT port A) and
//! `set_console(reset, select, pause)` — so the host cache mirrors the
//! current control state and re-applies the whole vector on every event.
//!
//! Both [`InputEvent::Button`] (the canonical joystick door) and
//! [`InputEvent::Key`] (keyboard-as-pad convenience) are accepted. The two
//! fire buttons per 7800 controller and the P1 port are not yet exposed by
//! `machine-atari-7800`; wiring them is deferred until setters exist.

use emu198x_shell::InputEvent;
use machine_atari_7800::Atari7800;

/// Host-side mirror of the P0 joystick directions + console switches.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    reset: bool,
    select: bool,
    pause: bool,
}

impl ControllerCache {
    fn apply(self, machine: &mut Atari7800) {
        machine.set_joystick(self.up, self.down, self.left, self.right);
        machine.set_console(self.reset, self.select, self.pause);
    }

    /// Records `name`'s new state. Returns `true` when `name` mapped to a
    /// control (so the machine needs a re-apply), `false` otherwise.
    fn set_control(&mut self, name: &str, pressed: bool) -> bool {
        match name {
            "up" | "arrowup" => self.up = pressed,
            "down" | "arrowdown" => self.down = pressed,
            "left" | "arrowleft" => self.left = pressed,
            "right" | "arrowright" => self.right = pressed,
            "reset" => self.reset = pressed,
            "select" => self.select = pressed,
            "pause" => self.pause = pressed,
            _ => return false,
        }
        true
    }
}

pub(crate) fn apply_input_event(
    machine: &mut Atari7800,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    // Only P0 is exposed by the machine today, so the port is ignored.
    let (name, pressed) = match event {
        InputEvent::Button { name, pressed, .. } => (name.to_ascii_lowercase(), *pressed),
        InputEvent::Key { name, pressed } => (name.to_ascii_lowercase(), *pressed),
        _ => return,
    };
    if cache.set_control(&name, pressed) {
        cache.apply(machine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_directions_and_switches() {
        let mut cache = ControllerCache::default();
        assert!(cache.set_control("up", true));
        assert!(cache.up);
        assert!(cache.set_control("arrowleft", true));
        assert!(cache.left);
        assert!(cache.set_control("select", true));
        assert!(cache.select);
        assert!(cache.set_control("up", false));
        assert!(!cache.up);
        // Unknown names are ignored (no re-apply).
        assert!(!cache.set_control("fire", true));
        assert!(!cache.set_control("hyperspace", true));
    }
}
