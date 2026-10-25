//! Neo Geo controller input mapping.
//!
//! Each controller's buttons live on the machine as a [`Buttons`] set, so
//! an event changes one button and leaves the rest as they were.

use emu198x_shell::InputEvent;
use machine_snk_neo_geo::{Buttons, NeoGeo};

/// Apply one host input event to the machine's controllers. Button events
/// name a port, 1 or 2; key events go to port 1.
pub(crate) fn apply_input_event(machine: &mut NeoGeo, event: &InputEvent) {
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
    if let Some(button) = button(&mut machine.buttons[index], name) {
        *button = pressed;
    }
}

/// The controller button `name` means. The four buttons sit in a row,
/// A B C D, so a host pad's south, east, west and north buttons take them.
fn button<'a>(buttons: &'a mut Buttons, name: &str) -> Option<&'a mut bool> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => &mut buttons.up,
        "down" | "arrowdown" => &mut buttons.down,
        "left" | "arrowleft" => &mut buttons.left,
        "right" | "arrowright" => &mut buttons.right,
        "a" | "south" | "cross" | "button1" | "fire" | "fire1" => &mut buttons.a,
        "b" | "east" | "circle" | "button2" | "fire2" => &mut buttons.b,
        "c" | "west" | "square" | "button3" => &mut buttons.c,
        "d" | "north" | "triangle" | "button4" => &mut buttons.d,
        "start" | "enter" => &mut buttons.start,
        "select" => &mut buttons.select,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::apply_input_event;
    use emu198x_shell::InputEvent;
    use machine_snk_neo_geo::{Buttons, Cartridge, NeoGeo};

    fn machine() -> NeoGeo {
        NeoGeo::new(Cartridge::default(), vec![0; 0x2_0000], Vec::new())
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
        let mut neo = machine();
        for name in ["up", "south", "east", "west", "north", "start", "select"] {
            apply_input_event(&mut neo, &button(2, name, true));
        }
        assert_eq!(
            neo.buttons[1],
            Buttons {
                up: true,
                a: true,
                b: true,
                c: true,
                d: true,
                start: true,
                select: true,
                ..Buttons::default()
            }
        );
        assert_eq!(neo.buttons[0], Buttons::default());
    }

    #[test]
    fn keys_go_to_port_1_and_unknown_names_and_ports_are_ignored() {
        let mut neo = machine();
        apply_input_event(
            &mut neo,
            &InputEvent::Key {
                name: "ArrowDown".to_owned().into(),
                pressed: true,
            },
        );
        apply_input_event(&mut neo, &button(1, "turbo", true));
        apply_input_event(&mut neo, &button(3, "a", true));
        apply_input_event(&mut neo, &button(0, "a", true));
        apply_input_event(&mut neo, &button(1, "a", true));
        apply_input_event(&mut neo, &button(1, "a", false));
        assert_eq!(
            neo.buttons[0],
            Buttons {
                down: true,
                ..Buttons::default()
            }
        );
        assert_eq!(neo.buttons[1], Buttons::default());
    }
}
