//! Mega Drive pad input mapping.
//!
//! Each pad's buttons live on the machine's port as a [`Buttons`] set, so
//! an event changes one button and leaves the rest as they were. There is
//! no cache to keep, unlike the Master System's whole-byte ports: the
//! machine holds the state, and snapshots carry it.
//!
//! The four extra buttons of the six-button pad (X, Y, Z and Mode) reach
//! the port whatever is plugged in; a three-button pad never reports them.

use emu198x_shell::InputEvent;
use machine_sega_mega_drive::{Buttons, MegaDrive};

/// Apply one host input event to the machine's pads. Button events name a
/// port, 1 or 2; key events go to port 1.
pub(crate) fn apply_input_event(machine: &mut MegaDrive, event: &InputEvent) {
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
    let mut buttons = machine.ports[index].buttons;
    if let Some(button) = button(&mut buttons, name) {
        *button = pressed;
        machine.set_buttons(index, buttons);
    }
}

/// The pad button `name` means. The face buttons sit in a row, A B C, so
/// a host pad's west, south and east buttons take them, and its north and
/// select buttons take X and Mode.
fn button<'a>(buttons: &'a mut Buttons, name: &str) -> Option<&'a mut bool> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => &mut buttons.up,
        "down" | "arrowdown" => &mut buttons.down,
        "left" | "arrowleft" => &mut buttons.left,
        "right" | "arrowright" => &mut buttons.right,
        "a" | "west" | "square" | "button1" | "fire" | "fire1" => &mut buttons.a,
        "b" | "south" | "cross" | "button2" | "fire2" => &mut buttons.b,
        "c" | "east" | "circle" | "button3" => &mut buttons.c,
        "start" | "enter" => &mut buttons.start,
        "x" | "north" | "triangle" => &mut buttons.x,
        "y" => &mut buttons.y,
        "z" => &mut buttons.z,
        "mode" | "select" => &mut buttons.mode,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::apply_input_event;
    use emu198x_shell::InputEvent;
    use machine_sega_mega_drive::{Buttons, MegaDrive, Region};

    fn machine() -> MegaDrive {
        MegaDrive::new(vec![0; 0x400], Region::Americas)
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
        let mut md = machine();
        for name in [
            "up", "west", "south", "east", "start", "x", "y", "z", "mode",
        ] {
            apply_input_event(&mut md, &button(2, name, true));
        }
        assert_eq!(
            md.ports[1].buttons,
            Buttons {
                up: true,
                a: true,
                b: true,
                c: true,
                start: true,
                x: true,
                y: true,
                z: true,
                mode: true,
                ..Buttons::default()
            }
        );
        assert_eq!(md.ports[0].buttons, Buttons::default());
    }

    #[test]
    fn a_release_clears_only_its_own_button() {
        let mut md = machine();
        apply_input_event(&mut md, &button(1, "a", true));
        apply_input_event(&mut md, &button(1, "left", true));
        apply_input_event(&mut md, &button(1, "a", false));
        assert!(md.ports[0].buttons.left);
        assert!(!md.ports[0].buttons.a);
    }

    #[test]
    fn keys_go_to_port_1_and_unknown_names_and_ports_are_ignored() {
        let mut md = machine();
        apply_input_event(
            &mut md,
            &InputEvent::Key {
                name: "ArrowDown".to_owned().into(),
                pressed: true,
            },
        );
        apply_input_event(&mut md, &button(1, "turbo", true));
        apply_input_event(&mut md, &button(3, "a", true));
        apply_input_event(&mut md, &button(0, "a", true));
        assert_eq!(
            md.ports[0].buttons,
            Buttons {
                down: true,
                ..Buttons::default()
            }
        );
        assert_eq!(md.ports[1].buttons, Buttons::default());
    }
}
