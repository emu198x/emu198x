//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Acorn Atom's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the keyboard routed through
//! the harness's general-keyboard path ([`UiSystem::map_keys`]). The Atom is
//! keyboard-only — no joystick or mouse — so it carries an empty button map and
//! routes every key through `map_keys`. Compiled only with the `ui` Cargo
//! feature; the shared launcher opens the window when no automation flag is
//! given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, KeyCode, UiSystem};
use runtime_acorn_atom::AtomRuntime;

use crate::app::{Atom, FRAME_TICKS};

const DEFAULT_SCALE: u32 = 3;
const FRAME_HZ: f64 = 50.0;

/// The Atom has no joystick, but the harness still wants a button map — so an
/// empty one. Every key flows through [`UiSystem::map_keys`] instead.
const ATOM_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

/// The Acorn Atom as a [`UiSystem`] for the shared harness. Keyboard-only; a
/// hard reset rebuilds the machine from the firmware the runtime already holds.
/// The RAM size is fixed at construction.
pub struct AtomSystem;

impl UiApp for Atom {
    type System = AtomSystem;

    fn ui_system(&self) -> AtomSystem {
        AtomSystem
    }
}

impl UiSystem for AtomSystem {
    type Runtime = AtomRuntime;

    fn window_title(&self) -> String {
        "Emu198x Acorn Atom".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Atom's MC6847 drove a 4:3 TV; its framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((372, 288))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ATOM_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_atom_keys(code)
    }
}

/// Map a physical host key to its Atom key name (matched by
/// `runtime-acorn-atom`'s `key_from_name`). The Atom's keyboard is uppercase,
/// so the unshifted letter and digit keys cover ordinary typing; only the keys
/// the runtime scans are mapped here.
fn map_atom_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Quote => &["@"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_keys() {
        assert_eq!(map_atom_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_atom_keys(KeyCode::Digit3), Some(&["3"][..]));
        assert_eq!(map_atom_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_atom_keys(KeyCode::Quote), Some(&["@"][..]));
        // Keys the Atom runtime doesn't scan are ignored.
        assert_eq!(map_atom_keys(KeyCode::Tab), None);
    }
}
