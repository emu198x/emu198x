//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Commodore PET's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters and the keyboard routed through
//! the harness's general-keyboard path ([`UiSystem::map_keys`]). The PET is
//! keyboard-only — no joystick, no sound — so it carries an empty button map
//! and routes every key through `map_keys`. Many PET symbols sit on dedicated
//! keys that are shifted on a modern host, so only the physically-unshifted
//! keys are mapped. Compiled only with the `ui` Cargo feature; the shared
//! launcher opens the window when no automation flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, KeyCode, UiSystem};
use runtime_commodore_pet::PetRuntime;

use crate::app::{CommodorePet, FRAME_TICKS};

const DEFAULT_SCALE: u32 = 3;
const FRAME_HZ: f64 = 50.0;

/// The PET has no joystick, but the harness still wants a button map — so an
/// empty one. Every key flows through [`UiSystem::map_keys`] instead.
const PET_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[]);

/// The Commodore PET as a [`UiSystem`] for the shared harness. Keyboard-only; a
/// hard reset rebuilds the machine from the firmware the runtime already holds.
/// The column model is fixed at construction.
pub struct PetSystem;

impl UiApp for CommodorePet {
    type System = PetSystem;

    fn ui_system(&self) -> PetSystem {
        PetSystem
    }
}

impl UiSystem for PetSystem {
    type Runtime = PetRuntime;

    fn window_title(&self) -> String {
        "Emu198x Commodore PET".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The PET drove a 4:3 monochrome monitor; its framebuffer stretches to fill
    // it.
    //
    // Deliberately still on this hook rather than `pixel_aspect_ratio`, and the
    // only core that should be. The raster derivation asks how much of a
    // *broadcast* line a set displays, and a set overscans; a dedicated monitor
    // shows the whole framebuffer, so "stretch this buffer to fill 4:3" is not
    // a legacy approximation here but the correct model. The PET's profile says
    // `Region::Other` for the same reason, and the derivation would decline to
    // answer. See `knowledge/decisions/pixel-aspect-comes-from-the-raster.md`.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((384, 248))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &PET_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_pet_keys(code)
    }
}

/// Map a physical host key to its PET key name (matched by
/// `runtime-commodore-pet`'s `key_from_name`). The PET places many symbols on
/// dedicated keys that are shifted on a modern keyboard; without shift-symbol
/// synthesis only the physically-unshifted keys are mapped here — letters,
/// digits, the directly-typable punctuation, RETURN, space, and cursor-right.
fn map_pet_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Period => &["."],
        KeyCode::Comma => &[","],
        KeyCode::Semicolon => &[";"],
        KeyCode::Slash => &["/"],
        KeyCode::Quote => &["'"],
        KeyCode::Equal => &["="],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::ArrowRight => &["right"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_digits_and_return() {
        assert_eq!(map_pet_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_pet_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_pet_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_pet_keys(KeyCode::ArrowRight), Some(&["right"][..]));
        // Keys with no directly-typable PET position are ignored.
        assert_eq!(map_pet_keys(KeyCode::Tab), None);
    }
}
