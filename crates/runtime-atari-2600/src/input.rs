//! Atari 2600 joystick / console-switch input mapping.
//!
//! The 2600 uses two 6532-driven joystick ports plus a separate "console
//! switch" byte (game select, reset, B&W / colour). The host-side cache
//! mirrors both bytes so individual key events can be combined into the
//! single byte machine API.
//!
//! Layout (active-low joystick byte; routes via the RIOT):
//!   bit 0 = up (P0), 1 = down (P0), 2 = left (P0), 3 = right (P0),
//!   bit 4 = up (P1), 5 = down (P1), 6 = left (P1), 7 = right (P1).
//! Fire buttons live elsewhere (INPT4 / INPT5 on the TIA) and use the
//! `fire` / `fire2` host names.

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
            }
            // Fire buttons are not on the joystick byte — TIA inputs.
            // Defer wiring until machine-atari-2600 exposes set_inpt4/5.
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
            }
        }
        _ => {}
    }
}

fn joystick_bit(name: &str, port: u8) -> Option<u8> {
    let base = if port == 2 { 4 } else { 0 };
    Some(match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => base,
        "down" | "arrowdown" => base + 1,
        "left" | "arrowleft" => base + 2,
        "right" | "arrowright" => base + 3,
        _ => return None,
    })
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
