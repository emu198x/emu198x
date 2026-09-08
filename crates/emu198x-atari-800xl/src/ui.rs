//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari 800XL's first native window on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed POKEY audio, and
//! keyboard + gamepad input. The 800XL is a home computer, so the keyboard
//! types (letters/digits/symbols via [`UiSystem::map_keys`], feeding the POKEY
//! scan-code path) and the joystick is reached by a gamepad — or the host arrow
//! keys — through the console path ([`UiSystem::map_key`] +
//! [`UiSystem::button_map`]). The three console keys (Start/Select/Option) are
//! momentary named key events on F2/F3/F4. Compiled only with the `ui` Cargo
//! feature; the shared launcher opens the window when no automation flag is
//! given.

use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineError, build_replacement};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_atari_800xl::{Atari800xlRuntime, Model};

use crate::app::Atari800xl;

const DEFAULT_SCALE: u32 = 3;

/// Player-1 joystick: directions + fire, driven by a gamepad or the host arrow
/// keys. The runtime maps these names onto the PIA port-A controller bits.
const ATARI_800XL_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// Native adapter backed by the runtime's regional catalogue.
pub struct Atari800xlSystem {
    model: Model,
}

impl UiApp for Atari800xl {
    type System = Atari800xlSystem;

    fn ui_system(&self) -> Atari800xlSystem {
        Atari800xlSystem { model: self.model }
    }
}

impl UiSystem for Atari800xlSystem {
    type Runtime = Atari800xlRuntime;

    fn window_title(&self) -> String {
        "Emu198x Atari 800XL".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 800XL drove a 4:3 TV; its GTIA framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            // Before a machine exists, the NTSC window: 7.15909 MHz over
            // 52.148 µs by 240 lines. Was 384 x 240, a fixed border.
            .unwrap_or((374, 240))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(match runtime.model() {
            Model::A800xlPal => 1.0 / 50.0,
            Model::A800xlNtsc => 1.0 / 60.0,
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
            operation: "unknown Atari 800XL variant",
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
        &ATARI_800XL_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        // Host arrow keys drive the joystick (the 800XL's own cursor movement
        // is Ctrl+key, not a plain scan code, so the arrows are best spent on
        // the joystick). Fire is a gamepad button — letters must stay typeable.
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
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
            KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
            KeyCode::Space => &["space"],
            KeyCode::Backspace | KeyCode::Delete => &["delete"],
            KeyCode::Tab => &["tab"],
            // Console keys — momentary, distinct from the harness's Esc/F12.
            KeyCode::F2 => &["start"],
            KeyCode::F3 => &["select"],
            KeyCode::F4 => &["option"],
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_startup_and_pacing_follow_the_runtime_catalogue() {
        let dir = std::env::temp_dir().join(format!("a800xl-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("directory");
        std::fs::write(dir.join("atarixl.rom"), vec![0; 16384]).expect("OS");
        for model in Model::ALL {
            let app = Atari800xl {
                model,
                firmware: FirmwareOverrides {
                    dir: Some(dir.clone()),
                    ..FirmwareOverrides::none()
                },
                basic_enabled: false,
                ..Atari800xl::default()
            };
            let mut runtime = app.build_ui_runtime().expect("runtime");
            let mut system = app.ui_system();
            assert_eq!(system.variants().len(), 2);
            assert_eq!(
                system.current_variant().as_deref(),
                Some(model.variant_id())
            );
            assert!(!runtime.basic_enabled());
            assert_eq!(system.frame_ticks(&runtime), model.frame_ticks());
            assert!(system.switch_variant(&mut runtime, "unknown").is_err());
            for other in Model::ALL {
                runtime = Atari800xlRuntime::new(other, Some(vec![0; 16384]), None, None, false)
                    .expect("live runtime");
                assert_eq!(system.frame_ticks(&runtime), other.frame_ticks());
                assert_eq!(
                    system.frame_duration(&runtime),
                    Duration::from_secs_f64(if other == Model::A800xlPal {
                        1.0 / 50.0
                    } else {
                        1.0 / 60.0
                    })
                );
                let machine = runtime.machine().expect("machine");
                assert_eq!(
                    system.framebuffer_size(&runtime),
                    (machine.framebuffer_width(), machine.framebuffer_height())
                );
            }
        }
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
