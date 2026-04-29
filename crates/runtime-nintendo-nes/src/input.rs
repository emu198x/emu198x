//! NES controller input mapping.
//!
//! Splits the controller-button lookup table out of `runtime.rs`. The
//! NES exposes one 8-bit shift register per controller; this module
//! maps host input names ("a", "b", "start", …) to bit positions in
//! that register and applies presses/releases to controller 1.

use emu198x_shell::InputEvent;
use machine_nintendo_nes::Nes;

/// Apply one host input event to the machine: keys and port-1 button
/// presses both land on controller 1 by name. Other event kinds (mouse
/// motion, port > 1, etc.) are ignored — the baseline NES has no second
/// controller wired up in this runtime.
pub(crate) fn apply_input_event(machine: &mut Nes, event: &InputEvent) {
    match event {
        InputEvent::Button {
            port,
            name,
            pressed,
        } if *port == 1 => {
            apply_named_button(machine, name.as_ref(), *pressed);
        }
        InputEvent::Key { name, pressed } => {
            apply_named_button(machine, name.as_ref(), *pressed);
        }
        _ => {}
    }
}

fn apply_named_button(machine: &mut Nes, name: &str, pressed: bool) {
    let Some(bit) = button_bit(name) else {
        return;
    };
    let mask = 1u8 << bit;
    let mut state = machine.controller1_state;
    if pressed {
        state |= mask;
    } else {
        state &= !mask;
    }
    machine.set_controller1(state);
}

/// Look up the controller-1 shift-register bit for a host-level button
/// or key name. Returns `None` for names that don't map to an NES
/// button; the caller silently drops those events.
fn button_bit(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        "a" => 0,
        "b" => 1,
        "select" => 2,
        "start" => 3,
        "up" => 4,
        "down" => 5,
        "left" => 6,
        "right" => 7,
        _ => return None,
    })
}

