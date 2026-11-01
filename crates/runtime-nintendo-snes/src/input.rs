//! Super NES pad input mapping.
//!
//! Each pad's buttons live on the machine's port, so an event changes one
//! button and leaves the rest as they were, and snapshots carry them. The
//! face buttons sit in a diamond like a modern pad's: B south, A east,
//! Y west, X north.

use crate::SnesMachine as Snes;
use common_nintendo_snes::Buttons;
use emu198x_shell::InputEvent;

/// Apply one host input event to the machine's pads. Button events name a
/// port, 1 or 2; key events go to port 1.
pub(crate) fn apply_input_event(machine: &mut Snes, event: &InputEvent) {
    let (port, name, pressed) = match event {
        InputEvent::Button {
            port,
            name,
            pressed,
        } => (*port, name.as_ref(), *pressed),
        InputEvent::Key { name, pressed } => (1, name.as_ref(), *pressed),
        _ => return,
    };
    let Some(index) = usize::from(port).checked_sub(1).filter(|&i| i < 2) else {
        return;
    };
    let mut buttons = machine.pads[index].buttons;
    if let Some(button) = button(&mut buttons, name) {
        *button = pressed;
        machine.set_buttons(index, buttons);
    }
}

fn button<'a>(buttons: &'a mut Buttons, name: &str) -> Option<&'a mut bool> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => &mut buttons.up,
        "down" | "arrowdown" => &mut buttons.down,
        "left" | "arrowleft" => &mut buttons.left,
        "right" | "arrowright" => &mut buttons.right,
        "b" | "south" | "cross" | "button1" | "fire" | "fire1" => &mut buttons.b,
        "a" | "east" | "circle" | "button2" | "fire2" => &mut buttons.a,
        "y" | "west" | "square" => &mut buttons.y,
        "x" | "north" | "triangle" => &mut buttons.x,
        "l" | "leftshoulder" | "l1" => &mut buttons.l,
        "r" | "rightshoulder" | "r1" => &mut buttons.r,
        "start" | "enter" => &mut buttons.start,
        "select" | "back" => &mut buttons.select,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::apply_input_event;
    use crate::{Model, SnesMachine as Snes};
    use common_nintendo_snes::{Buttons, Cartridge, Map};
    use emu198x_shell::InputEvent;

    fn machine() -> Snes {
        Snes::new(
            Model::SnesNtsc,
            Cartridge::from_rom(vec![0; 0x8000], Map::LoRom, 0),
            None,
        )
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: name.to_owned().into(),
            pressed,
        }
    }

    #[test]
    fn each_name_reaches_its_button_on_its_port() {
        let mut snes = machine();
        for name in [
            "up", "south", "east", "west", "north", "l", "r", "start", "select",
        ] {
            apply_input_event(&mut snes, &button(2, name, true));
        }
        assert_eq!(
            snes.pads[1].buttons,
            Buttons {
                up: true,
                b: true,
                a: true,
                y: true,
                x: true,
                l: true,
                r: true,
                start: true,
                select: true,
                ..Buttons::default()
            }
        );
        assert_eq!(snes.pads[0].buttons, Buttons::default());
    }

    #[test]
    fn keys_go_to_port_1_and_unknown_names_and_ports_are_ignored() {
        let mut snes = machine();
        apply_input_event(
            &mut snes,
            &InputEvent::Key {
                name: "ArrowDown".to_owned().into(),
                pressed: true,
            },
        );
        apply_input_event(&mut snes, &button(1, "turbo", true));
        apply_input_event(&mut snes, &button(3, "a", true));
        apply_input_event(&mut snes, &button(0, "a", true));
        apply_input_event(&mut snes, &button(1, "down", false));
        assert_eq!(snes.pads[0].buttons, Buttons::default());
        assert_eq!(snes.pads[1].buttons, Buttons::default());
    }
}
