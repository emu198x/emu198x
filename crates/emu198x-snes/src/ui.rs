//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Super NES's native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, 48 kHz stereo audio, and
//! keyboard/gamepad input. The pad's d-pad, face buttons, Start and Select
//! go through the harness's console path ([`UiSystem::map_key`] +
//! [`UiSystem::button_map`]); a host has no controls for L and R, so the
//! keyboard sends those as named [`InputEvent::Key`] events through
//! [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo feature; the
//! shared launcher opens the window when no automation flag is given.
//!
//! [`InputEvent::Key`]: emu198x_shell::InputEvent::Key

use emu198x_shell::FamilyRuntime;
use emu198x_shell::{FirmwareOverrides, MachineError, build_replacement};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_nintendo_snes::{Model, SnesRuntime};

use crate::app::Snes;

const DEFAULT_SCALE: u32 = 2;

/// Player-1 pad. The names are the ones `runtime-nintendo-snes`'s input
/// mapping takes; the face buttons sit in the same diamond as a modern
/// pad's, so B is south and A east.
const SNES_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "b")),
    (HostControl::East, ButtonTarget::new(1, "a")),
    (HostControl::West, ButtonTarget::new(1, "y")),
    (HostControl::North, ButtonTarget::new(1, "x")),
    (HostControl::Start, ButtonTarget::new(1, "start")),
    (HostControl::Select, ButtonTarget::new(1, "select")),
]);

/// The Super NES as a [`UiSystem`] for the shared harness. Switching
/// models cold-boots with the in-memory cartridge.
pub struct SnesSystem {
    model: Model,
}

impl UiApp for Snes {
    type System = SnesSystem;

    fn ui_system(&self) -> SnesSystem {
        SnesSystem {
            model: self.model(),
        }
    }
}

impl UiSystem for SnesSystem {
    type Runtime = SnesRuntime;

    fn window_title(&self) -> String {
        "Emu198x Super NES".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    /// 512 wide, so hi-res modes keep every dot; 480 high when interlaced.
    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        let interlaced = runtime.machine().is_some_and(|m| m.ppu.interlaced());
        (512, if interlaced { 480 } else { 240 })
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    /// A frame is 262 or 312 lines of 1364 master clocks, so the NTSC
    /// models run at 60.10 Hz and the PAL one at 50.01 Hz.
    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        let model = runtime.model();
        Duration::from_secs_f64(model.frame_ticks() as f64 / model.master_hz() as f64)
    }

    fn variants(&self) -> Vec<VariantInfo> {
        Model::ALL
            .iter()
            .map(|model| VariantInfo::new(model.variant_id(), model.display_name()))
            .collect()
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(self.model.variant_id()))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        id: &str,
    ) -> Result<(), MachineError> {
        let model = Model::from_variant_id(id).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown nintendo-snes variant",
        })?;
        *runtime =
            build_replacement(runtime, model, &FirmwareOverrides::none()).map_err(|err| {
                MachineError::Host {
                    reason: err.to_string(),
                }
            })?;
        self.model = model;
        Ok(())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SNES_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyZ => HostControl::South,
            KeyCode::KeyX => HostControl::East,
            KeyCode::KeyA => HostControl::West,
            KeyCode::KeyS => HostControl::North,
            KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
            KeyCode::ShiftRight => HostControl::Select,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // The shoulder buttons, beyond the host's controls. The runtime
        // takes them as named key events on port 1.
        match code {
            KeyCode::KeyQ => Some(&["l"]),
            KeyCode::KeyW => Some(&["r"]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::MachineCore;

    /// `BRA *` at $8000, as a 32 KB headerless LoROM.
    fn cart() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[..2].copy_from_slice(&[0x80, 0xFE]);
        rom[0x7FFC..0x7FFE].copy_from_slice(&0x8000u16.to_le_bytes());
        rom
    }

    #[test]
    fn menu_switch_keeps_the_cartridge_and_tracks_live_pacing() {
        let mut system = SnesSystem {
            model: Model::SnesNtsc,
        };
        assert_eq!(
            system
                .variants()
                .iter()
                .map(|v| v.id.as_ref())
                .collect::<Vec<_>>(),
            Model::VARIANT_IDS
        );
        let mut runtime = SnesRuntime::new(Model::SnesNtsc, cart());
        for model in Model::ALL {
            system
                .switch_variant(&mut runtime, model.variant_id())
                .expect("switch");
            assert_eq!(
                system.current_variant().as_deref(),
                Some(model.variant_id())
            );
            assert_eq!(system.frame_ticks(&runtime), model.frame_ticks());
            let hz = 1.0 / system.frame_duration(&runtime).as_secs_f64();
            let expected = if model.region() == emu198x_shell::Region::Pal {
                50.01
            } else {
                60.10
            };
            assert!((hz - expected).abs() < 0.01, "{model:?} at {hz} Hz");
            let machine = runtime.machine().expect("machine");
            assert_eq!(machine.cart.rom(), cart());
            assert_eq!(system.framebuffer_size(&runtime), (512, 240));
        }
        let before = runtime.snapshot().expect("snapshot");
        assert!(system.switch_variant(&mut runtime, "genesis").is_err());
        assert_eq!(runtime.snapshot().expect("snapshot"), before);
    }

    #[test]
    fn pad_maps_and_shoulder_keys() {
        let system = SnesSystem {
            model: Model::SnesNtsc,
        };
        assert_eq!(system.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(system.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(system.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(system.map_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(
            system.map_key(KeyCode::ShiftRight),
            Some(HostControl::Select)
        );
        assert_eq!(system.map_keys(KeyCode::KeyQ), Some(&["l"][..]));
        assert_eq!(system.map_keys(KeyCode::KeyW), Some(&["r"][..]));
    }
}
