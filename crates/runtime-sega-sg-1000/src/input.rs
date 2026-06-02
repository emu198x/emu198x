//! SG-1000 / SC-3000 controller input mapping.

use emu198x_shell::InputEvent;
use machine_sega_sg_1000::{ControllerState, Sg1000};

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
