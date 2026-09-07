//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari 5200's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed POKEY audio, and
//! keyboard/gamepad input. The 5200 controller is an analogue stick + fire +
//! a 16-key keypad. The stick and fire go through the harness's console path
//! ([`UiSystem::map_key`] + [`UiSystem::button_map`]) — the runtime snaps the
//! digital directions to the POKEY pot extremes — and the keypad keys
//! (`start`/`pause`/`reset`/`0`-`9`/`*`/`#`) are momentary named key events,
//! routed through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo
//! feature; the shared launcher opens the window when no automation flag is
//! given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_atari_5200::Atari5200Runtime;

use crate::app::{Atari5200, Region};

const DEFAULT_SCALE: u32 = 3;

/// Player-1 controller: stick directions plus fire. The runtime snaps the
/// digital directions to the analogue pot extremes; `fire` drives the trigger.
/// A real gamepad reaches these through the same map; the keyboard does via
/// [`UiSystem::map_key`].
const ATARI_5200_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The Atari 5200 as a [`UiSystem`] for the shared harness. The region is fixed
/// at construction; a hard reset rebuilds the machine from the cartridge and
/// BIOS the runtime already holds.
pub struct Atari5200System {
    region: Region,
}

impl UiApp for Atari5200 {
    type System = Atari5200System;

    fn ui_system(&self) -> Atari5200System {
        Atari5200System {
            region: self.region,
        }
    }
}

impl UiSystem for Atari5200System {
    type Runtime = Atari5200Runtime;

    fn window_title(&self) -> String {
        "Emu198x Atari 5200".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 5200 drove a 4:3 TV; its GTIA framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((374, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ATARI_5200_BUTTON_MAP
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
        // The 16-key keypad — momentary named key events. The three console
        // keys (Start / Pause / Reset) are keypad keys on the 5200, distinct
        // from the harness's own Esc-quit / F12-reset.
        Some(match code {
            KeyCode::Enter | KeyCode::NumpadEnter => &["start"],
            KeyCode::Backspace => &["pause"],
            KeyCode::Delete => &["reset"],
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
    fn stick_on_map_key_and_keypad_on_map_keys() {
        let sys = Atari5200System {
            region: Region::Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["start"][..]));
        assert_eq!(sys.map_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(sys.map_keys(KeyCode::NumpadMultiply), Some(&["*"][..]));
        // No double-routing: stick keys aren't keypad keys and vice versa.
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
        assert_eq!(sys.map_key(KeyCode::Digit5), None);
    }
}
