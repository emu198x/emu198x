//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Memotech MTX's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the full keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). The
//! MTX is keyboard-led; its joysticks share the keyboard matrix and are reached
//! by a real gamepad through [`UiSystem::button_map`] (the harness drains
//! gamepad events through the button map regardless of the keyboard path). The
//! cursor keys are genuine matrix cells, so they type rather than driving the
//! stick. Compiled only with the `ui` Cargo feature; the shared launcher opens
//! the window when no automation flag is given.

use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineError, build_variant};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_memotech_mtx::{Model, MtxRuntime};

use crate::app::Mtx;

const DEFAULT_SCALE: u32 = 3;
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 joystick: four directions plus fire, named as
/// `runtime-memotech-mtx`'s controller mirror expects. The MTX joystick shares
/// the keyboard matrix, so a real gamepad reaches it through the button map;
/// keyboard cursor keys deliberately do not, so they keep their MTX meaning.
const MTX_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The Memotech MTX as a [`UiSystem`] for the shared harness. It tracks the selected model;
/// a hard reset rebuilds the machine from the firmware
/// the runtime already holds.
pub struct MtxSystem {
    model: Model,
}

impl UiApp for Mtx {
    type System = MtxSystem;

    fn ui_system(&self) -> MtxSystem {
        MtxSystem { model: self.model }
    }
}

impl UiSystem for MtxSystem {
    type Runtime = MtxRuntime;

    fn window_title(&self) -> String {
        "Emu198x Memotech MTX".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The MTX's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
    // fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            // Before a machine exists, the NTSC window: 5.369318 MHz over
            // 52.148 µs by 240 lines. Was 288 x 240, a fixed border.
            .unwrap_or((278, 288))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn variants(&self) -> Vec<VariantInfo> {
        Model::ALL
            .iter()
            .map(|model| VariantInfo::new(model.variant_id(), model.menu_label()))
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
            operation: "unknown memotech-mtx variant",
        })?;
        *runtime =
            build_variant::<MtxRuntime>(model, &FirmwareOverrides::none()).map_err(|err| {
                MachineError::Host {
                    reason: err.to_string(),
                }
            })?;
        self.model = model;
        Ok(())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &MTX_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_mtx_keys(code)
    }
}

/// Map a physical host key to its MTX key name (matched by
/// `runtime-memotech-mtx`'s `key_from_name`). The cursor keys are genuine
/// matrix cells, so they map here rather than to the joystick. Shifted symbols
/// are reached by holding a shift with another key, so only the unshifted
/// legends need mapping. The MTX's own Escape key is unreachable — the harness
/// owns Esc for quit.
fn map_mtx_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Minus => &["-"],
        KeyCode::Backslash => &["\\"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Tab => &["tab"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::CapsLock => &["caps"],
        KeyCode::Delete => &["delete"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Home => &["home"],
        KeyCode::Insert => &["insert"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::F6 => &["f6"],
        KeyCode::F7 => &["f7"],
        KeyCode::F8 => &["f8"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_uses_runtime_ids_and_a_failed_switch_preserves_selection() {
        let mut system = MtxSystem {
            model: Model::Mtx500,
        };
        let choices = system.variants();
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.id.as_ref())
                .collect::<Vec<_>>(),
            Model::VARIANT_IDS
        );
        let mut runtime = <MtxSystem as UiSystem>::Runtime::blank(Model::Mtx500);
        assert!(system.switch_variant(&mut runtime, "unknown").is_err());
        assert_eq!(runtime.model(), Model::Mtx500);
        assert_eq!(
            system.current_variant().as_deref(),
            Some(Model::Mtx500.variant_id())
        );
        assert_eq!(system.frame_ticks(&runtime), runtime.native_frame_ticks());
    }

    #[test]
    fn cursor_keys_are_keyboard_cells_not_joystick() {
        // The MTX's cursor keys are genuine matrix cells, so they go through
        // the keyboard path and keep their MTX names.
        assert_eq!(map_mtx_keys(KeyCode::ArrowDown), Some(&["down"][..]));
        assert_eq!(map_mtx_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_mtx_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_mtx_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_mtx_keys(KeyCode::ShiftRight), Some(&["rshift"][..]));
        assert_eq!(map_mtx_keys(KeyCode::F1), Some(&["f1"][..]));
        // Keys with no MTX position are ignored.
        assert_eq!(map_mtx_keys(KeyCode::PageUp), None);
    }
}
