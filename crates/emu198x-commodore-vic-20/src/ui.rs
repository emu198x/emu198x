//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Commodore VIC-20's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters, framed VIC audio, and
//! keyboard/gamepad input. The VIC-20 is keyboard-led; its two real cursor
//! keys are matrix cells, so they type, and the single joystick port is reached
//! by a real gamepad through [`UiSystem::button_map`]. Compiled only with the
//! `ui` Cargo feature; the shared launcher opens the window when no automation
//! flag is given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_commodore_vic_20::Vic20Runtime;

use crate::app::{Region, Vic20};

const DEFAULT_SCALE: u32 = 3;

/// The VIC-20's single control port: four directions plus fire, named as
/// `runtime-commodore-vic-20`'s controller mirror expects. The cursor keys are
/// keyboard cells, so a real gamepad reaches the joystick through this map.
const VIC20_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The Commodore VIC-20 as a [`UiSystem`] for the shared harness. The region is
/// fixed at construction; a hard reset rebuilds the machine from the firmware
/// the runtime already holds.
pub struct Vic20System {
    region: Region,
}

impl UiApp for Vic20 {
    type System = Vic20System;

    fn ui_system(&self) -> Vic20System {
        Vic20System {
            region: self.region,
        }
    }
}

impl UiSystem for Vic20System {
    type Runtime = Vic20Runtime;

    fn window_title(&self) -> String {
        "Emu198x Commodore VIC-20".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The VIC-20 drove a 4:3 TV; its framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((230, 288))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &VIC20_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_vic20_keys(code)
    }
}

/// Map a physical host key to its VIC-20 key name (matched by
/// `runtime-commodore-vic-20`'s `key_from_name`). The VIC-20 has only two
/// physical cursor keys (right and down — up/left are shifted), so only those
/// map; the joystick is the gamepad. Symbols that are shifted on a modern host
/// are omitted, like the other Commodore keyboards. Host Tab is RUN/STOP and
/// host Alt is the Commodore key.
fn map_vic20_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::NumpadAdd => &["+"],
        KeyCode::NumpadMultiply => &["*"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::Home => &["home"],
        KeyCode::Tab => &["stop"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["commodore"],
        KeyCode::F1 => &["f1"],
        KeyCode::F3 => &["f3"],
        KeyCode::F5 => &["f5"],
        KeyCode::F7 => &["f7"],
        // The VIC-20 has only right/down cursor keys (up/left are shifted).
        KeyCode::ArrowRight => &["crsr-right"],
        KeyCode::ArrowDown => &["crsr-down"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_maps_cursor_keys_and_specials() {
        assert_eq!(map_vic20_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_vic20_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_vic20_keys(KeyCode::Tab), Some(&["stop"][..]));
        assert_eq!(map_vic20_keys(KeyCode::AltLeft), Some(&["commodore"][..]));
        assert_eq!(
            map_vic20_keys(KeyCode::ArrowRight),
            Some(&["crsr-right"][..])
        );
        assert_eq!(map_vic20_keys(KeyCode::ArrowDown), Some(&["crsr-down"][..]));
        // Up/left have no unshifted cursor key on the VIC-20.
        assert_eq!(map_vic20_keys(KeyCode::ArrowUp), None);
    }
}
