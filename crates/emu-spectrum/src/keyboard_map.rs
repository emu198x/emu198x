//! Host keyboard → Spectrum key mapping.
//!
//! Maps winit `KeyCode` values to `SpectrumKey` for the windowed binary.

use winit::keyboard::KeyCode;

use crate::input::SpectrumKey;

/// Map a host key to a Spectrum key.
///
/// Returns `None` for unmapped keys.
#[must_use]
pub fn map_keycode(key: KeyCode) -> Option<SpectrumKey> {
    match key {
        // Letters
        KeyCode::KeyA => Some(SpectrumKey::A),
        KeyCode::KeyB => Some(SpectrumKey::B),
        KeyCode::KeyC => Some(SpectrumKey::C),
        KeyCode::KeyD => Some(SpectrumKey::D),
        KeyCode::KeyE => Some(SpectrumKey::E),
        KeyCode::KeyF => Some(SpectrumKey::F),
        KeyCode::KeyG => Some(SpectrumKey::G),
        KeyCode::KeyH => Some(SpectrumKey::H),
        KeyCode::KeyI => Some(SpectrumKey::I),
        KeyCode::KeyJ => Some(SpectrumKey::J),
        KeyCode::KeyK => Some(SpectrumKey::K),
        KeyCode::KeyL => Some(SpectrumKey::L),
        KeyCode::KeyM => Some(SpectrumKey::M),
        KeyCode::KeyN => Some(SpectrumKey::N),
        KeyCode::KeyO => Some(SpectrumKey::O),
        KeyCode::KeyP => Some(SpectrumKey::P),
        KeyCode::KeyQ => Some(SpectrumKey::Q),
        KeyCode::KeyR => Some(SpectrumKey::R),
        KeyCode::KeyS => Some(SpectrumKey::S),
        KeyCode::KeyT => Some(SpectrumKey::T),
        KeyCode::KeyU => Some(SpectrumKey::U),
        KeyCode::KeyV => Some(SpectrumKey::V),
        KeyCode::KeyW => Some(SpectrumKey::W),
        KeyCode::KeyX => Some(SpectrumKey::X),
        KeyCode::KeyY => Some(SpectrumKey::Y),
        KeyCode::KeyZ => Some(SpectrumKey::Z),

        // Digits
        KeyCode::Digit0 => Some(SpectrumKey::N0),
        KeyCode::Digit1 => Some(SpectrumKey::N1),
        KeyCode::Digit2 => Some(SpectrumKey::N2),
        KeyCode::Digit3 => Some(SpectrumKey::N3),
        KeyCode::Digit4 => Some(SpectrumKey::N4),
        KeyCode::Digit5 => Some(SpectrumKey::N5),
        KeyCode::Digit6 => Some(SpectrumKey::N6),
        KeyCode::Digit7 => Some(SpectrumKey::N7),
        KeyCode::Digit8 => Some(SpectrumKey::N8),
        KeyCode::Digit9 => Some(SpectrumKey::N9),

        // Modifiers
        KeyCode::ShiftLeft => Some(SpectrumKey::CapsShift),
        KeyCode::ShiftRight | KeyCode::ControlLeft | KeyCode::ControlRight => {
            Some(SpectrumKey::SymShift)
        }

        // Special keys
        KeyCode::Enter => Some(SpectrumKey::Enter),
        KeyCode::Space => Some(SpectrumKey::Space),

        _ => None,
    }
}

/// Keys to press when Backspace is pressed (CAPS SHIFT + 0 = DELETE on Spectrum).
///
/// Returns a pair of keys that should both be pressed/released together.
#[must_use]
pub fn backspace_keys() -> [SpectrumKey; 2] {
    [SpectrumKey::CapsShift, SpectrumKey::N0]
}
