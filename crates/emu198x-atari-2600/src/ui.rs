//! Interactive UI mode — the default when no automation flag is present.
//!
//! A native Atari 2600 window built on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, framed TIA audio, and keyboard/gamepad
//! input. Compiled only with the `ui` Cargo feature; the shared launcher
//! opens the window when no automation flag is given.

use emu198x_shell::Region;
use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineError, build_replacement};
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_atari_2600::{Atari2600Runtime, Model};
use std::borrow::Cow;

use crate::app::Atari2600;

const DEFAULT_SCALE: u32 = 3;
const NTSC_COLOUR_HZ: f64 = 3_579_545.0;
const PAL_COLOUR_HZ: f64 = 3_546_894.0;

/// Joystick directions + fire (port 1) and the console RESET/SELECT switches.
/// The runtime ignores the port on the console switches, so port 1 is fine.
const ATARI_2600_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
    (HostControl::Start, ButtonTarget::new(1, "reset")),
    (HostControl::Select, ButtonTarget::new(1, "select")),
]);

/// The Atari 2600 as a [`UiSystem`] for the shared harness.
pub struct Atari2600System {
    model: Model,
}

impl UiApp for Atari2600 {
    type System = Atari2600System;

    fn ui_system(&self) -> Atari2600System {
        Atari2600System { model: self.model }
    }
}

impl UiSystem for Atari2600System {
    type Runtime = Atari2600Runtime;

    fn window_title(&self) -> String {
        "Emu198x Atari 2600".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 2600 drove a 4:3 TV. The harness derives the horizontal pixel stretch
    // from this and the cropped window height, matching Stella's proportions
    // (whose 160-wide framebuffer displays at a 4:3 viewable).

    // The runtime advances in whole frames, so a sub-frame target would
    // overshoot — run exactly one frame per slice.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| {
                // Visible window — the runtime crops the HBLANK margin and the
                // VBLANK/overscan lines out of the frames it presents, so the
                // window must match that cropped size, not the full raster.
                (
                    machine.visible_framebuffer_width(),
                    machine.visible_framebuffer_height(),
                )
            })
            .unwrap_or((160, 240))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> std::time::Duration {
        let hz = match runtime.model().region() {
            Region::Pal => PAL_COLOUR_HZ,
            _ => NTSC_COLOUR_HZ,
        };
        std::time::Duration::from_secs_f64(self.frame_ticks(runtime) as f64 / hz)
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
            operation: "unknown atari-2600 variant",
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
        &ATARI_2600_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyX | KeyCode::KeyZ | KeyCode::Space => HostControl::South,
            KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
            KeyCode::ShiftRight => HostControl::Select,
            _ => return None,
        })
    }

    /// Report a halted 6507 — a JAM/stop-code, almost always a corrupted ROM
    /// dump (a bad bank decodes a stop-code that hangs the CPU). F12 resets to
    /// clear it.
    fn halt_status(&self, runtime: &Self::Runtime) -> Option<String> {
        let cpu = runtime.machine()?.cpu();
        cpu.halted.then(|| {
            format!(
                "CPU halted (JAM) at ${:04X} — likely a bad ROM dump",
                cpu.regs.pc
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_atari_2600::Model;

    #[test]
    fn selector_switches_live_region_and_preserves_cartridge() {
        let app = Atari2600::default();
        let mut system = app.ui_system();
        let mut runtime =
            Atari2600Runtime::new(Model::default(), vec![0x5a; 4096]).expect("runtime");
        assert_eq!(system.variants().len(), 2);
        for model in Model::ALL {
            system
                .switch_variant(&mut runtime, model.variant_id())
                .expect("switch");
            assert_eq!(
                system.current_variant().as_deref(),
                Some(model.variant_id())
            );
            assert_eq!(runtime.model(), model);
            assert_eq!(system.frame_ticks(&runtime), model.frame_ticks());
            let pal = model.region() == emu198x_shell::Region::Pal;
            assert_eq!(
                system.framebuffer_size(&runtime),
                (160, if pal { 288 } else { 240 })
            );
            let expected_seconds =
                (model.frame_ticks() as f64) / if pal { PAL_COLOUR_HZ } else { NTSC_COLOUR_HZ };
            assert_eq!(
                system.frame_duration(&runtime),
                std::time::Duration::from_secs_f64(expected_seconds)
            );
            assert_eq!(runtime.machine().expect("machine").peek(4096), 0x5a);
        }
        let before = system.current_variant();
        assert!(system.switch_variant(&mut runtime, "unknown").is_err());
        assert_eq!(system.current_variant(), before);
    }

    #[test]
    fn system_frame_ticks_match_region() {
        let sys = Atari2600System {
            model: Model::Vcs2600Ntsc,
        };
        let ntsc = Atari2600Runtime::blank(Model::Vcs2600Ntsc);
        let pal = Atari2600Runtime::blank(Model::Vcs2600Pal);
        assert_eq!(sys.frame_ticks(&ntsc), 262 * 228);
        assert_eq!(sys.frame_ticks(&pal), 312 * 228);
    }

    #[test]
    fn maps_joystick_and_console_keys() {
        let sys = Atari2600System {
            model: Model::Vcs2600Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(sys.map_key(KeyCode::ShiftRight), Some(HostControl::Select));
        assert_eq!(sys.map_key(KeyCode::KeyQ), None);
    }
}
