//! VIC-20 keyboard and control-port input mapping.
//!
//! The VIC-20 has a single DE-9 control port wired across both VIAs:
//! up/down/left/fire on VIA #1 PA2-PA5 and right on VIA #2 PB7 (the awkward
//! one), all active low. Joystick input arrives as [`InputEvent::Button`] and
//! drives [`Vic20::set_joystick`] through a host-side [`ControllerCache`] that
//! re-applies the whole switch state on each event. The arrow *keys* (host
//! `Key` events) stay mapped to the VIC-20's cursor keys — a separate input
//! path from the joystick directions.

use emu198x_shell::InputEvent;
use machine_commodore_vic_20::{Vic20, Vic20Key};

/// Host-side mirror of the single control-port joystick: four directions plus
/// the fire button, re-applied via [`Vic20::set_joystick`] on every event.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

impl ControllerCache {
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

    /// Record a control and, if it mapped, push the whole switch state to the
    /// machine's single control port.
    fn apply(&mut self, machine: &mut Vic20, name: &str, pressed: bool) {
        if self.set_control(name, pressed) {
            machine.set_joystick(self.up, self.down, self.left, self.right, self.fire);
        }
    }
}

pub(crate) fn apply_input_event(
    machine: &mut Vic20,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some(key) = key_from_name(name.as_ref()) {
                if *pressed {
                    machine.press_key(key);
                } else {
                    machine.release_key(key);
                }
            }
        }
        InputEvent::Button { name, pressed, .. } => {
            cache.apply(machine, &name.to_ascii_lowercase(), *pressed);
        }
        _ => {}
    }
}

#[must_use]
fn key_from_name(name: &str) -> Option<Vic20Key> {
    Some(match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => Vic20Key::Return,
        "space" | " " => Vic20Key::Space,
        "stop" | "runstop" | "run/stop" => Vic20Key::Stop,
        "delete" | "del" | "backspace" => Vic20Key::Delete,
        "home" | "clr" => Vic20Key::Home,
        "ctrl" | "control" => Vic20Key::Ctrl,
        "commodore" | "cbm" => Vic20Key::Commodore,
        "lshift" | "shift" => Vic20Key::ShiftLeft,
        "rshift" => Vic20Key::ShiftRight,
        "crsr-right" | "right" => Vic20Key::CursorRight,
        "crsr-down" | "down" => Vic20Key::CursorDown,
        "f1" => Vic20Key::F1,
        "f3" => Vic20Key::F3,
        "f5" => Vic20Key::F5,
        "f7" => Vic20Key::F7,
        "0" => Vic20Key::Num0,
        "1" => Vic20Key::Num1,
        "2" => Vic20Key::Num2,
        "3" => Vic20Key::Num3,
        "4" => Vic20Key::Num4,
        "5" => Vic20Key::Num5,
        "6" => Vic20Key::Num6,
        "7" => Vic20Key::Num7,
        "8" => Vic20Key::Num8,
        "9" => Vic20Key::Num9,
        "+" | "plus" => Vic20Key::Plus,
        "-" | "minus" => Vic20Key::Minus,
        "*" | "asterisk" => Vic20Key::Asterisk,
        "/" | "slash" => Vic20Key::Slash,
        "=" | "equal" => Vic20Key::Equal,
        ":" | "colon" => Vic20Key::Colon,
        ";" | "semicolon" => Vic20Key::Semicolon,
        "," | "comma" => Vic20Key::Comma,
        "." | "period" => Vic20Key::Period,
        "@" | "at" => Vic20Key::At,
        "pound" | "sterling" => Vic20Key::Pound,
        "arrowup" | "up" => Vic20Key::ArrowUp,
        "arrowleft" | "left" => Vic20Key::ArrowLeft,
        "a" => Vic20Key::A,
        "b" => Vic20Key::B,
        "c" => Vic20Key::C,
        "d" => Vic20Key::D,
        "e" => Vic20Key::E,
        "f" => Vic20Key::F,
        "g" => Vic20Key::G,
        "h" => Vic20Key::H,
        "i" => Vic20Key::I,
        "j" => Vic20Key::J,
        "k" => Vic20Key::K,
        "l" => Vic20Key::L,
        "m" => Vic20Key::M,
        "n" => Vic20Key::N,
        "o" => Vic20Key::O,
        "p" => Vic20Key::P,
        "q" => Vic20Key::Q,
        "r" => Vic20Key::R,
        "s" => Vic20Key::S,
        "t" => Vic20Key::T,
        "u" => Vic20Key::U,
        "v" => Vic20Key::V,
        "w" => Vic20Key::W,
        "x" => Vic20Key::X,
        "y" => Vic20Key::Y,
        "z" => Vic20Key::Z,
        _ => return None,
    })
}

/// Whether this machine's input layer can deliver `name`.
///
/// This is the same lookup [`apply_input_event`] performs before injecting a
/// keystroke, exposed so the shared keyboard can refuse a character the
/// machine cannot type instead of counting one it silently dropped (#1196).
pub(crate) fn knows_key_name(name: &str) -> bool {
    key_from_name(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use machine_commodore_vic_20::Vic20Model;
    use std::borrow::Cow;

    fn make_vic20() -> Vic20 {
        // ROM-free guard: a NOP-filled KERNAL is enough to tick the VIAs.
        Vic20::new(
            vec![0xEA; 0x2000],
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Ntsc,
            0,
        )
    }

    fn button(name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port: 1,
            name: Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    #[test]
    fn button_events_drive_the_control_port() {
        let mut sys = make_vic20();
        let mut cache = ControllerCache::default();

        // Up + fire land on VIA #1 PA2 / PA5 (active low, read at $9111).
        apply_input_event(&mut sys, &mut cache, &button("up", true));
        apply_input_event(&mut sys, &mut cache, &button("fire", true));
        let _ = sys.step_instruction();
        let pa = sys.peek(0x9111);
        assert_eq!(pa & (1 << 2), 0, "up → PA2 low");
        assert_eq!(pa & (1 << 5), 0, "fire → PA5 low");
        assert_eq!(pa & (1 << 3), 1 << 3, "down idle → PA3 high");

        // Right is the awkward VIA #2 PB7 line (read at $9120), not PA.
        apply_input_event(&mut sys, &mut cache, &button("right", true));
        let _ = sys.step_instruction();
        assert_eq!(sys.peek(0x9120) & (1 << 7), 0, "right → PB7 low");

        // Releasing up leaves fire + right held.
        apply_input_event(&mut sys, &mut cache, &button("up", false));
        let _ = sys.step_instruction();
        assert_eq!(
            sys.peek(0x9111) & (1 << 2),
            1 << 2,
            "up released → PA2 high"
        );
        assert_eq!(sys.peek(0x9111) & (1 << 5), 0, "fire still held → PA5 low");
    }

    #[test]
    fn arrow_keys_stay_on_the_keyboard_not_the_joystick() {
        // `Key` arrow names map to cursor keys, never the joystick — so a key
        // press must leave the control-port lines idle high.
        let mut sys = make_vic20();
        let mut cache = ControllerCache::default();
        apply_input_event(
            &mut sys,
            &mut cache,
            &InputEvent::Key {
                name: Cow::Borrowed("up"),
                pressed: true,
            },
        );
        let _ = sys.step_instruction();
        assert_eq!(sys.peek(0x9111) & 0x3C, 0x3C, "joystick PA lines untouched");
    }
}
