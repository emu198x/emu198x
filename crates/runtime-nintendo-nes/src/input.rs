//! NES controller input mapping.
//!
//! The NES has two physical controller ports — controller 1 is read
//! through `$4016` and controller 2 through `$4017`. Both share an
//! 8-bit shift register protocol latched by the strobe bit of
//! `$4016` writes.
//!
//! Runtime-side port convention (mirrors Spectrum / C64 Seam 2):
//! - `InputEvent::Button { port: 1, … }` → controller 1.
//! - `InputEvent::Button { port: 2, … }` → controller 2.
//! - `InputEvent::Key { … }` → controller 1 (keyboard-as-pad). A
//!   future `InputEvent::Key { port: 2, … }` extension would land
//!   here once we add port-specific keyboard mapping; for now plain
//!   `Key` events default to port 1.
//!
//! Button names supported (case-insensitive):
//!     `a`, `b`, `select`, `start`, `up`, `down`, `left`, `right`.
//!
//! Gamepad SDK alias mapping (so native code is neutral):
//!     `south` / `cross` / `button1` → `a`  (NES A — primary/jump)
//!     `east`  / `circle` / `button2` → `b`  (NES B — secondary/run)
//!     `west`  / `square` / `button3` → `b`  (extra B for 4-button pads)
//!     `north` / `triangle`/ `button4` → `a`  (extra A for 4-button pads)
//! Matches `emu198x-nes`'s `ButtonTarget` table — South-on-gamepad =
//! A-on-NES is the canonical "primary action" mapping.

use emu198x_shell::InputEvent;
use machine_nintendo_nes::Nes;

/// Apply one host input event to the machine. `Button` events route
/// by port (1 = controller 1, 2 = controller 2); other ports are
/// dropped silently. `Key` events default to controller 1 — the
/// canonical keyboard-as-pad routing.
pub(crate) fn apply_input_event(machine: &mut Nes, event: &InputEvent) {
    match event {
        InputEvent::Button {
            port,
            name,
            pressed,
        } => match *port {
            1 => apply_named_button(machine, 1, name.as_ref(), *pressed),
            2 => apply_named_button(machine, 2, name.as_ref(), *pressed),
            _ => {}
        },
        InputEvent::Key { name, pressed } => {
            apply_named_button(machine, 1, name.as_ref(), *pressed);
        }
        _ => {}
    }
}

fn apply_named_button(machine: &mut Nes, port: u8, name: &str, pressed: bool) {
    let Some(bit) = button_bit(name) else {
        return;
    };
    let mask = 1u8 << bit;
    let current = if port == 2 {
        machine.controller2_state
    } else {
        machine.controller1_state
    };
    let next = if pressed {
        current | mask
    } else {
        current & !mask
    };
    if port == 2 {
        machine.set_controller2(next);
    } else {
        machine.set_controller1(next);
    }
}

/// Look up the NES controller shift-register bit for a host-level
/// button or key name. Returns `None` for names that don't map; the
/// caller drops those events silently.
///
/// Gamepad-SDK aliases (`south` / `cross` / `button1` etc.) route to
/// the canonical NES `a` / `b` so host code stays neutral.
fn button_bit(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        // NES A — primary / jump. Gamepad south (cross / Xbox A) is
        // the canonical primary-action mapping.
        "a" | "south" | "cross" | "button1" => 0,
        // NES B — secondary / run. Gamepad east (circle / Xbox B).
        "b" | "east" | "circle" | "button2" => 1,
        "select" => 2,
        "start" => 3,
        "up" => 4,
        "down" => 5,
        "left" => 6,
        "right" => 7,
        // Four-button gamepads have north/west extras; mirror them
        // onto the NES A/B pair so users get fire on any face button.
        "north" | "triangle" | "button4" => 0,
        "west" | "square" | "button3" => 1,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use format_nintendo_nes_ines::{Mirroring, parse_ines};
    use std::borrow::Cow;

    fn make_nes() -> Nes {
        // Build a minimal NROM cartridge (16 KiB PRG + no CHR-ROM).
        let mut rom = vec![0u8; 16 + 16384];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = 1; // 16 KiB PRG
        rom[5] = 0; // CHR-RAM
        rom[6] = match Mirroring::Horizontal {
            Mirroring::Vertical => 1,
            _ => 0,
        };
        // Reset vector points at $8000 (NOP-loop irrelevant for input).
        rom[16 + 0x3FFC] = 0x00;
        rom[16 + 0x3FFD] = 0x80;
        let parsed = parse_ines(&rom).expect("test cart");
        Nes::new(parsed.mapper)
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: Cow::Owned(name.to_string()),
            pressed,
        }
    }

    #[test]
    fn port_1_button_lands_on_controller_1() {
        let mut nes = make_nes();
        apply_input_event(&mut nes, &button(1, "a", true));
        assert_eq!(nes.controller1_state & 1, 1);
        assert_eq!(nes.controller2_state, 0);
    }

    #[test]
    fn port_2_button_lands_on_controller_2() {
        let mut nes = make_nes();
        apply_input_event(&mut nes, &button(2, "start", true));
        assert_eq!(nes.controller2_state & 0b0000_1000, 0b0000_1000);
        assert_eq!(nes.controller1_state, 0);
    }

    #[test]
    fn port_3_button_is_dropped() {
        let mut nes = make_nes();
        apply_input_event(&mut nes, &button(3, "a", true));
        assert_eq!(nes.controller1_state, 0);
        assert_eq!(nes.controller2_state, 0);
    }

    #[test]
    fn gamepad_alias_south_maps_to_a_button() {
        let mut nes = make_nes();
        apply_input_event(&mut nes, &button(1, "south", true));
        assert_eq!(nes.controller1_state & 0b0000_0001, 0b0000_0001, "south → a");
    }

    #[test]
    fn gamepad_alias_east_maps_to_b_button() {
        let mut nes = make_nes();
        apply_input_event(&mut nes, &button(1, "east", true));
        assert_eq!(
            nes.controller1_state & 0b0000_0010,
            0b0000_0010,
            "east → b"
        );
    }

    #[test]
    fn release_clears_the_bit() {
        let mut nes = make_nes();
        apply_input_event(&mut nes, &button(1, "up", true));
        assert_eq!(nes.controller1_state & 0b0001_0000, 0b0001_0000);
        apply_input_event(&mut nes, &button(1, "up", false));
        assert_eq!(nes.controller1_state & 0b0001_0000, 0);
    }

    #[test]
    fn key_event_defaults_to_controller_1() {
        let mut nes = make_nes();
        apply_input_event(
            &mut nes,
            &InputEvent::Key {
                name: Cow::Owned("start".to_string()),
                pressed: true,
            },
        );
        assert_eq!(nes.controller1_state & 0b0000_1000, 0b0000_1000);
        assert_eq!(nes.controller2_state, 0);
    }
}
