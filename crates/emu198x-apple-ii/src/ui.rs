//! Board-specific presentation and key names; the shared harness owns the host.
use std::time::Duration;

use emu198x_runtime_apple_ii::{AppleIIRuntime, profiles};
use emu198x_shell::MachineCore;
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, KeyCode, KeywordModifier, UiSystem};

use crate::app::AppleII;

pub struct AppleIISystem;
const BUTTONS: ButtonInputMap = ButtonInputMap::new(&[]);

impl UiApp for AppleII {
    type System = AppleIISystem;
    fn ui_system(&self) -> AppleIISystem {
        AppleIISystem
    }
}

impl UiSystem for AppleIISystem {
    type Runtime = AppleIIRuntime;

    fn window_title(&self) -> String {
        "Emu198x Apple II — original NTSC revision 0".into()
    }
    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (560, 192)
    }
    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        profiles::FRAME_TICKS
    }
    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        let rate = runtime.profile().clock.rate;
        Duration::from_secs_f64(
            self.frame_ticks(runtime) as f64 * rate.denominator_hz as f64
                / rate.numerator_hz as f64,
        )
    }
    fn button_map(&self) -> &'static ButtonInputMap {
        &BUTTONS
    }
    fn supports_host_keyboard(&self) -> bool {
        true
    }
    fn host_character_keys(&self, runtime: &Self::Runtime, ch: char) -> Option<Vec<String>> {
        runtime.keyboard_target()?.keys_for_char(ch)
    }
    fn host_keyword_modifier(&self) -> Option<KeywordModifier> {
        Some(KeywordModifier {
            key: KeyCode::AltLeft,
            keys: &["ctrl"],
            shifted_keys: &["ctrl"],
        })
    }
    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // Used for deliberate Ctrl chords and the shared physical-key mode.
        // Shift is encoded in host characters, never sent as a target contact.
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
            KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
            KeyCode::Escape => &["escape"],
            KeyCode::Backspace | KeyCode::ArrowLeft => &["left"],
            KeyCode::ArrowRight => &["right"],
            KeyCode::Space => &["space"],
            KeyCode::BracketLeft => &["["],
            KeyCode::BracketRight => &["]"],
            KeyCode::Backslash => &["\\"],
            KeyCode::Semicolon => &[";"],
            KeyCode::Quote => &["'"],
            KeyCode::Comma => &[","],
            KeyCode::Period => &["."],
            KeyCode::Slash => &["/"],
            KeyCode::Minus => &["-"],
            KeyCode::Equal => &["="],
            KeyCode::Backquote => &["`"],
            KeyCode::AltLeft => &["ctrl"],
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests;
