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

use std::path::PathBuf;

use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, read_media_asset};
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use machine_nintendo_nes::{FB_HEIGHT, FB_WIDTH};
use runtime_nintendo_nes::{ApuChannel, NesRuntime};

use crate::app::{NES_FRAME_TICKS, Nes, resolve_battery_save_path, write_battery_save};

const DEFAULT_SCALE: u32 = 3;
const INPUT_SLICES_PER_FRAME: u32 = 4;
const NES_PPU_DOT_HZ: f64 = 5_369_318.0;

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

/// The NES as a [`UiSystem`] for the shared harness. Holds the cartridge bytes
/// (to re-insert on a hard reset) and the battery-save path (to flush on exit).
pub struct NesSystem {
    cartridge_media: Vec<u8>,
    battery_save_path: Option<PathBuf>,
}

impl UiApp for Nes {
    type System = NesSystem;

    /// The cartridge bytes are read here for the reset path; a file that
    /// cannot be read leaves them empty, and `build_runtime` reports the
    /// failure on the same path a moment later.
    fn ui_system(&self) -> NesSystem {
        let cartridge_media = self
            .media
            .iter()
            .find(|entry| entry.kind == MediaKind::Cartridge)
            .and_then(|entry| read_media_asset(&entry.path, entry.kind).ok())
            .map(|loaded| loaded.bytes)
            .unwrap_or_default();
        NesSystem {
            cartridge_media,
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

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        NES_FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> std::time::Duration {
        std::time::Duration::from_secs_f64(NES_FRAME_TICKS as f64 / NES_PPU_DOT_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &NES_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        map_nes_key(code)
    }

    /// A hard reset drops the cartridge; re-insert it so the machine reboots.
    fn after_reset(&mut self, runtime: &mut Self::Runtime) -> Result<(), MachineError> {
        runtime.load_media(&cartridge_media_set(&self.cartridge_media))
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

/// The single-cartridge media set the NES boots from.
fn cartridge_media_set(bytes: &[u8]) -> MediaSet<'_> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, bytes));
    media
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
