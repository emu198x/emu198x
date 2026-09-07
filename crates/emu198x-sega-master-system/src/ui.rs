//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Sega Master System / Game Gear's first native window, on the shared
//! `emu198x-ui` harness: wgpu video with `raw`/`lcd`/`crt` filters, framed VDP
//! audio, and keyboard/gamepad input. The SMS is a console — its pad is the
//! harness's console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]) —
//! plus the single Pause button, which the runtime takes as an
//! [`InputEvent::Key`] (`pause` on the SMS, `start` on the Game Gear), routed
//! through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo feature;
//! the shared launcher opens the window when no automation flag is given.

use std::path::PathBuf;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_sega_master_system::SmsRuntime;

use crate::app::{MasterSystem, Variant, default_battery_save_path};

const DEFAULT_SCALE: u32 = 3;

/// Player-1 control pad: directions plus the two face buttons. `south`/`east`
/// are the names `runtime-sega-master-system`'s `controller_bit` maps to the
/// SMS pad's button 1 / button 2. A real gamepad reaches these through the same
/// map; the keyboard does via [`UiSystem::map_key`].
const SMS_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "south")),
    (HostControl::East, ButtonTarget::new(1, "east")),
]);

/// The Sega Master System as a [`UiSystem`] for the shared harness.
/// The variant is fixed at construction; a hard reset rebuilds the machine from
/// the cartridge the runtime already holds.
pub struct SmsSystem {
    variant: Variant,
    battery_save_path: PathBuf,
}

impl UiApp for MasterSystem {
    type System = SmsSystem;

    /// The save path follows the cartridge; without one the runtime build
    /// fails before the window opens, so the placeholder is never written.
    fn ui_system(&self) -> SmsSystem {
        SmsSystem {
            variant: self.variant,
            battery_save_path: self
                .cart
                .as_deref()
                .map(default_battery_save_path)
                .unwrap_or_default(),
        }
    }
}

impl UiSystem for SmsSystem {
    /// The Light Phaser lives in controller port 1, where every light-gun
    /// title expects it. Pointing the mouse aims it; the left button is the
    /// trigger, mapped through the ordinary button path.
    fn aim_port(&self) -> Option<u8> {
        Some(1)
    }

    type Runtime = SmsRuntime;

    fn window_title(&self) -> String {
        "Emu198x Sega Master System".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The SMS drove a 4:3 TV.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((280, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.variant.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.variant.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SMS_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyZ => HostControl::South,
            KeyCode::KeyX => HostControl::East,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // The single console button. The runtime takes it as a named key
        // event.
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(&["pause"]),
            _ => None,
        }
    }

    fn on_exit(&mut self, runtime: &mut Self::Runtime) -> Result<(), String> {
        let Some(image) = runtime.cartridge_save_image() else {
            return Ok(());
        };
        std::fs::write(&self.battery_save_path, image).map_err(|err| {
            format!(
                "failed to write battery save {}: {err}",
                self.battery_save_path.display()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_maps_and_console_button() {
        let sms = SmsSystem {
            variant: Variant::SmsNtsc,
            battery_save_path: PathBuf::from("game.sav"),
        };
        assert_eq!(sms.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sms.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sms.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(sms.map_keys(KeyCode::Enter), Some(&["pause"][..]));
    }
}
