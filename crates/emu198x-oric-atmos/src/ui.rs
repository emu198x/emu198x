//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Oric-1 / Atmos's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and the full 8×8 keyboard
//! routed through the harness's general-keyboard path ([`UiSystem::map_keys`]).
//! The Oric is keyboard-led; its IJK joystick is an add-on reached by a real
//! gamepad through [`UiSystem::button_map`] (the harness drains gamepad events
//! through the button map regardless of the keyboard path). The cursor keys are
//! genuine keyboard cells on the Oric, so they type — they don't drive the
//! stick. Compiled only with the `ui` Cargo feature; the shared launcher opens
//! the window when no automation flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use machine_oric_atmos::{FB_HEIGHT, FB_WIDTH};
use runtime_oric_atmos::OricRuntime;

use crate::app::{FRAME_TICKS, Oric};

const DEFAULT_SCALE: u32 = 3;
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 IJK (left) stick: four directions plus fire, named as
/// `runtime-oric-atmos`'s controller mirror expects. A real gamepad reaches
/// these through the button map; keyboard cursor keys deliberately do not, so
/// they keep their Oric keyboard meaning.
const ORIC_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The Oric-1 / Atmos as a [`UiSystem`] for the shared harness. The model is
/// fixed at construction; a hard reset rebuilds the machine from the firmware
/// the runtime already holds.
pub struct OricSystem;

impl UiApp for Oric {
    type System = OricSystem;

    fn ui_system(&self) -> OricSystem {
        OricSystem
    }
}

impl UiSystem for OricSystem {
    type Runtime = OricRuntime;

    fn window_title(&self) -> String {
        "Emu198x Oric Atmos".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Oric drove a 4:3 TV; its 240×224 framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ORIC_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_oric_keys(code)
    }
}

/// Map a physical host key to its Oric key name (matched by
/// `runtime-oric-atmos`'s `key_to_matrix`). The cursor keys are real Oric
/// keyboard cells, so they map here rather than to the joystick. Shifted
/// symbols are reached by holding a shift with another key, so only the
/// unshifted legends need mapping.
fn map_oric_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Semicolon => &[";"],
        KeyCode::Minus => &["-"],
        KeyCode::Quote => &["'"],
        KeyCode::Backslash => &["\\"],
        KeyCode::Slash => &["/"],
        KeyCode::Equal => &["="],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
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
    fn cursor_keys_are_keyboard_cells_not_joystick() {
        // The Oric's cursor keys are genuine keyboard cells, so they go
        // through the keyboard path and keep their Oric names.
        assert_eq!(map_oric_keys(KeyCode::ArrowLeft), Some(&["left"][..]));
        assert_eq!(map_oric_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_oric_keys(KeyCode::KeyH), Some(&["h"][..]));
        assert_eq!(map_oric_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_oric_keys(KeyCode::ShiftRight), Some(&["rshift"][..]));
        assert_eq!(map_oric_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        // Keys with no Oric position are ignored.
        assert_eq!(map_oric_keys(KeyCode::Tab), None);
    }
}
