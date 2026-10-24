//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Mega Drive's native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, 48 kHz stereo audio, and
//! keyboard/gamepad input. The pad's A, B, C, Start, X and Mode go through
//! the harness's console path ([`UiSystem::map_key`] +
//! [`UiSystem::button_map`]); a host has no controls left for Y and Z, so
//! the keyboard sends those as named [`InputEvent::Key`] events through
//! [`UiSystem::map_keys`], as it does Mode. Compiled only with the `ui`
//! Cargo feature; the shared launcher opens the window when no automation
//! flag is given.
//!
//! [`InputEvent::Key`]: emu198x_shell::InputEvent::Key

use emu198x_shell::FamilyRuntime;
use emu198x_shell::{FirmwareOverrides, MachineError, build_replacement};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_sega_mega_drive::{MegaDriveRuntime, Model};

use crate::app::MegaDrive;

const DEFAULT_SCALE: u32 = 3;

/// Player-1 pad. The names are the ones `runtime-sega-mega-drive`'s input
/// mapping takes; A B C sit in a row, so the host's west, south and east
/// buttons take them.
const MEGA_DRIVE_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::West, ButtonTarget::new(1, "a")),
    (HostControl::South, ButtonTarget::new(1, "b")),
    (HostControl::East, ButtonTarget::new(1, "c")),
    (HostControl::North, ButtonTarget::new(1, "x")),
    (HostControl::Start, ButtonTarget::new(1, "start")),
    (HostControl::Select, ButtonTarget::new(1, "mode")),
]);

/// The Mega Drive as a [`UiSystem`] for the shared harness. Switching
/// models cold-boots with the in-memory cartridge.
pub struct MegaDriveSystem {
    model: Model,
}

impl UiApp for MegaDrive {
    type System = MegaDriveSystem;

    fn ui_system(&self) -> MegaDriveSystem {
        MegaDriveSystem {
            model: self.model(),
        }
    }
}

impl UiSystem for MegaDriveSystem {
    type Runtime = MegaDriveRuntime;

    fn window_title(&self) -> String {
        "Emu198x Sega Mega Drive".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.frame_width(), machine.frame_height()))
            .unwrap_or((347, 243))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    /// A frame is 262 or 313 lines of the master clock, so the NTSC models
    /// run at 59.92 Hz and the PAL one at 49.70 Hz, not 60 and 50.
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
            operation: "unknown sega-mega-drive variant",
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
        &MEGA_DRIVE_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyZ => HostControl::West,
            KeyCode::KeyX => HostControl::South,
            KeyCode::KeyC => HostControl::East,
            KeyCode::KeyA => HostControl::North,
            KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // The six-button pad's buttons beyond the host's controls. The
        // runtime takes them as named key events on port 1.
        match code {
            KeyCode::KeyS => Some(&["y"]),
            KeyCode::KeyD => Some(&["z"]),
            KeyCode::ShiftRight => Some(&["mode"]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::MachineCore;

    fn cart() -> Vec<u8> {
        let mut rom = vec![0u8; 0x400];
        rom[0..4].copy_from_slice(&0x00FF_FE00u32.to_be_bytes());
        rom[4..8].copy_from_slice(&0x200u32.to_be_bytes());
        rom[0x200..0x202].copy_from_slice(&[0x60, 0xFE]); // BRA.S *
        rom
    }

    #[test]
    fn menu_switch_keeps_the_cartridge_and_tracks_live_pacing() {
        let mut system = MegaDriveSystem {
            model: Model::Genesis,
        };
        assert_eq!(
            system
                .variants()
                .iter()
                .map(|v| v.id.as_ref())
                .collect::<Vec<_>>(),
            Model::VARIANT_IDS
        );
        let mut runtime = MegaDriveRuntime::new(Model::Genesis, cart());
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
                49.70
            } else {
                59.92
            };
            assert!((hz - expected).abs() < 0.01, "{model:?} at {hz} Hz");
            let machine = runtime.machine().expect("machine");
            assert_eq!(machine.rom(), cart());
            assert_eq!(
                system.framebuffer_size(&runtime),
                (machine.frame_width(), machine.frame_height())
            );
        }
        let before = runtime.snapshot().expect("snapshot");
        assert!(system.switch_variant(&mut runtime, "sms-ntsc").is_err());
        assert_eq!(runtime.snapshot().expect("snapshot"), before);
    }

    #[test]
    fn pad_maps_and_six_button_keys() {
        let system = MegaDriveSystem {
            model: Model::Genesis,
        };
        assert_eq!(system.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(system.map_key(KeyCode::KeyZ), Some(HostControl::West));
        assert_eq!(system.map_key(KeyCode::KeyC), Some(HostControl::East));
        assert_eq!(system.map_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(system.map_keys(KeyCode::KeyS), Some(&["y"][..]));
        assert_eq!(system.map_keys(KeyCode::ShiftRight), Some(&["mode"][..]));
    }
}
