//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari 7800's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed TIA audio, and
//! keyboard/gamepad input. The 7800 pad is a digital joystick + two fire
//! buttons — the harness's console path ([`UiSystem::map_key`] +
//! [`UiSystem::button_map`]) — plus the three console switches (Reset / Select
//! / Pause), which the runtime takes as named key events, routed through
//! [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo feature; the
//! shared launcher opens the window when no automation flag is given.

use std::time::Duration;

use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineError, build_replacement};
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_atari_7800::{Atari7800Runtime, Model};
use std::borrow::Cow;

use crate::app::Atari7800;

const DEFAULT_SCALE: u32 = 3;

/// Player-1 control: joystick directions, the two fire buttons, and the two
/// gamepad menu buttons mapped to the console Select / Reset switches. The
/// names are the ones `runtime-atari-7800`'s `set_control` understands.
const ATARI_7800_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire2")),
    (HostControl::Start, ButtonTarget::new(1, "select")),
    (HostControl::Select, ButtonTarget::new(1, "reset")),
]);

/// Native adapter for the runtime-owned regional catalogue.
pub struct Atari7800System {
    model: Model,
}

impl UiApp for Atari7800 {
    type System = Atari7800System;

    fn ui_system(&self) -> Atari7800System {
        Atari7800System { model: self.model }
    }
}

impl UiSystem for Atari7800System {
    type Runtime = Atari7800Runtime;

    fn window_title(&self) -> String {
        "Emu198x Atari 7800".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 7800 drove a 4:3 TV; its MARIA framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((374, 240))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(match runtime.model().region() {
            emu198x_shell::Region::Pal => 1.0 / 50.0,
            _ => 1.0 / 60.0,
        })
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
            operation: "unknown atari-7800 variant",
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
        &ATARI_7800_BUTTON_MAP
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
        // The three console switches — named key events, distinct from the
        // harness's own Esc-quit / F12-reset.
        Some(match code {
            KeyCode::Enter | KeyCode::NumpadEnter => &["select"],
            KeyCode::Backspace => &["reset"],
            KeyCode::Delete => &["pause"],
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_switches_live_region_and_preserves_cartridge() {
        let app = Atari7800::default();
        let mut system = app.ui_system();
        let mut runtime =
            Atari7800Runtime::new(Model::default(), vec![0x5a; 16384]).expect("runtime");
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
                (if pal { 368 } else { 374 }, if pal { 288 } else { 240 })
            );
            let expected_seconds = if pal { 1.0 / 50.0 } else { 1.0 / 60.0 };
            assert_eq!(
                system.frame_duration(&runtime),
                std::time::Duration::from_secs_f64(expected_seconds)
            );
            assert_eq!(runtime.machine().expect("machine").peek(49152), 0x5a);
        }
        let before = system.current_variant();
        assert!(system.switch_variant(&mut runtime, "unknown").is_err());
        assert_eq!(system.current_variant(), before);
    }

    #[test]
    fn pad_on_map_key_and_console_switches_on_map_keys() {
        let sys = Atari7800System {
            model: Model::A7800Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["select"][..]));
        assert_eq!(sys.map_keys(KeyCode::Backspace), Some(&["reset"][..]));
        assert_eq!(sys.map_keys(KeyCode::Delete), Some(&["pause"][..]));
        // No double-routing.
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
        assert_eq!(sys.map_key(KeyCode::Enter), None);
    }
}
