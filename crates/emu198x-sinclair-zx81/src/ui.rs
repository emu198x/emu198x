//! Interactive UI mode — the default when no automation flag is present.
//!
//! The ZX81's first native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters and the full membrane keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). The
//! ZX81 is keyboard-only — no sound, joystick, or mouse — which makes it the
//! proving ground for the harness's home-computer keyboard input. Compiled only
//! with the `ui` Cargo feature; the shared launcher opens the window when no
//! automation flag is given.

use std::borrow::Cow;
use std::time::Duration;

use emu198x_shell::{FirmwareOverrides, MachineError, build_variant};
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, KeyCode, UiSystem, VariantInfo};
use runtime_sinclair_zx81::{Model, Zx81Runtime};

use crate::app::Zx81;

const DEFAULT_SCALE: u32 = 3;

/// The frame budget comes from the board strap now — see
/// `TelevisionStandard::slow_mode_frame_tstates`.
/// The Z80 clock, for turning a frame's T-states into a wall-clock duration.
const CPU_HZ: f64 = 3_250_000.0;

/// The ZX81 has no joystick, but the harness still wants a button map — so an
/// empty one. Every key flows through [`UiSystem::map_keys`] instead.
const ZX81_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

/// The ZX81 as a [`UiSystem`] for the shared harness. Keyboard-only and
/// Carries only the board strap, so a hard reset rebuilds the machine from the
/// firmware the runtime already holds.
pub struct Zx81System {
    model: Model,
}

impl UiApp for Zx81 {
    type System = Zx81System;

    fn ui_system(&self) -> Zx81System {
        Zx81System {
            model: self.model(),
        }
    }
}

impl UiSystem for Zx81System {
    type Runtime = Zx81Runtime;

    fn window_title(&self) -> String {
        "Emu198x ZX81".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    /// The runtime catalogue owns the RAM and television-standard variants.
    fn variants(&self) -> Vec<VariantInfo> {
        Model::ALL
            .iter()
            .map(|model| VariantInfo::new(model.profile_id(), model.display_name()))
            .collect()
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(self.model.profile_id()))
    }

    /// Switch to the model's conventional ROM and RAM configuration.
    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let model = Model::from_variant_id(variant).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown ZX81 variant",
        })?;
        *runtime =
            build_variant::<Zx81Runtime>(model, &FirmwareOverrides::none()).map_err(|err| {
                MachineError::Host {
                    reason: err.to_string(),
                }
            })?;
        self.model = model;
        Ok(())
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((320, 288))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        u64::from(
            runtime
                .model()
                .television_standard()
                .slow_mode_frame_tstates(),
        )
    }

    /// Paced from the frame the ROM actually lays out, not a nominal 50. A
    /// 50 Hz ZX81 runs at 50.65 Hz and a 60 Hz one at 59.93 Hz, both because
    /// the ROM decides the field length.
    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(f64::from(self.frame_ticks(runtime) as u32) / CPU_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ZX81_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_zx81_keys(code)
    }
}

/// Map a physical host key to its ZX81 membrane key name. The ZX81's symbols
/// and keywords are Shift-layer combos, reached by holding Shift with a key, so
/// only the base keys need mapping here.
fn map_zx81_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
    fn maps_membrane_keys_and_shift() {
        assert_eq!(map_zx81_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_zx81_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_zx81_keys(KeyCode::Enter), Some(&["newline"][..]));
        assert_eq!(map_zx81_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(map_zx81_keys(KeyCode::Space), Some(&["space"][..]));
        // Keys with no ZX81 membrane position are ignored.
        assert_eq!(map_zx81_keys(KeyCode::Tab), None);
    }
}
