//! Interactive UI mode — the default when no automation flag is present.
//!
//! The MSX1's first native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, framed PSG audio, and keyboard/gamepad
//! input. The MSX is keyboard-led; its cursor keys are genuine matrix cells, so
//! they type rather than driving the stick, and the joystick is reached by a
//! real gamepad through [`UiSystem::button_map`]. Compiled only with the `ui`
//! Cargo feature; the shared launcher opens the window when no automation
//! flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_msx::MsxRuntime;

use crate::app::{Msx, Region};

const DEFAULT_SCALE: u32 = 3;

/// Player-1 joystick: four directions plus the trigger-A button, named as
/// `runtime-msx`'s controller mirror expects. The cursor keys are keyboard
/// cells, so a real gamepad reaches the stick through this map.
const MSX_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The MSX1 as a [`UiSystem`] for the shared harness. The region is fixed at
/// construction; a hard reset rebuilds the machine from the firmware and
/// cartridge the runtime already holds.
pub struct MsxSystem {
    region: Region,
}

impl UiApp for Msx {
    type System = MsxSystem;

    fn ui_system(&self) -> MsxSystem {
        MsxSystem {
            region: self.region,
        }
    }
}

impl UiSystem for MsxSystem {
    type Runtime = MsxRuntime;

    fn window_title(&self) -> String {
        "Emu198x MSX".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The MSX's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches to
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
        &MSX_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_msx_keys(code)
    }
}

/// Map a physical host key to its MSX key name (matched by `runtime-msx`'s
/// `key_to_matrix`). The cursor keys are genuine matrix cells, so they map here
/// rather than to the joystick. Shifted symbols are reached by holding SHIFT;
/// host Alt is the GRAPH key. The MSX's own Escape key is unreachable — the
/// harness owns Esc for quit.
fn map_msx_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Backslash => &["\\"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &["'"],
        KeyCode::Backquote => &["`"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Tab => &["tab"],
        KeyCode::Backspace => &["bs"],
        KeyCode::Delete => &["delete"],
        KeyCode::Insert => &["insert"],
        KeyCode::Home => &["home"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["graph"],
        KeyCode::CapsLock => &["caps"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_keys_are_keyboard_cells_and_graph_maps() {
        assert_eq!(map_msx_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_msx_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_msx_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_msx_keys(KeyCode::AltLeft), Some(&["graph"][..]));
        assert_eq!(map_msx_keys(KeyCode::F1), Some(&["f1"][..]));
        // Keys with no MSX position are ignored.
        assert_eq!(map_msx_keys(KeyCode::PageUp), None);
    }
}
