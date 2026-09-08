//! Interactive UI mode — the default when no automation flag is present.
//!
//! The ColecoVision's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed PSG audio, and
//! keyboard/gamepad input. The Coleco controller is a joystick + two fire
//! buttons + a 12-key numeric keypad: the joystick and fire go through the
//! harness's console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]),
//! and the keypad digits / `*` / `#` are named key events on controller 1,
//! routed through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo
//! feature; the shared launcher opens the window when no automation flag is
//! given.

use emu198x_shell::{
    FamilyRuntime, FirmwareOverrides, MachineCore, MachineError, build_replacement,
};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_coleco_colecovision::{CvRuntime, Model};

use crate::app::ColecoVision;

const DEFAULT_SCALE: u32 = 3;

/// Player-1 controller: joystick directions plus the two fire buttons.
/// `south`/`east` are the names `runtime-coleco-colecovision`'s `apply_button`
/// maps to the controller's left / right fire buttons. A real gamepad reaches
/// these through the same map; the keyboard does via [`UiSystem::map_key`].
const COLECO_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "south")),
    (HostControl::East, ButtonTarget::new(1, "east")),
]);

/// Native window adapter for the runtime catalogue. Region switches cold-boot
/// with the cartridge held in memory by the runtime.
pub struct ColecoSystem {
    model: Model,
}

impl UiApp for ColecoVision {
    type System = ColecoSystem;

    fn ui_system(&self) -> ColecoSystem {
        ColecoSystem { model: self.model }
    }
}

impl UiSystem for ColecoSystem {
    type Runtime = CvRuntime;

    fn window_title(&self) -> String {
        "Emu198x ColecoVision".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Coleco's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
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
            .unwrap_or((280, 240))
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
            operation: "unknown colecovision variant",
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
        &COLECO_BUTTON_MAP
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
        // The 12-key numeric keypad — named key events on controller 1. Digits
        // come from both the top row and the numeric keypad; `*` and `#` from
        // the numpad operator keys.
        Some(match code {
            KeyCode::Digit0 | KeyCode::Numpad0 => &["0"],
            KeyCode::Digit1 | KeyCode::Numpad1 => &["1"],
            KeyCode::Digit2 | KeyCode::Numpad2 => &["2"],
            KeyCode::Digit3 | KeyCode::Numpad3 => &["3"],
            KeyCode::Digit4 | KeyCode::Numpad4 => &["4"],
            KeyCode::Digit5 | KeyCode::Numpad5 => &["5"],
            KeyCode::Digit6 | KeyCode::Numpad6 => &["6"],
            KeyCode::Digit7 | KeyCode::Numpad7 => &["7"],
            KeyCode::Digit8 | KeyCode::Numpad8 => &["8"],
            KeyCode::Digit9 | KeyCode::Numpad9 => &["9"],
            KeyCode::NumpadMultiply => &["*"],
            KeyCode::NumpadDivide => &["#"],
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_startup_loads_parsed_cartridge_and_rejects_missing_media() {
        let dir =
            std::env::temp_dir().join(format!("colecovision-ui-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("directory");
        let cart = dir.join("cart.rom");
        std::fs::write(&cart, vec![0x5a; 8192]).expect("cartridge");
        let app = ColecoVision {
            cart: Some(cart.clone()),
            firmware: {
                let bios = dir.join("bios.rom");
                std::fs::write(&bios, vec![0; 8192]).expect("BIOS");
                let mut firmware = FirmwareOverrides::none();
                firmware.by_id.insert(
                    runtime_coleco_colecovision::BIOS_FIRMWARE_ID.to_owned(),
                    bios,
                );
                firmware
            },
            model: Model::CvNtsc,
        };
        let mut runtime = app.build_ui_runtime().expect("window runtime");
        assert!(runtime.cartridge_loaded());
        std::fs::remove_file(cart).expect("remove source");
        assert!(app.build_ui_runtime().is_err());
        runtime = build_replacement(&runtime, Model::CvPal, &app.firmware)
            .expect("window replacement builder");
        assert!(runtime.cartridge_loaded());
        assert_eq!(runtime.machine().expect("cartridge").peek(32768), 0x5a);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn menu_uses_runtime_ids_and_a_failed_switch_preserves_selection() {
        let mut system = ColecoSystem {
            model: Model::CvNtsc,
        };
        let choices = system.variants();
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.id.as_ref())
                .collect::<Vec<_>>(),
            Model::VARIANT_IDS
        );
        let mut runtime = <ColecoSystem as UiSystem>::Runtime::blank(Model::CvNtsc);
        assert!(system.switch_variant(&mut runtime, "unknown").is_err());
        assert_eq!(runtime.model(), Model::CvNtsc);
        assert_eq!(
            system.current_variant().as_deref(),
            Some(Model::CvNtsc.variant_id())
        );
        assert_eq!(system.frame_ticks(&runtime), runtime.native_frame_ticks());
    }

    #[test]
    fn pacing_and_dimensions_follow_the_runtime_region() {
        let system = ColecoSystem {
            model: Model::CvNtsc,
        };
        for model in Model::ALL {
            let runtime = CvRuntime::new(model, vec![0; 8192]).expect("BIOS");
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
                (machine.framebuffer_width(), machine.framebuffer_height())
            );
        }
    }

    #[test]
    fn joystick_on_map_key_and_keypad_on_map_keys() {
        let sys = ColecoSystem {
            model: Model::CvNtsc,
        };
        // Joystick + fire on the console path.
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::East));
        // Keypad on the keyboard path — and not also a joystick control.
        assert_eq!(sys.map_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(sys.map_keys(KeyCode::Numpad5), Some(&["5"][..]));
        assert_eq!(sys.map_keys(KeyCode::NumpadMultiply), Some(&["*"][..]));
        assert_eq!(sys.map_keys(KeyCode::NumpadDivide), Some(&["#"][..]));
        assert_eq!(sys.map_key(KeyCode::Digit5), None);
        // Arrows are joystick, not keypad.
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
    }
}
