//! Host-key → Spectrum-key mapping for the UI mode.
//!
//! Maps physical USB keys to the Spectrum's 8×5 keyboard matrix
//! positions. Cursor keys map to Caps Shift + 5/6/7/8 — the exact
//! membrane wiring on every Spectrum from the Spectrum+ (1984)
//! onwards. The 16K and pre-1984 48K never had labelled cursor keys
//! but the matrix combo still works (the user just had to press both
//! keys explicitly on the rubber keyboard).

use emu198x_shell::InputEvent;
use winit::keyboard::KeyCode;

/// Wraps a Spectrum key name in an `InputEvent` for the runtime.
pub fn spectrum_key_event(name: &'static str, pressed: bool) -> InputEvent {
    InputEvent::Key {
        name: name.into(),
        pressed,
    }
}

/// Maps one physical host key to one or more Spectrum-matrix key
/// names. Returning multiple names is how host keys produce
/// hardware-correct combos: arrows close two matrix positions
/// simultaneously (Caps Shift + 5/6/7/8) the same way a Spectrum+
/// membrane does.
pub fn map_spectrum_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Space => &["space"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["caps"],
        KeyCode::AltLeft | KeyCode::AltRight => &["symbol"],
        // The Spectrum+ (1984) and every later model — 128K, +2, +2A,
        // +2B, +3 — have full-stroke keyboards with labelled cursor
        // keys whose membrane wiring closes the exact same matrix
        // contacts as Caps Shift + 5/6/7/8. The ROM never sees a
        // dedicated "cursor" scancode; it sees Caps held and a number
        // key pressed. So this *is* the hardware mapping, not a
        // synthesis. Boot menus on the 128K-family rely on it; games
        // that read the matrix directly see exactly what the real
        // hardware would send.
        KeyCode::ArrowLeft => &["caps", "5"],
        KeyCode::ArrowDown => &["caps", "6"],
        KeyCode::ArrowUp => &["caps", "7"],
        KeyCode::ArrowRight => &["caps", "8"],
        KeyCode::Backspace => &["caps", "0"],
        KeyCode::Quote => &["symbol", "p"],
        _ => return None,
    })
}

/// Cycles a host audio gain value through 1.0 → 0.5 → 0.25 → 0.0 → 1.0.
/// Used by the Numpad-2 audio-toggle shortcut.
pub fn next_audio_gain(gain: f32) -> f32 {
    if gain > 0.75 {
        0.5
    } else if gain > 0.375 {
        0.25
    } else if gain > 0.0 {
        0.0
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_keys_map_to_caps_shift_combos() {
        // Matches Spectrum+ / 128K-family hardware: physical cursor
        // keys are membrane-wired as Caps Shift + 5/6/7/8.
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowLeft),
            Some(&["caps", "5"][..])
        );
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowDown),
            Some(&["caps", "6"][..])
        );
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowUp),
            Some(&["caps", "7"][..])
        );
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowRight),
            Some(&["caps", "8"][..])
        );
        assert_eq!(map_spectrum_keys(KeyCode::AltLeft), Some(&["symbol"][..]));
    }

    #[test]
    fn enter_maps_from_main_and_keypad_return() {
        assert_eq!(map_spectrum_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(
            map_spectrum_keys(KeyCode::NumpadEnter),
            Some(&["enter"][..])
        );
    }

    #[test]
    fn audio_gain_cycles_through_debug_levels() {
        assert_eq!(next_audio_gain(1.0), 0.5);
        assert_eq!(next_audio_gain(0.5), 0.25);
        assert_eq!(next_audio_gain(0.25), 0.0);
        assert_eq!(next_audio_gain(0.0), 1.0);
    }
}
