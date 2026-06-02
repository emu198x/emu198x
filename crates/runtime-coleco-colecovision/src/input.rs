//! ColecoVision controller input mapping.
//!
//! The Coleco has two controllers; each has an 8-direction joystick,
//! two fire buttons (left/right), and a 12-key numeric keypad
//! (0-9, *, #). Host `Button` events route by port (1/2); the name
//! determines which control. `Key` events with arrow / digit names
//! map to controller 1 as the keyboard-as-pad convention.

use emu198x_shell::InputEvent;
use machine_coleco_colecovision::{ColecoVision, CvController, KeypadKey};

/// Apply one host input event to the ColecoVision.
pub(crate) fn apply_input_event(machine: &mut ColecoVision, event: &InputEvent) {
    match event {
        InputEvent::Button {
            port,
            name,
            pressed,
        } => match *port {
            1 => apply_button(machine.controller1_mut(), name.as_ref(), *pressed),
            2 => apply_button(machine.controller2_mut(), name.as_ref(), *pressed),
            _ => {}
        },
        InputEvent::Key { name, pressed } => {
            apply_button(machine.controller1_mut(), name.as_ref(), *pressed);
        }
        _ => {}
    }
}

fn apply_button(ctrl: &mut CvController, name: &str, pressed: bool) {
    match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => ctrl.up = pressed,
        "down" | "arrowdown" => ctrl.down = pressed,
        "left" | "arrowleft" => ctrl.left = pressed,
        "right" | "arrowright" => ctrl.right = pressed,
        // Left fire button — primary action. Maps gamepad south.
        "left_button" | "fire" | "fire1" | "south" | "cross" | "button1" => {
            ctrl.left_button = pressed;
        }
        // Right fire button — secondary action. Maps gamepad east.
        "right_button" | "fire2" | "east" | "circle" | "button2" => {
            ctrl.right_button = pressed;
        }
        // Numeric keypad keys
        "0" => keypad(ctrl, KeypadKey::K0, pressed),
        "1" => keypad(ctrl, KeypadKey::K1, pressed),
        "2" => keypad(ctrl, KeypadKey::K2, pressed),
        "3" => keypad(ctrl, KeypadKey::K3, pressed),
        "4" => keypad(ctrl, KeypadKey::K4, pressed),
        "5" => keypad(ctrl, KeypadKey::K5, pressed),
        "6" => keypad(ctrl, KeypadKey::K6, pressed),
        "7" => keypad(ctrl, KeypadKey::K7, pressed),
        "8" => keypad(ctrl, KeypadKey::K8, pressed),
        "9" => keypad(ctrl, KeypadKey::K9, pressed),
        "*" | "star" => keypad(ctrl, KeypadKey::Star, pressed),
        "#" | "hash" | "pound" => keypad(ctrl, KeypadKey::Hash, pressed),
        _ => {}
    }
}

fn keypad(ctrl: &mut CvController, key: KeypadKey, pressed: bool) {
    if pressed {
        ctrl.keypad = Some(key);
    } else if ctrl.keypad == Some(key) {
        ctrl.keypad = None;
    }
}
