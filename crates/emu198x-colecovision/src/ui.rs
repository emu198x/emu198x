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

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_coleco_colecovision::CvRuntime;

use crate::app::{ColecoVision, Region};

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

/// The ColecoVision as a [`UiSystem`] for the shared harness. The region is
/// fixed at construction; a hard reset rebuilds the machine from the BIOS and
/// cartridge the runtime already holds.
pub struct ColecoSystem {
    region: Region,
}

impl UiApp for ColecoVision {
    type System = ColecoSystem;

    fn ui_system(&self) -> ColecoSystem {
        ColecoSystem {
            region: self.region,
        }
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

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
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
    fn joystick_on_map_key_and_keypad_on_map_keys() {
        let sys = ColecoSystem {
            region: Region::Ntsc,
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
