//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Jupiter Ace's native window on the shared `emu198x-ui` harness. The
//! Ace is keyboard-only: no joystick port, so the button map is empty and
//! every key goes through [`UiSystem::map_keys`]. Compiled only with the
//! `ui` Cargo feature; the shared launcher opens the window when no
//! automation flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, KeyCode, UiSystem};
use runtime_jupiter_ace::JupiterAceRuntime;

use crate::app::{FRAME_TICKS, JupiterAce};

const DEFAULT_SCALE: u32 = 3;
const PAL_FRAME_HZ: f64 = 50.0;
const ACE_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

pub struct JupiterAceSystem;

impl UiApp for JupiterAce {
    type System = JupiterAceSystem;

    fn ui_system(&self) -> JupiterAceSystem {
        JupiterAceSystem
    }
}

impl UiSystem for JupiterAceSystem {
    type Runtime = JupiterAceRuntime;

    fn window_title(&self) -> String {
        "Emu198x Jupiter Ace".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((320, 288))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ACE_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_ace_keys(code)
    }
}

fn map_ace_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["symbol"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys_and_both_shifts() {
        assert_eq!(map_ace_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_ace_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_ace_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_ace_keys(KeyCode::Space), Some(&["space"][..]));
        assert_eq!(map_ace_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(map_ace_keys(KeyCode::ControlLeft), Some(&["symbol"][..]));
        // Keys with no Ace position are ignored.
        assert_eq!(map_ace_keys(KeyCode::Tab), None);
    }
}
