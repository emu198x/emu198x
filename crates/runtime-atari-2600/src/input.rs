//! Atari 2600 joystick / console-switch input mapping.
//!
//! The 2600 uses two 6532-driven joystick ports plus a separate "console
//! switch" byte (game select, reset, B&W / colour). The host-side cache
//! mirrors both bytes so individual key events can be combined into the
//! single byte machine API.
//!
//! Layout (active-low SWCHA byte; routes via the RIOT). Player 1 (the left
//! jack) sits in the HIGH nibble, player 2 (the right jack) in the LOW nibble —
//! per MAME's `a2600.cpp` `switch_A_r` (`joyport1 << 4`, `joyport2 & 0x0f`):
//!   bit 4 = up (P1/port 1), 5 = down, 6 = left, 7 = right,
//!   bit 0 = up (P2/port 2), 1 = down, 2 = left, 3 = right.
//! Fire buttons live elsewhere (INPT4 / INPT5 on the TIA), not on the SWCHA
//! byte: a `fire` button event routes to `Atari2600::set_fire` for its port
//! (port 1 → INPT4, port 2 → INPT5).
//!
//! Paddles arrive as [`InputEvent::Axis`] and drive the TIA's INPT0-3
//! capacitor-charge inputs: the left jack (port 1) carries paddles INPT0/INPT1,
//! the right jack (port 2) INPT2/INPT3. The signed host axis scales to the
//! 8-bit pot position the TIA times against (0 charges fastest).

use emu198x_shell::InputEvent;
use machine_atari_2600::Atari2600;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ControllerCache {
    /// Active-low joystick byte (P0 in low nibble, P1 in high nibble).
    pub(crate) joystick: u8,
    /// Active-low console-switch byte (reset / select / colour / etc.).
    pub(crate) switches: u8,
}

impl Default for ControllerCache {
    fn default() -> Self {
        Self {
            joystick: 0xFF,
            switches: 0xFF,
        }
    }
}

pub(crate) fn apply_input_event(
    machine: &mut Atari2600,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            if let Some(bit) = joystick_bit(name.as_ref(), *port) {
                cache.joystick = toggle(cache.joystick, bit, *pressed);
                machine.set_joystick_input(cache.joystick);
            } else if let Some(bit) = switch_bit(name.as_ref()) {
                cache.switches = toggle(cache.switches, bit, *pressed);
                machine.set_switch_input(cache.switches);
            } else if is_fire(name.as_ref()) {
                // Fire is a TIA input (INPT4/INPT5), not part of the SWCHA
                // joystick byte; route it straight to the addressed port.
                machine.set_fire(*port, *pressed);
            }
        }
        InputEvent::Key { name, pressed } => {
            // Default keyboard-as-pad → player 1.
            let lower = name.to_ascii_lowercase();
            if let Some(bit) = joystick_bit(&lower, 1) {
                cache.joystick = toggle(cache.joystick, bit, *pressed);
                machine.set_joystick_input(cache.joystick);
            } else if let Some(bit) = switch_bit(&lower) {
                cache.switches = toggle(cache.switches, bit, *pressed);
                machine.set_switch_input(cache.switches);
            } else if is_fire(&lower) {
                machine.set_fire(1, *pressed);
            }
        }
        InputEvent::Axis { port, name, value } => {
            if let Some(index) = paddle_index(name.as_ref(), *port) {
                machine.set_paddle(index, axis_to_pot8(*value));
            }
        }
        _ => {}
    }
}

/// Map a paddle axis to its INPT line (0-3). The left jack (port 1) carries
/// INPT0/INPT1, the right jack (port 2) INPT2/INPT3; `x` selects the first
/// paddle of the pair, `y` the second.
fn paddle_index(name: &str, port: u8) -> Option<u8> {
    let base = if port == 2 { 2 } else { 0 };
    Some(match name.to_ascii_lowercase().as_str() {
        "x" | "horizontal" | "pot0" | "paddle0" => base,
        "y" | "vertical" | "pot1" | "paddle1" => base + 1,
        _ => return None,
    })
}

/// Scale a normalized signed axis (`i16::MIN..=i16::MAX`) onto the 8-bit paddle
/// pot position (`0..=255`); `0` lands near centre (`128`).
fn axis_to_pot8(value: i16) -> u8 {
    let shifted = i32::from(value) - i32::from(i16::MIN); // 0..=65535
    u8::try_from((shifted * 255) / 65535).unwrap_or(255)
}

fn joystick_bit(name: &str, port: u8) -> Option<u8> {
    // SWCHA carries player 1 (port 1, left jack) in the high nibble and
    // player 2 (port 2, right jack) in the low nibble (MAME a2600 switch_A_r).
    let base = if port == 2 { 0 } else { 4 };
    Some(match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => base,
        "down" | "arrowdown" => base + 1,
        "left" | "arrowleft" => base + 2,
        "right" | "arrowright" => base + 3,
        _ => return None,
    })
}

/// Whether `name` is a fire-button name. The 2600 joystick has a single
/// button per port (read on INPT4/INPT5), so all the common aliases map to it.
fn is_fire(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "fire" | "fire1" | "trigger" | "button"
    )
}

fn switch_bit(name: &str) -> Option<u8> {
    // RIOT switch byte: bit 0 = reset, 1 = select, 2 = colour/bw,
    // 3 = p0 difficulty, 4 = p1 difficulty (active-low).
    Some(match name.to_ascii_lowercase().as_str() {
        "reset" => 0,
        "select" => 1,
        "colour" | "color" | "bw" => 2,
        "diff1" | "p0_difficulty" => 3,
        "diff2" | "p1_difficulty" => 4,
        _ => return None,
    })
}

fn toggle(current: u8, bit: u8, pressed: bool) -> u8 {
    if pressed {
        current & !(1u8 << bit)
    } else {
        current | (1u8 << bit)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControllerCache, InputEvent, apply_input_event, axis_to_pot8, is_fire, joystick_bit,
        paddle_index,
    };
    use machine_atari_2600::{Atari2600, Atari2600Region};
    use std::borrow::Cow;

    fn trap_machine() -> Atari2600 {
        // 4 KB cart whose reset vector jumps to a self-loop at $1000.
        let mut rom = vec![0xEA_u8; 4096];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0x10;
        rom[0x0FFC] = 0x00;
        rom[0x0FFD] = 0x10;
        Atari2600::new(rom, Atari2600Region::Ntsc).expect("init")
    }

    fn fire(port: u8, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: Cow::Borrowed("fire"),
            pressed,
        }
    }

    #[test]
    fn fire_button_events_reach_inpt4_and_inpt5() {
        let mut m = trap_machine();
        let mut cache = ControllerCache::default();

        // Port 1 fire → INPT4 ($0C); port 2 fire → INPT5 ($0D), active low.
        apply_input_event(&mut m, &mut cache, &fire(1, true));
        assert_eq!(m.tia().read(0x0C) & 0x80, 0, "p1 fire reaches INPT4");
        apply_input_event(&mut m, &mut cache, &fire(2, true));
        assert_eq!(m.tia().read(0x0D) & 0x80, 0, "p2 fire reaches INPT5");

        // Release restores the line.
        apply_input_event(&mut m, &mut cache, &fire(1, false));
        assert_eq!(m.tia().read(0x0C) & 0x80, 0x80, "release restores INPT4");
    }

    #[test]
    fn fire_aliases_are_recognised() {
        for n in ["fire", "Fire", "fire1", "trigger", "button"] {
            assert!(is_fire(n), "{n} is a fire name");
        }
        assert!(!is_fire("up"), "directions are not fire");
    }

    #[test]
    fn joystick_nibbles_follow_swcha_player_assignment() {
        // Player 1 (port 1, left jack) occupies the high nibble; player 2
        // (port 2) the low nibble. Within each nibble: up,down,left,right.
        // Matches MAME a2600 switch_A_r + vcs_ctrl/joystick.cpp.
        assert_eq!(joystick_bit("up", 1), Some(4));
        assert_eq!(joystick_bit("down", 1), Some(5));
        assert_eq!(joystick_bit("left", 1), Some(6));
        assert_eq!(joystick_bit("right", 1), Some(7));
        assert_eq!(joystick_bit("up", 2), Some(0));
        assert_eq!(joystick_bit("down", 2), Some(1));
        assert_eq!(joystick_bit("left", 2), Some(2));
        assert_eq!(joystick_bit("right", 2), Some(3));
        assert_eq!(joystick_bit("throttle", 1), None);
    }

    #[test]
    fn paddles_route_to_the_right_inpt_lines() {
        // Left jack (port 1) → INPT0/1; right jack (port 2) → INPT2/3.
        assert_eq!(paddle_index("x", 1), Some(0));
        assert_eq!(paddle_index("y", 1), Some(1));
        assert_eq!(paddle_index("x", 2), Some(2));
        assert_eq!(paddle_index("vertical", 2), Some(3));
        assert_eq!(paddle_index("throttle", 1), None);
    }

    #[test]
    fn axis_scales_to_the_8bit_pot_range() {
        assert_eq!(axis_to_pot8(i16::MIN), 0);
        assert_eq!(axis_to_pot8(i16::MAX), 255);
        assert!((120..=136).contains(&axis_to_pot8(0)));
    }
}
