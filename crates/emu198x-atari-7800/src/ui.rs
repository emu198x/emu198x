//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari 7800's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed TIA audio, and
//! keyboard/gamepad input. The 7800 pad is a digital joystick + two fire
//! buttons — the harness's console path ([`UiSystem::map_key`] +
//! [`UiSystem::button_map`]) — plus the three console switches (Reset / Select
//! / Pause), which the runtime takes as named key events, routed through
//! [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo feature; the
//! shared launcher opens the window when no automation flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_atari_7800::Atari7800Runtime;

use crate::app::{Atari7800, Region};

const DEFAULT_SCALE: u32 = 3;

/// Player-1 control: joystick directions, the two fire buttons, and the two
/// gamepad menu buttons mapped to the console Select / Reset switches. The
/// names are the ones `runtime-atari-7800`'s `set_control` understands.
const ATARI_7800_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire2")),
    (HostControl::Start, ButtonTarget::new(1, "select")),
    (HostControl::Select, ButtonTarget::new(1, "reset")),
]);

/// The Atari 7800 as a [`UiSystem`] for the shared harness. The region is fixed
/// at construction; a hard reset rebuilds the machine from the cartridge the
/// runtime already holds.
pub struct Atari7800System {
    region: Region,
}

impl UiApp for Atari7800 {
    type System = Atari7800System;

    fn ui_system(&self) -> Atari7800System {
        Atari7800System {
            region: self.region,
        }
    }
}

impl UiSystem for Atari7800System {
    type Runtime = Atari7800Runtime;

    fn window_title(&self) -> String {
        "Emu198x Atari 7800".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The 7800 drove a 4:3 TV; its MARIA framebuffer stretches to fill it.

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
        &ATARI_7800_BUTTON_MAP
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
        // The three console switches — named key events, distinct from the
        // harness's own Esc-quit / F12-reset.
        Some(match code {
            KeyCode::Enter | KeyCode::NumpadEnter => &["select"],
            KeyCode::Backspace => &["reset"],
            KeyCode::Delete => &["pause"],
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_on_map_key_and_console_switches_on_map_keys() {
        let sys = Atari7800System {
            region: Region::Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["select"][..]));
        assert_eq!(sys.map_keys(KeyCode::Backspace), Some(&["reset"][..]));
        assert_eq!(sys.map_keys(KeyCode::Delete), Some(&["pause"][..]));
        // No double-routing.
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
        assert_eq!(sys.map_key(KeyCode::Enter), None);
    }
}
