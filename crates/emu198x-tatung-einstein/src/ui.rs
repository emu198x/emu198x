//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Tatung Einstein's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters and the keyboard routed
//! through the harness's general-keyboard path ([`UiSystem::map_keys`]). The
//! Einstein is keyboard-led; its joysticks are analogue (pot-per-axis) and are
//! reached by a real gamepad through [`UiSystem::button_map`] (the harness
//! drains gamepad events through the button map, which the runtime snaps to the
//! pot extremes). Compiled only with the `ui` Cargo feature; the shared
//! launcher opens the window when no automation flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_tatung_einstein::EinsteinRuntime;

use crate::app::{Einstein, FRAME_TICKS_PAL};

const DEFAULT_SCALE: u32 = 3;
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
const PAL_FRAME_HZ: f64 = 50.0;

/// Player-1 joystick: four directions plus fire, named as
/// `runtime-tatung-einstein`'s controller mirror expects (the digital
/// directions snap the analogue pots to their extremes). A real gamepad reaches
/// these through the button map.
const EINSTEIN_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The Tatung Einstein as a [`UiSystem`] for the shared harness. Single-model;
/// a hard reset rebuilds the machine from the firmware the runtime holds.
pub struct EinsteinSystem;

impl UiApp for Einstein {
    type System = EinsteinSystem;

    fn ui_system(&self) -> EinsteinSystem {
        EinsteinSystem
    }
}

impl UiSystem for EinsteinSystem {
    type Runtime = EinsteinRuntime;

    fn window_title(&self) -> String {
        "Emu198x Tatung Einstein".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Einstein's TMS9929 drove a 4:3 TV; its 288×240 framebuffer stretches
    // to fill it.

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
            .unwrap_or((278, 288))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        FRAME_TICKS_PAL
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &EINSTEIN_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_einstein_keys(code)
    }
}

/// Map a physical host key to its Einstein key name (matched by
/// `runtime-tatung-einstein`'s `key_to_matrix` / `key_to_modifier`). SHIFT and
/// CONTROL are status-port modifiers, not matrix cells, but the runtime routes
/// them by name so they map here too. Shifted symbols are reached by holding a
/// shift with another key, so only the unshifted legends need mapping. The
/// Einstein's own Escape key is unreachable — the harness owns Esc for quit.
fn map_einstein_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::Equal => &["="],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["graph"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys_modifiers_and_graph() {
        assert_eq!(map_einstein_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_einstein_keys(KeyCode::Digit5), Some(&["5"][..]));
        assert_eq!(map_einstein_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_einstein_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
        assert_eq!(map_einstein_keys(KeyCode::ControlLeft), Some(&["ctrl"][..]));
        assert_eq!(map_einstein_keys(KeyCode::AltLeft), Some(&["graph"][..]));
        // Keys with no Einstein position are ignored.
        assert_eq!(map_einstein_keys(KeyCode::Tab), None);
    }
}
