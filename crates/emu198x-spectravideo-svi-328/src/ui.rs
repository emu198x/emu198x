//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Spectravideo SVI-328's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and the full keyboard
//! routed through the harness's general-keyboard path ([`UiSystem::map_keys`]).
//! The SVI-328 is keyboard-led; its cursor keys are genuine matrix cells, so
//! they type rather than driving the stick. The joystick is reached by a real
//! gamepad through [`UiSystem::button_map`]. Compiled only with the `ui`
//! Cargo feature; the shared launcher opens the window when no automation
//! flag is given.

use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineCore, MachineError, build_variant};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

use crate::app::Svi328;

const DEFAULT_SCALE: u32 = 3;
/// Player-1 joystick: four directions plus fire, named as
/// `runtime-spectravideo-svi-328`'s controller mirror expects. The cursor keys
/// are keyboard cells, so a real gamepad reaches the stick through this map.
const SVI_328_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// Native window adapter for the runtime catalogue.
pub struct Svi328System {
    model: Model,
}

impl UiApp for Svi328 {
    type System = Svi328System;

    fn ui_system(&self) -> Svi328System {
        Svi328System { model: self.model }
    }
}

impl UiSystem for Svi328System {
    type Runtime = Svi328Runtime;

    fn window_title(&self) -> String {
        "Emu198x Spectravideo SVI-328".to_owned()
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
        let region = match runtime.model() {
            Model::Svi328Ntsc => ti_tms9918::VdpRegion::Ntsc,
            Model::Svi328Pal => ti_tms9918::VdpRegion::Pal,
        };
        (region.framebuffer_width(), region.framebuffer_height())
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(match runtime.profile().region {
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
            operation: "unknown spectravideo-svi-328 variant",
        })?;
        *runtime =
            build_variant::<Svi328Runtime>(model, &FirmwareOverrides::none()).map_err(|err| {
                MachineError::Host {
                    reason: err.to_string(),
                }
            })?;
        self.model = model;
        Ok(())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SVI_328_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_svi_keys(code)
    }
}

/// Map a physical host key to its SVI-328 key name (matched by
/// `runtime-spectravideo-svi-328`'s `key_to_matrix`). The cursor keys are
/// genuine matrix cells, so they map here rather than to the joystick. Shifted
/// symbols are reached by holding SHIFT; host Alt is the GRAPH/CODE key. The
/// SVI's own Escape key is unreachable — the harness owns Esc for quit.
fn map_svi_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &["'"],
        KeyCode::Comma => &[","],
        KeyCode::Equal => &["="],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Minus => &["-"],
        KeyCode::BracketLeft => &["["],
        KeyCode::Backslash => &["\\"],
        KeyCode::BracketRight => &["]"],
        KeyCode::Space => &["space"],
        KeyCode::Tab => &["tab"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Delete => &["delete"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["graph"],
        KeyCode::CapsLock => &["caps"],
        KeyCode::Home => &["home"],
        KeyCode::Insert => &["insert"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_startup_loads_the_parsed_cartridge_and_refuses_missing_media() {
        let dir = std::env::temp_dir().join(format!(
            "spectravideo-svi-328-ui-media-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("directory");
        let rom = dir.join("firmware.rom");
        let cart = dir.join("cart.rom");
        std::fs::write(&rom, vec![0; 32768]).expect("firmware");
        std::fs::write(&cart, vec![0x5a; 8192]).expect("cartridge");
        let mut app = Svi328::default();
        app.firmware.by_id.insert(
            runtime_spectravideo_svi_328::BIOS_FIRMWARE_ID.to_owned(),
            rom,
        );
        app.cart = Some(cart.clone());
        let runtime = app.build_ui_runtime().expect("window runtime");
        assert!(runtime.cartridge_loaded());
        std::fs::remove_file(cart).expect("remove cartridge");
        assert!(app.build_ui_runtime().is_err());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn menu_uses_runtime_ids_and_a_failed_switch_preserves_selection() {
        let mut system = Svi328System {
            model: Model::Svi328Ntsc,
        };
        let choices = system.variants();
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.id.as_ref())
                .collect::<Vec<_>>(),
            Model::VARIANT_IDS
        );
        let mut runtime = <Svi328System as UiSystem>::Runtime::blank(Model::Svi328Ntsc);
        assert!(system.switch_variant(&mut runtime, "unknown").is_err());
        assert_eq!(runtime.model(), Model::Svi328Ntsc);
        assert_eq!(
            system.current_variant().as_deref(),
            Some(Model::Svi328Ntsc.variant_id())
        );
        assert_eq!(system.frame_ticks(&runtime), runtime.native_frame_ticks());
    }

    #[test]
    fn pacing_and_dimensions_follow_the_runtime_region() {
        let system = Svi328System {
            model: Model::Svi328Ntsc,
        };
        for model in Model::ALL {
            let runtime = Svi328Runtime::new(model, vec![0; 32768]).expect("firmware");
            assert_eq!(system.frame_ticks(&runtime), model.frame_ticks());
            let hz = if model.region() == emu198x_shell::Region::Pal {
                50.0
            } else {
                60.0
            };
            assert_eq!(
                system.frame_duration(&runtime),
                Duration::from_secs_f64(1.0 / hz)
            );
            let machine = runtime.machine().expect("machine");
            assert_eq!(
                system.framebuffer_size(&runtime),
                (
                    machine.vdp().framebuffer_width(),
                    machine.vdp().framebuffer_height()
                )
            );
        }
    }

    #[test]
    fn cursor_keys_are_keyboard_cells_and_graph_maps() {
        // The SVI's cursor keys are genuine matrix cells, so they type.
        assert_eq!(map_svi_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_svi_keys(KeyCode::ArrowRight), Some(&["right"][..]));
        assert_eq!(map_svi_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_svi_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_svi_keys(KeyCode::AltLeft), Some(&["graph"][..]));
        assert_eq!(map_svi_keys(KeyCode::F1), Some(&["f1"][..]));
        // Keys with no SVI position are ignored.
        assert_eq!(map_svi_keys(KeyCode::PageUp), None);
    }
}
