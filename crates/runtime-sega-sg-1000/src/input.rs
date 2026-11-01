//! SG-1000 console controller input mapping.

use crate::Sg1000Machine as Sg1000;
use common_sega_sg_1000::ControllerState;
use emu198x_shell::InputEvent;

pub(crate) fn apply_input_event(machine: &mut Sg1000, event: &InputEvent) {
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
            if name.as_ref().eq_ignore_ascii_case("pause") {
                machine.set_pause_pressed(*pressed);
            } else {
                apply_button(machine.controller1_mut(), name.as_ref(), *pressed);
            }
        }
        _ => {}
    }
}

fn apply_button(ctrl: &mut ControllerState, name: &str, pressed: bool) {
    match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => ctrl.up = pressed,
        "down" | "arrowdown" => ctrl.down = pressed,
        "left" | "arrowleft" => ctrl.left = pressed,
        "right" | "arrowright" => ctrl.right = pressed,
        // Button 1 — primary action. Maps gamepad south.
        "button1" | "fire" | "fire1" | "south" | "cross" => {
            ctrl.button1 = pressed;
        }
        // Button 2 — secondary action. Maps gamepad east.
        "button2" | "fire2" | "east" | "circle" => {
            ctrl.button2 = pressed;
        }
        _ => {}
    }
}
