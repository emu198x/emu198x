//! Interactive UI mode — the default when no automation flag is present.
//!
//! A native NES window built on the shared `emu198x-ui` harness: wgpu video
//! with `raw`/`lcd`/`crt` filters, framed APU audio, and keyboard/gamepad
//! controller input. Compiled only with the `ui` Cargo feature; the shared
//! launcher opens the window when no automation flag is given.
//!
//! Beyond the harness defaults the NES adds two hooks: per-system shortcuts
//! (the `1`-`5` / `6`-`0` APU channel debug controls) via
//! [`UiSystem::handle_key`], and a teardown that flushes cartridge battery RAM
//! to its `.sav` sidecar via [`UiSystem::on_exit`].

use emu198x_shell::{FamilyRuntime, FirmwareOverrides, build_replacement};
use std::borrow::Cow;
use std::path::PathBuf;

use emu198x_shell::MachineError;
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use machine_nintendo_nes::{FB_HEIGHT, FB_WIDTH};
use runtime_nintendo_nes::{ApuChannel, Model, NesRuntime};

use crate::app::{Nes, resolve_battery_save_path, write_battery_save};

const DEFAULT_SCALE: u32 = 3;
const INPUT_SLICES_PER_FRAME: u32 = 4;

const NES_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "a")),
    (HostControl::East, ButtonTarget::new(1, "b")),
    (HostControl::West, ButtonTarget::new(1, "b")),
    (HostControl::Start, ButtonTarget::new(1, "start")),
    (HostControl::Select, ButtonTarget::new(1, "select")),
]);

/// Native region selection and the battery-save path (flushed on exit).
pub struct NesSystem {
    model: Model,
    battery_save_path: Option<PathBuf>,
}

impl UiApp for Nes {
    type System = NesSystem;

    fn ui_system(&self) -> NesSystem {
        NesSystem {
            model: self.model,
            battery_save_path: resolve_battery_save_path(self),
        }
    }
}

impl UiSystem for NesSystem {
    type Runtime = NesRuntime;

    fn window_title(&self) -> String {
        "Emu198x NES".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The runtime honours sub-frame targets, so finer slices cut input latency.
    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> std::time::Duration {
        std::time::Duration::from_secs_f64(
            runtime.native_frame_ticks() as f64 / runtime.model().ppu_dot_hz() as f64,
        )
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &NES_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        map_nes_key(code)
    }

    fn variants(&self) -> Vec<VariantInfo> {
        Model::ALL
            .into_iter()
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
            operation: "unknown NES region",
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

    /// The `1`-`5` / `6`-`0` digit row toggles / cycles APU channels for audio
    /// debugging; consume those keys so they aren't treated as buttons.
    fn handle_key(&mut self, runtime: &mut Self::Runtime, code: KeyCode, pressed: bool) -> bool {
        let action = match code {
            KeyCode::Digit1 => ApuShortcut::Toggle(ApuChannel::Pulse1),
            KeyCode::Digit2 => ApuShortcut::Toggle(ApuChannel::Pulse2),
            KeyCode::Digit3 => ApuShortcut::Toggle(ApuChannel::Triangle),
            KeyCode::Digit4 => ApuShortcut::Toggle(ApuChannel::Noise),
            KeyCode::Digit5 => ApuShortcut::Toggle(ApuChannel::Dmc),
            KeyCode::Digit6 => ApuShortcut::Gain(ApuChannel::Pulse1),
            KeyCode::Digit7 => ApuShortcut::Gain(ApuChannel::Pulse2),
            KeyCode::Digit8 => ApuShortcut::Gain(ApuChannel::Triangle),
            KeyCode::Digit9 => ApuShortcut::Gain(ApuChannel::Noise),
            KeyCode::Digit0 => ApuShortcut::Gain(ApuChannel::Dmc),
            _ => return false,
        };
        if pressed {
            action.apply(runtime);
        }
        true
    }

    /// Persist the cartridge's battery PRG-RAM to its `.sav` on the way out.
    fn on_exit(&mut self, runtime: &mut Self::Runtime) -> Result<(), String> {
        match &self.battery_save_path {
            Some(path) => write_battery_save(runtime, path),
            None => Ok(()),
        }
    }
}

/// An APU debug shortcut: mute/unmute a channel, or cycle its gain.
enum ApuShortcut {
    Toggle(ApuChannel),
    Gain(ApuChannel),
}

impl ApuShortcut {
    fn apply(self, runtime: &mut NesRuntime) {
        match self {
            Self::Toggle(channel) => {
                let Some(controls) = runtime.audio_controls() else {
                    return;
                };
                let enabled = !controls.channel(channel).enabled();
                runtime.set_audio_channel_enabled(channel, enabled);
                eprintln!(
                    "audio: {} {}",
                    channel.label(),
                    if enabled { "enabled" } else { "muted" }
                );
            }
            Self::Gain(channel) => {
                let Some(controls) = runtime.audio_controls() else {
                    return;
                };
                let next = next_audio_gain(controls.channel(channel).gain());
                runtime.set_audio_channel_gain(channel, next);
                eprintln!("audio: {} gain {:.0}%", channel.label(), next * 100.0);
            }
        }
    }
}

fn next_audio_gain(gain: f32) -> f32 {
    if gain > 0.75 {
        0.5
    } else if gain > 0.375 {
        0.25
    } else if gain > 0.0 {
        0.0
    } else {
        1.0
    }
}

fn map_nes_key(code: KeyCode) -> Option<HostControl> {
    Some(match code {
        KeyCode::KeyX => HostControl::South,
        KeyCode::KeyZ => HostControl::East,
        KeyCode::ShiftRight => HostControl::Select,
        KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
        KeyCode::ArrowUp => HostControl::Up,
        KeyCode::ArrowDown => HostControl::Down,
        KeyCode::ArrowLeft => HostControl::Left,
        KeyCode::ArrowRight => HostControl::Right,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_controls_to_controller_buttons() {
        assert_eq!(map_nes_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(map_nes_key(KeyCode::KeyZ), Some(HostControl::East));
        assert_eq!(map_nes_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(map_nes_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(map_nes_key(KeyCode::Digit1), None);
    }

    #[test]
    fn audio_gain_cycles_through_debug_levels() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}

#[cfg(test)]
mod catalogue_tests {
    use super::*;
    use emu198x_shell::MediaKind;

    #[test]
    fn native_startup_and_switching_use_both_catalogue_regions() {
        let mut app = Nes::default();
        app.no_battery_save = true;
        app.media.push(crate::app::MediaArg {
            slot: "cartridge-1".to_owned(),
            kind: MediaKind::Cartridge,
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../test-data/synthetic-cartridges/nintendo-nes-logo.nes"),
        });
        let mut runtime = app.build_ui_runtime().expect("native runtime");
        let mut system = app.ui_system();
        assert_eq!(system.variants().len(), 2);
        for model in Model::ALL {
            system
                .switch_variant(&mut runtime, model.variant_id())
                .expect("switch");
            assert_eq!(runtime.model(), model);
            assert_eq!(
                runtime.machine().expect("machine").region(),
                model.machine_region()
            );
            assert_eq!(
                system.current_variant().as_deref(),
                Some(model.variant_id())
            );
            assert_eq!(system.frame_ticks(&runtime), model.frame_ticks());
            assert_eq!(
                system.frame_duration(&runtime),
                std::time::Duration::from_secs_f64(
                    model.frame_ticks() as f64 / model.ppu_dot_hz() as f64
                )
            );
        }
        assert!(system.switch_variant(&mut runtime, "dendy").is_err());
        assert!(runtime.machine().is_some());
        assert_eq!(system.frame_ticks(&runtime), runtime.model().frame_ticks());
        assert_eq!(system.framebuffer_size(&runtime), (FB_WIDTH, FB_HEIGHT));
    }
}
