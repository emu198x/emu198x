//! Interactive UI mode — the default when no automation flag is present.
//!
//! A native Game Boy window built on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, framed APU audio, and keyboard/gamepad
//! joypad input. Compiled only with the `ui` Cargo feature; the shared
//! launcher opens the window when no automation flag is given.
//!
//! Beyond the harness defaults the Game Boy adds per-system shortcuts (the
//! `0`-`8` APU channel debug controls) via [`UiSystem::handle_key`], and a
//! teardown that flushes the cartridge save image (RAM + RTC footer) to its
//! `.sav` sidecar via [`UiSystem::on_exit`].

use std::path::PathBuf;

use common_nintendo_game_boy::timing::MCYCLE_HZ;
use common_nintendo_game_boy::{MCYCLES_PER_FRAME, SCREEN_HEIGHT, SCREEN_WIDTH};
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_nintendo_game_boy::{ApuChannel, AudioControls, GameBoyRuntime};

use crate::app::{GameBoy, resolve_battery_save_path, write_battery_save};

const DEFAULT_SCALE: u32 = 4;
const INPUT_SLICES_PER_FRAME: u32 = 4;

const GAME_BOY_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
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

/// The Game Boy as a [`UiSystem`] for the shared harness. A hard reset keeps
/// the cartridge in the runtime, so the only state it carries is the
/// battery-save path (flushed on exit).
pub struct GameBoySystem {
    battery_save_path: Option<PathBuf>,
}

impl UiApp for GameBoy {
    type System = GameBoySystem;

    fn ui_system(&self) -> GameBoySystem {
        GameBoySystem {
            battery_save_path: resolve_battery_save_path(self),
        }
    }
}

impl UiSystem for GameBoySystem {
    type Runtime = GameBoyRuntime;

    fn window_title(&self) -> String {
        "Emu198x Game Boy".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The runtime honours sub-frame targets, so finer slices cut input latency.
    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (SCREEN_WIDTH, SCREEN_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        u64::from(MCYCLES_PER_FRAME)
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> std::time::Duration {
        std::time::Duration::from_secs_f64(f64::from(MCYCLES_PER_FRAME) / f64::from(MCYCLE_HZ))
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &GAME_BOY_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        map_game_boy_key(code)
    }

    /// The `0`-`8` digit row drives the APU debug controls; consume those keys
    /// so they aren't treated as joypad buttons.
    fn handle_key(&mut self, runtime: &mut Self::Runtime, code: KeyCode, pressed: bool) -> bool {
        let action = match code {
            KeyCode::Digit0 => AudioShortcut::Reset,
            KeyCode::Digit1 => AudioShortcut::Toggle(ApuChannel::Pulse1),
            KeyCode::Digit2 => AudioShortcut::Toggle(ApuChannel::Pulse2),
            KeyCode::Digit3 => AudioShortcut::Toggle(ApuChannel::Wave),
            KeyCode::Digit4 => AudioShortcut::Toggle(ApuChannel::Noise),
            KeyCode::Digit5 => AudioShortcut::Gain(ApuChannel::Pulse1),
            KeyCode::Digit6 => AudioShortcut::Gain(ApuChannel::Pulse2),
            KeyCode::Digit7 => AudioShortcut::Gain(ApuChannel::Wave),
            KeyCode::Digit8 => AudioShortcut::Gain(ApuChannel::Noise),
            _ => return false,
        };
        if pressed {
            action.apply(runtime);
        }
        true
    }

    /// Persist the cartridge save image (RAM + RTC footer) to its `.sav` on the
    /// way out.
    fn on_exit(&mut self, runtime: &mut Self::Runtime) -> Result<(), String> {
        match &self.battery_save_path {
            Some(path) => write_battery_save(runtime, path),
            None => Ok(()),
        }
    }
}

/// An APU debug shortcut: reset all controls, mute/unmute a channel, or cycle
/// its gain.
enum AudioShortcut {
    Reset,
    Toggle(ApuChannel),
    Gain(ApuChannel),
}

impl AudioShortcut {
    fn apply(self, runtime: &mut GameBoyRuntime) {
        match self {
            Self::Reset => {
                runtime.set_audio_controls(AudioControls::default());
                eprintln!("audio: reset channel controls");
            }
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

fn map_game_boy_key(code: KeyCode) -> Option<HostControl> {
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
    fn maps_controls_to_joypad_buttons() {
        assert_eq!(map_game_boy_key(KeyCode::KeyX), Some(HostControl::South));
        assert_eq!(map_game_boy_key(KeyCode::KeyZ), Some(HostControl::East));
        assert_eq!(map_game_boy_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(
            map_game_boy_key(KeyCode::ArrowLeft),
            Some(HostControl::Left)
        );
        assert_eq!(map_game_boy_key(KeyCode::Digit1), None);
    }

    #[test]
    fn audio_gain_shortcut_cycles_down_then_restores() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}
