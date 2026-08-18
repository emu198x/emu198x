//! SMS / Game Gear controller input mapping.
//!
//! SMS controllers are 8-bit active-low bytes accessed through I/O ports
//! `$DC` (port 1) / `$DD` (port 2). Layout (active-low):
//!   bit 0 = up, 1 = down, 2 = left, 3 = right,
//!   bit 4 = button 1, 5 = button 2.
//!
//! `Sms::set_port_dc/dd` replaces the whole byte each time, so the
//! per-instance cache lives on `SmsRuntime` and is folded onto the
//! machine via `apply_input_event`.

use emu198x_shell::InputEvent;
use machine_sega_master_system::Sms;

/// Per-runtime cache: active-low controller bytes (0xFF = neutral).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ControllerCache {
    pub(crate) port_dc: u8,
    pub(crate) port_dd: u8,
    pub(crate) gg_start: u8,
}

impl Default for ControllerCache {
    fn default() -> Self {
        Self {
            port_dc: 0xFF,
            port_dd: 0xFF,
            gg_start: 0xFF,
        }
    }
}

/// Apply one host input event, updating the cache and pushing the
/// resulting bytes back into the machine.
pub(crate) fn apply_input_event(
    machine: &mut Sms,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            if let Some(bit) = controller_bit(name.as_ref()) {
                match *port {
                    1 => {
                        cache.port_dc = toggle(cache.port_dc, bit, *pressed);
                        machine.set_port_dc(cache.port_dc);
                    }
                    2 => {
                        cache.port_dd = toggle(cache.port_dd, bit, *pressed);
                        machine.set_port_dd(cache.port_dd);
                    }
                    _ => {}
                }
            }
        }
        InputEvent::Key { name, pressed } => match name.to_ascii_lowercase().as_str() {
            "pause" => machine.set_pause_pressed(*pressed),
            "start" => {
                cache.gg_start = if *pressed { 0x7F } else { 0xFF };
                machine.set_gg_start(cache.gg_start);
            }
            other => {
                if let Some(bit) = controller_bit(other) {
                    cache.port_dc = toggle(cache.port_dc, bit, *pressed);
                    machine.set_port_dc(cache.port_dc);
                }
            }
        },
        _ => {}
    }
}

fn controller_bit(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => 0,
        "down" | "arrowdown" => 1,
        "left" | "arrowleft" => 2,
        "right" | "arrowright" => 3,
        "button1" | "fire" | "fire1" | "south" | "cross" => 4,
        "button2" | "fire2" | "east" | "circle" => 5,
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
