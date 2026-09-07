//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Spectravideo SVI-328's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and the full keyboard
//! routed through the harness's general-keyboard path ([`UiSystem::map_keys`]).
//! The SVI-328 is keyboard-led; its cursor keys are genuine matrix cells, so
//! they type rather than driving the stick. The joystick is reached by a real
//! gamepad through [`UiSystem::button_map`]. Compiled only with the `ui`
//! Cargo feature; the shared launcher opens the window when no automation
//! flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_spectravideo_svi_328::Svi328Runtime;

use crate::app::{Region, Svi328};

const DEFAULT_SCALE: u32 = 3;
/// The SVI-328's TMS9918 framebuffer (active + border), fixed by the VDP.
const FB_WIDTH: u32 = 288;
const FB_HEIGHT: u32 = 240;

/// Player-1 joystick: four directions plus fire, named as
/// `runtime-spectravideo-svi-328`'s controller mirror expects. The cursor keys
/// are keyboard cells, so a real gamepad reaches the stick through this map.
const SVI_328_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The Spectravideo SVI-328 as a [`UiSystem`] for the shared harness. The
/// region is fixed at construction; a hard reset rebuilds the machine from the
/// firmware the runtime already holds.
pub struct Svi328System {
    region: Region,
}

impl UiApp for Svi328 {
    type System = Svi328System;

    fn ui_system(&self) -> Svi328System {
        Svi328System {
            region: self.region,
        }
    }
}

impl UiSystem for Svi328System {
    type Runtime = Svi328Runtime;

    fn window_title(&self) -> String {
        "Emu198x Spectravideo SVI-328".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The SVI-328's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches
    // to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SVI_328_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_svi_keys(code)
    }
}

/// Map a physical host key to its SVI-328 key name (matched by
/// `runtime-spectravideo-svi-328`'s `key_to_matrix`). The cursor keys are
/// genuine matrix cells, so they map here rather than to the joystick. Shifted
/// symbols are reached by holding SHIFT; host Alt is the GRAPH/CODE key. The
/// SVI's own Escape key is unreachable — the harness owns Esc for quit.
fn map_svi_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &["'"],
        KeyCode::Comma => &[","],
        KeyCode::Equal => &["="],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Minus => &["-"],
        KeyCode::BracketLeft => &["["],
        KeyCode::Backslash => &["\\"],
        KeyCode::BracketRight => &["]"],
        KeyCode::Space => &["space"],
        KeyCode::Tab => &["tab"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Delete => &["delete"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["graph"],
        KeyCode::CapsLock => &["caps"],
        KeyCode::Home => &["home"],
        KeyCode::Insert => &["insert"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_keys_are_keyboard_cells_and_graph_maps() {
        // The SVI's cursor keys are genuine matrix cells, so they type.
        assert_eq!(map_svi_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_svi_keys(KeyCode::ArrowRight), Some(&["right"][..]));
        assert_eq!(map_svi_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_svi_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_svi_keys(KeyCode::AltLeft), Some(&["graph"][..]));
        assert_eq!(map_svi_keys(KeyCode::F1), Some(&["f1"][..]));
        // Keys with no SVI position are ignored.
        assert_eq!(map_svi_keys(KeyCode::PageUp), None);
    }
}
