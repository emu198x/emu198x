//! Atari ST input mapping.
//!
//! Everything reaches the machine through the IKBD, as on the real thing:
//!
//! - **Keys** are named and become the ST's scancodes, which follow the IBM
//!   PC's for the main block, with the ST's own Help, Undo and keypad
//!   bracket keys added. Names are lower case; letters and digits name
//!   themselves.
//! - **The mouse** is `mouse-1`: relative motion and the left and right
//!   buttons.
//! - **Joysticks** are button events: port 1 is the joystick port, port 2
//!   the mouse port (where two-player games put the second stick), with up,
//!   down, left, right and fire.

use crate::machine::AtariSt;
use common_atari_st::Stick;
use emu198x_shell::InputEvent;

/// The mouse's device name.
pub(crate) const MOUSE_DEVICE: &str = "mouse-1";

/// Apply one host input event to the machine.
pub(crate) fn apply_input_event(machine: &mut AtariSt, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some(code) = scancode(name) {
                machine.key(code, *pressed);
            }
        }
        InputEvent::PointerMotion { device, dx, dy } if device.as_ref() == MOUSE_DEVICE => {
            machine.mouse_move(*dx, *dy);
        }
        InputEvent::PointerButton {
            device,
            button,
            pressed,
        } if device.as_ref() == MOUSE_DEVICE => match button.as_ref() {
            "left" => machine.mouse_button(true, *pressed),
            "right" => machine.mouse_button(false, *pressed),
            _ => {}
        },
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            // Port 1 is the IKBD's joystick 1, port 2 its joystick 0.
            let stick = match port {
                1 => 1,
                2 => 0,
                _ => return,
            };
            let bit = match name.to_ascii_lowercase().as_str() {
                "up" => Stick::UP,
                "down" => Stick::DOWN,
                "left" => Stick::LEFT,
                "right" => Stick::RIGHT,
                "fire" | "fire1" | "button1" | "a" | "south" => Stick::FIRE,
                _ => return,
            };
            let Stick(mut state) = machine.ikbd().stick(stick);
            if *pressed {
                state |= bit;
            } else {
                state &= !bit;
            }
            machine.joystick(stick, Stick(state));
        }
        _ => {}
    }
}

/// The ST scancode a key name means.
#[must_use]
pub(crate) fn scancode(name: &str) -> Option<u8> {
    let name = name.to_ascii_lowercase();
    if let [c] = name.as_bytes() {
        let row =
            |keys: &[u8], first: u8| keys.iter().position(|k| k == c).map(|i| first + i as u8);
        return row(b"1234567890-=", 0x02)
            .or_else(|| row(b"qwertyuiop[]", 0x10))
            .or_else(|| row(b"asdfghjkl;'`", 0x1E))
            .or_else(|| row(b"\\zxcvbnm,./", 0x2B))
            .or(if *c == b'#' { Some(0x2B) } else { None });
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=10).contains(&n)
    {
        return Some(0x3A + n);
    }
    Some(match name.as_str() {
        "escape" | "esc" => 0x01,
        "minus" => 0x0C,
        "equals" | "equal" => 0x0D,
        "backspace" => 0x0E,
        "tab" => 0x0F,
        "leftbracket" | "bracketleft" => 0x1A,
        "rightbracket" | "bracketright" => 0x1B,
        "enter" | "return" => 0x1C,
        "control" | "ctrl" | "controlleft" | "controlright" => 0x1D,
        "semicolon" => 0x27,
        "quote" | "apostrophe" => 0x28,
        "backquote" | "grave" => 0x29,
        "shift" | "lshift" | "shiftleft" => 0x2A,
        "backslash" => 0x2B,
        "comma" => 0x33,
        "period" => 0x34,
        "slash" => 0x35,
        "rshift" | "shiftright" => 0x36,
        "alt" | "altleft" | "altright" => 0x38,
        "space" => 0x39,
        "capslock" => 0x3A,
        "clrhome" | "home" => 0x47,
        "up" | "arrowup" => 0x48,
        "kp-minus" | "numpadsubtract" => 0x4A,
        "left" | "arrowleft" => 0x4B,
        "right" | "arrowright" => 0x4D,
        "kp-plus" | "numpadadd" => 0x4E,
        "down" | "arrowdown" => 0x50,
        "insert" => 0x52,
        "delete" => 0x53,
        // The ISO key between left Shift and Z on European keyboards.
        "iso" | "intlbackslash" => 0x60,
        "undo" => 0x61,
        "help" => 0x62,
        "kp-(" | "numpadparenleft" => 0x63,
        "kp-)" | "numpadparenright" => 0x64,
        "kp-/" | "numpaddivide" => 0x65,
        "kp-*" | "numpadmultiply" => 0x66,
        "kp-7" | "numpad7" => 0x67,
        "kp-8" | "numpad8" => 0x68,
        "kp-9" | "numpad9" => 0x69,
        "kp-4" | "numpad4" => 0x6A,
        "kp-5" | "numpad5" => 0x6B,
        "kp-6" | "numpad6" => 0x6C,
        "kp-1" | "numpad1" => 0x6D,
        "kp-2" | "numpad2" => 0x6E,
        "kp-3" | "numpad3" => 0x6F,
        "kp-0" | "numpad0" => 0x70,
        "kp-." | "numpaddecimal" => 0x71,
        "kp-enter" | "numpadenter" => 0x72,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_block_follows_the_pc_layout() {
        assert_eq!(scancode("Escape"), Some(0x01));
        assert_eq!(scancode("1"), Some(0x02));
        assert_eq!(scancode("0"), Some(0x0B));
        assert_eq!(scancode("q"), Some(0x10));
        assert_eq!(scancode("A"), Some(0x1E));
        assert_eq!(scancode("`"), Some(0x29));
        assert_eq!(scancode("z"), Some(0x2C));
        assert_eq!(scancode("/"), Some(0x35));
        assert_eq!(scancode("space"), Some(0x39));
        assert_eq!(scancode("f1"), Some(0x3B));
        assert_eq!(scancode("f10"), Some(0x44));
        assert_eq!(scancode("f11"), None);
        assert_eq!(scancode("help"), Some(0x62));
        assert_eq!(scancode("kp-enter"), Some(0x72));
        assert_eq!(scancode("hyper"), None);
    }
}
