//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Sord M5's native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters and the keyboard routed through the
//! harness's general-keyboard path ([`UiSystem::map_keys`]). The M5 is
//! keyboard-led; its joystick carries only the four directions (the action
//! buttons are keyboard keys), reached by a real gamepad through
//! [`UiSystem::button_map`]. Compiled only with the `ui` Cargo feature; the
//! shared launcher opens the window when no automation flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_sord_m5::M5Runtime;

use crate::app::{Region, SordM5};

const DEFAULT_SCALE: u32 = 3;

/// Player-1 joystick: four directions only — the M5 has no joystick fire line
/// (action buttons are keyboard keys), so the button map carries no fire. A
/// real gamepad reaches the directions through this map.
const SORD_M5_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
]);

/// The Sord M5 as a [`UiSystem`] for the shared harness. The region is fixed at
/// construction; a hard reset rebuilds the machine from the firmware the
/// runtime already holds.
pub struct SordM5System {
    region: Region,
}

impl UiApp for SordM5 {
    type System = SordM5System;

    fn ui_system(&self) -> SordM5System {
        SordM5System {
            region: self.region,
        }
    }
}

impl UiSystem for SordM5System {
    type Runtime = M5Runtime;

    fn window_title(&self) -> String {
        "Emu198x Sord M5".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The M5's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
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
        &SORD_M5_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_m5_keys(code)
    }
}

/// Map a physical host key to its M5 key name (matched by `runtime-sord-m5`'s
/// `key_to_matrix`). The M5 has no cursor keys on the keyboard — those are the
/// gamepad joystick — so only the matrix keys map here. Shifted symbols are
/// reached by holding SHIFT; host Tab is the M5's FUNC key.
fn map_m5_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Minus => &["-"],
        KeyCode::Equal => &["="],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Semicolon => &[":"],
        KeyCode::Quote => &["'"],
        KeyCode::Backslash => &["\\"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Backspace | KeyCode::Delete => &["backspace"],
        KeyCode::Tab => &["func"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys_func_and_shifts() {
        assert_eq!(map_m5_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_m5_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_m5_keys(KeyCode::Tab), Some(&["func"][..]));
        assert_eq!(map_m5_keys(KeyCode::ShiftRight), Some(&["rshift"][..]));
        assert_eq!(map_m5_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        // The M5 has no keyboard cursor keys — those are the gamepad joystick.
        assert_eq!(map_m5_keys(KeyCode::ArrowUp), None);
    }
}
