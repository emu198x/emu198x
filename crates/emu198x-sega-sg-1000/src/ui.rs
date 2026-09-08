//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Sega SG-1000's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed PSG audio, and
//! keyboard/gamepad input. The SG-1000 is a console — its pad is the harness's
//! console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]) — plus the
//! Pause button, which the runtime takes as an [`InputEvent::Key`] (`pause`),
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
use runtime_sega_sg_1000::{Model, Sg1000Runtime};

use crate::app::Sg1000;

const DEFAULT_SCALE: u32 = 3;

/// Player-1 control pad: directions plus the two face buttons. `south`/`east`
/// are the names `runtime-sega-sg-1000`'s `apply_button` maps to the pad's
/// button 1 / button 2. A real gamepad reaches these through the same map; the
/// keyboard does via [`UiSystem::map_key`].
const SG1000_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "south")),
    (HostControl::East, ButtonTarget::new(1, "east")),
]);

/// Native window adapter for the runtime catalogue. Region switches cold-boot
/// with the cartridge held in memory by the runtime.
pub struct Sg1000System {
    model: Model,
}

impl UiApp for Sg1000 {
    type System = Sg1000System;

    fn ui_system(&self) -> Sg1000System {
        Sg1000System { model: self.model }
    }
}

impl UiSystem for Sg1000System {
    type Runtime = Sg1000Runtime;

    fn window_title(&self) -> String {
        "Emu198x Sega SG-1000".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The SG-1000's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches
    // to fill it.

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
            operation: "unknown sega-sg-1000 variant",
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
        &SG1000_BUTTON_MAP
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
        // The console Pause button — a named key event, not a pad control.
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(&["pause"]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_startup_loads_parsed_cartridge_and_rejects_missing_media() {
        let dir =
            std::env::temp_dir().join(format!("sega-sg-1000-ui-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("directory");
        let cart = dir.join("cart.rom");
        std::fs::write(&cart, vec![0x5a; 8192]).expect("cartridge");
        let app = Sg1000 {
            cart: Some(cart.clone()),
            model: Model::Sg1000Ntsc,
        };
        let mut runtime = app.build_ui_runtime().expect("window runtime");
        assert!(runtime.cartridge_loaded());
        std::fs::remove_file(cart).expect("remove source");
        assert!(app.build_ui_runtime().is_err());
        let mut system = app.ui_system();
        system
            .switch_variant(&mut runtime, Model::Sg1000Pal.variant_id())
            .expect("switch");
        assert_eq!(
            system.current_variant().as_deref(),
            Some(Model::Sg1000Pal.variant_id())
        );
        assert_eq!(system.frame_ticks(&runtime), Model::Sg1000Pal.frame_ticks());
        assert!(runtime.cartridge_loaded());
        assert_eq!(runtime.machine().expect("cartridge").peek(0), 0x5a);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn menu_uses_runtime_ids_and_a_failed_switch_preserves_selection() {
        let mut system = Sg1000System {
            model: Model::Sg1000Ntsc,
        };
        let choices = system.variants();
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.id.as_ref())
                .collect::<Vec<_>>(),
            Model::VARIANT_IDS
        );
        let mut runtime = <Sg1000System as UiSystem>::Runtime::blank(Model::Sg1000Ntsc);
        assert!(system.switch_variant(&mut runtime, "unknown").is_err());
        assert_eq!(runtime.model(), Model::Sg1000Ntsc);
        assert_eq!(
            system.current_variant().as_deref(),
            Some(Model::Sg1000Ntsc.variant_id())
        );
        assert_eq!(system.frame_ticks(&runtime), runtime.native_frame_ticks());
    }

    #[test]
    fn pacing_and_dimensions_follow_the_runtime_region() {
        let system = Sg1000System {
            model: Model::Sg1000Ntsc,
        };
        for model in Model::ALL {
            let runtime = Sg1000Runtime::new(model, vec![0; 8192]);
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
    fn pad_and_pause_map() {
        let sys = Sg1000System {
            model: Model::Sg1000Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["pause"][..]));
        // Pad keys aren't keyboard keys (no double-routing).
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
    }
}
