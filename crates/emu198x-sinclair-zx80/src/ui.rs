//! Interactive UI mode — the default when no automation flag is present.
//!
//! The ZX80's first native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters and the membrane keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). Like
//! its ZX81 sibling the ZX80 is keyboard-only — no sound, joystick, or mouse.
//! Compiled only with the `ui` Cargo feature; the shared launcher opens the
//! window when no automation flag is given.

use std::borrow::Cow;
use std::time::Duration;

use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineCore, MachineError, build_variant};

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, KeyCode, UiSystem, VariantInfo};
use runtime_sinclair_zx80::{Model, Zx80Runtime};

use crate::app::Zx80;

const DEFAULT_SCALE: u32 = 3;

/// The ZX80 has no joystick; the harness still wants a button map, so an empty
/// one. Every key flows through [`UiSystem::map_keys`].
const ZX80_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

/// The ZX80 as a [`UiSystem`] for the shared harness. Keyboard-only and
/// carries the selected preset for the menu. A hard reset rebuilds the machine
/// from its current firmware; switching installs a fresh configuration.
pub struct Zx80System {
    model: Model,
}

impl UiApp for Zx80 {
    type System = Zx80System;

    fn ui_system(&self) -> Zx80System {
        Zx80System { model: self.model }
    }
}

impl UiSystem for Zx80System {
    type Runtime = Zx80Runtime;

    fn window_title(&self) -> String {
        "Emu198x ZX80".to_owned()
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
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((320, 288))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        let rate = &runtime.profile().clock.rate;
        Duration::from_secs_f64(
            runtime.native_frame_ticks() as f64 * rate.denominator_hz as f64
                / rate.numerator_hz as f64,
        )
    }

    fn variants(&self) -> Vec<VariantInfo> {
        Model::ALL
            .iter()
            .map(|model| VariantInfo::new(model.profile_id(), model.menu_label()))
            .collect()
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(self.model.profile_id()))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let model = Model::from_variant_id(variant).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown ZX80 variant",
        })?;
        *runtime =
            build_variant::<Zx80Runtime>(model, &FirmwareOverrides::none()).map_err(|err| {
                MachineError::Host {
                    reason: err.to_string(),
                }
            })?;
        self.model = model;
        Ok(())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ZX80_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_zx80_keys(code)
    }
}

/// Map a physical host key to its ZX80 membrane key name. The ZX80's symbols
/// and keywords are Shift-layer combos reached by holding Shift, so only the
/// base keys need mapping.
fn map_zx80_keys(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
        KeyCode::KeyA => &["a"],
        KeyCode::KeyB => &["b"],
        KeyCode::KeyC => &["c"],
        KeyCode::KeyD => &["d"],
        KeyCode::KeyE => &["e"],
        KeyCode::KeyF => &["f"],
        KeyCode::KeyG => &["g"],
        KeyCode::KeyH => &["h"],
        KeyCode::KeyI => &["i"],
        KeyCode::KeyJ => &["j"],
        KeyCode::KeyK => &["k"],
        KeyCode::KeyL => &["l"],
        KeyCode::KeyM => &["m"],
        KeyCode::KeyN => &["n"],
        KeyCode::KeyO => &["o"],
        KeyCode::KeyP => &["p"],
        KeyCode::KeyQ => &["q"],
        KeyCode::KeyR => &["r"],
        KeyCode::KeyS => &["s"],
        KeyCode::KeyT => &["t"],
        KeyCode::KeyU => &["u"],
        KeyCode::KeyV => &["v"],
        KeyCode::KeyW => &["w"],
        KeyCode::KeyX => &["x"],
        KeyCode::KeyY => &["y"],
        KeyCode::KeyZ => &["z"],
        KeyCode::Digit0 => &["0"],
        KeyCode::Digit1 => &["1"],
        KeyCode::Digit2 => &["2"],
        KeyCode::Digit3 => &["3"],
        KeyCode::Digit4 => &["4"],
        KeyCode::Digit5 => &["5"],
        KeyCode::Digit6 => &["6"],
        KeyCode::Digit7 => &["7"],
        KeyCode::Digit8 => &["8"],
        KeyCode::Digit9 => &["9"],
        KeyCode::Space => &["space"],
        KeyCode::Period => &["."],
        KeyCode::Enter | KeyCode::NumpadEnter => &["newline"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_and_pacing_follow_the_runtime_catalogue() {
        let system = Zx80System { model: Model::Zx80 };
        let choices = system.variants();
        let ids: Vec<_> = choices.iter().map(|choice| choice.id.as_ref()).collect();
        assert_eq!(ids, Model::VARIANT_IDS);
        assert!(choices[2].label.contains("RAM pack"));
        let pal = Zx80Runtime::blank(Model::Zx80);
        let ntsc = Zx80Runtime::blank(Model::Zx80Usa);
        assert_eq!(system.frame_ticks(&ntsc), ntsc.native_frame_ticks());
        assert!(system.frame_duration(&ntsc) < system.frame_duration(&pal));
    }

    #[test]
    fn an_unknown_variant_leaves_the_window_and_machine_unchanged() {
        let mut system = Zx80System {
            model: Model::Zx80RamPack,
        };
        let mut runtime = Zx80Runtime::blank(Model::Zx80RamPack);
        assert!(system.switch_variant(&mut runtime, "unknown").is_err());
        assert_eq!(runtime.model(), Model::Zx80RamPack);
        assert_eq!(runtime.ram_bytes(), 16384);
        assert_eq!(
            system.current_variant().as_deref(),
            Some("sinclair-zx80-16k")
        );
    }

    #[test]
    fn maps_membrane_keys_and_shift() {
        assert_eq!(map_zx80_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_zx80_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_zx80_keys(KeyCode::Enter), Some(&["newline"][..]));
        assert_eq!(map_zx80_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(map_zx80_keys(KeyCode::Tab), None);
    }
}
