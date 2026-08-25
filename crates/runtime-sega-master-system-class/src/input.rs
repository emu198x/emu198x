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

use emu198x_shell::{InputEvent, aim_to_pixels};
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
            if name.eq_ignore_ascii_case("trigger") {
                machine.set_light_phaser_trigger(*port, *pressed);
                return;
            }
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
        // A light gun reports where it is aimed, not how far it moved, so it
        // arrives as an absolute `Aim` rather than pointer motion. The
        // coordinates are normalized across the framebuffer, which includes
        // the border; the machine maps them onto the picture and answers
        // `None` for anywhere the sensor cannot see.
        InputEvent::Aim { port, at } => {
            let (width, height) = (machine.framebuffer_width(), machine.framebuffer_height());
            let aim = at
                .map(|at| aim_to_pixels(at, width, height))
                .and_then(|(x, y)| machine.active_position(u32::from(x), u32::from(y)));
            machine.set_light_phaser_aim(*port, aim);
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

#[cfg(test)]
mod tests {
    use super::{ControllerCache, apply_input_event};
    use emu198x_shell::{InputEvent, aim_from_pixels};
    use machine_sega_master_system::{Sms, SmsVariant};

    fn machine() -> Sms {
        let mut cart = vec![0u8; 0x10000];
        cart[0x0008] = 0x18; // JR -2
        cart[0x0009] = 0xFE;
        Sms::new(cart, SmsVariant::SmsNtsc)
    }

    /// Aim at a framebuffer pixel, the way the harness does.
    fn aim_at(machine: &mut Sms, cache: &mut ControllerCache, x: i32, y: i32) {
        let (w, h) = (machine.framebuffer_width(), machine.framebuffer_height());
        let event = InputEvent::Aim {
            port: 1,
            at: aim_from_pixels(x, y, w, h),
        };
        apply_input_event(machine, cache, &event);
    }

    /// A host aims at somewhere on the screen it is showing, borders and all;
    /// the gun reads the picture. The corners of the picture have to survive
    /// that conversion exactly, because an off-by-one at the edge is an aim
    /// the game can never reach.
    #[test]
    fn an_aim_lands_on_the_picture_pixel_under_the_pointer() {
        // NTSC: a 280x240 window with 12 pixels of border each side and 25
        // above, around a 256x192 picture.
        for (fb_x, fb_y, expected) in [
            (12, 25, Some((0, 0))),
            (140, 121, Some((128, 96))),
            (267, 216, Some((255, 191))),
        ] {
            let mut machine = machine();
            let mut cache = ControllerCache::default();
            aim_at(&mut machine, &mut cache, fb_x, fb_y);
            assert_eq!(
                machine.light_phaser_aim(1),
                expected,
                "framebuffer ({fb_x}, {fb_y})"
            );
        }
    }

    /// The border is on the screen but not in the picture, so a gun aimed
    /// there has nothing to see.
    #[test]
    fn an_aim_into_the_border_finds_no_picture() {
        for (fb_x, fb_y) in [(11, 25), (12, 24), (268, 216), (267, 217)] {
            let mut machine = machine();
            let mut cache = ControllerCache::default();
            aim_at(&mut machine, &mut cache, fb_x, fb_y);
            assert_eq!(
                machine.light_phaser_aim(1),
                None,
                "framebuffer ({fb_x}, {fb_y}) is border"
            );
        }
    }

    /// Pointing off the window entirely is a gesture, not an absence — it is
    /// the reload in several light-gun games — and it has to reach the machine
    /// as such rather than being dropped or clamped to an edge.
    #[test]
    fn an_aim_off_the_window_unplugs_the_gun() {
        let mut machine = machine();
        let mut cache = ControllerCache::default();
        aim_at(&mut machine, &mut cache, 140, 121);
        assert!(machine.light_phaser_aim(1).is_some());

        apply_input_event(
            &mut machine,
            &mut cache,
            &InputEvent::Aim { port: 1, at: None },
        );
        assert_eq!(machine.light_phaser_aim(1), None);
    }

    /// The trigger arrives as an ordinary control on the port, because that is
    /// what it is — the same pin a pad uses for button 1.
    #[test]
    fn the_trigger_reaches_the_ports_tl_bit() {
        let mut machine = machine();
        let mut cache = ControllerCache::default();
        aim_at(&mut machine, &mut cache, 140, 121);

        let trigger = |pressed| InputEvent::Button {
            port: 1,
            name: "trigger".into(),
            pressed,
        };
        apply_input_event(&mut machine, &mut cache, &trigger(true));
        assert_eq!(
            machine.read_controller_port(1) & 0x10,
            0,
            "a held trigger pulls TL"
        );
        apply_input_event(&mut machine, &mut cache, &trigger(false));
        assert_eq!(machine.read_controller_port(1) & 0x10, 0x10);
    }

    /// Naming a control "trigger" must not have eaten the ordinary pad path.
    #[test]
    fn a_pad_button_still_works() {
        let mut machine = machine();
        let mut cache = ControllerCache::default();
        apply_input_event(
            &mut machine,
            &mut cache,
            &InputEvent::Button {
                port: 1,
                name: "button1".into(),
                pressed: true,
            },
        );
        assert_eq!(machine.read_controller_port(1) & 0x10, 0);
    }
}
