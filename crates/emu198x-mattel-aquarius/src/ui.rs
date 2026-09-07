//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Mattel Aquarius's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and input on both of the
//! harness's paths. The Aquarius is a home computer with a hand controller, so
//! it uses the keyboard path ([`UiSystem::map_keys`]) for its 8×6 matrix *and*
//! the console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]) for the
//! Mini Expander hand controller. Arrow keys and Alt aren't on the keyboard
//! matrix, so they fall through to the joystick path without clashing. Compiled
//! only with the `ui` Cargo feature; the shared launcher opens the window
//! when no automation flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_mattel_aquarius::AquariusRuntime;

use crate::app::{Aquarius, FRAME_TICKS, REGION};

const DEFAULT_SCALE: u32 = 3;

/// Player-1 hand controller on the Mini Expander: four disc directions plus the
/// first side button, named as `runtime-mattel-aquarius`'s controller mirror
/// expects. A real gamepad reaches these through the same map.
const AQUARIUS_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The Mattel Aquarius as a [`UiSystem`] for the shared harness. Single-model;
/// a hard reset rebuilds the machine from the firmware the runtime holds. The
/// cartridge and RAM expansion are fixed at construction.
pub struct AquariusSystem;

impl UiApp for Aquarius {
    type System = AquariusSystem;

    fn ui_system(&self) -> AquariusSystem {
        AquariusSystem
    }
}

impl UiSystem for AquariusSystem {
    type Runtime = AquariusRuntime;

    fn window_title(&self) -> String {
        "Emu198x Mattel Aquarius".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Aquarius drove a 4:3 TV; its 320×192 framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((352, 232))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / REGION.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &AQUARIUS_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::AltLeft | KeyCode::AltRight => HostControl::South,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_aquarius_keys(code)
    }
}

/// Map a physical host key to its Aquarius key name (matched by
/// `runtime-mattel-aquarius`'s `key_to_matrix`). The Aquarius's symbols are
/// Shift/Ctrl-layer combos reached by holding a shift with another key, so only
/// the base keys need mapping here. Arrow and Alt keys are deliberately absent
/// — they drive the hand controller through [`UiSystem::map_key`] instead.
fn map_aquarius_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Minus => &["-"],
        KeyCode::Equal => &["="],
        KeyCode::Slash => &["/"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_and_controller_paths_do_not_clash() {
        let sys = AquariusSystem;
        // Keyboard keys go through map_keys…
        assert_eq!(sys.map_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(sys.map_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(sys.map_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        // …and are not also controller controls.
        assert_eq!(sys.map_key(KeyCode::KeyA), None);
        // Controller keys go through map_key and are not keyboard keys.
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::AltLeft), Some(HostControl::South));
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
        assert_eq!(sys.map_keys(KeyCode::AltLeft), None);
    }
}
