//! Host keyboard → Spectrum key mapping.
//!
//! Maps winit `KeyCode` values to `SpectrumKey` for the windowed binary.
//!
//! The Spectrum has a 40-key matrix with no dedicated punctuation keys.
//! Punctuation is produced by pressing Symbol Shift (or Caps Shift) with
//! another key. This module provides both single-key and combo mappings
//! so common host punctuation keys produce the expected Spectrum symbol.

use winit::keyboard::KeyCode;

use crate::input::SpectrumKey;

/// A mapped key: either a single Spectrum key, or a two-key combo
/// (modifier + key) that should be pressed/released together.
#[derive(Clone, Copy)]
pub enum MappedKey {
    Single(SpectrumKey),
    Combo([SpectrumKey; 2]),
}

/// Map a host key to a Spectrum key or key combo.
///
/// Returns `None` for unmapped keys.
#[must_use]
pub fn map_keycode(key: KeyCode) -> Option<MappedKey> {
    match key {
        // Letters
        KeyCode::KeyA => Some(MappedKey::Single(SpectrumKey::A)),
        KeyCode::KeyB => Some(MappedKey::Single(SpectrumKey::B)),
        KeyCode::KeyC => Some(MappedKey::Single(SpectrumKey::C)),
        KeyCode::KeyD => Some(MappedKey::Single(SpectrumKey::D)),
        KeyCode::KeyE => Some(MappedKey::Single(SpectrumKey::E)),
        KeyCode::KeyF => Some(MappedKey::Single(SpectrumKey::F)),
        KeyCode::KeyG => Some(MappedKey::Single(SpectrumKey::G)),
        KeyCode::KeyH => Some(MappedKey::Single(SpectrumKey::H)),
        KeyCode::KeyI => Some(MappedKey::Single(SpectrumKey::I)),
        KeyCode::KeyJ => Some(MappedKey::Single(SpectrumKey::J)),
        KeyCode::KeyK => Some(MappedKey::Single(SpectrumKey::K)),
        KeyCode::KeyL => Some(MappedKey::Single(SpectrumKey::L)),
        KeyCode::KeyM => Some(MappedKey::Single(SpectrumKey::M)),
        KeyCode::KeyN => Some(MappedKey::Single(SpectrumKey::N)),
        KeyCode::KeyO => Some(MappedKey::Single(SpectrumKey::O)),
        KeyCode::KeyP => Some(MappedKey::Single(SpectrumKey::P)),
        KeyCode::KeyQ => Some(MappedKey::Single(SpectrumKey::Q)),
        KeyCode::KeyR => Some(MappedKey::Single(SpectrumKey::R)),
        KeyCode::KeyS => Some(MappedKey::Single(SpectrumKey::S)),
        KeyCode::KeyT => Some(MappedKey::Single(SpectrumKey::T)),
        KeyCode::KeyU => Some(MappedKey::Single(SpectrumKey::U)),
        KeyCode::KeyV => Some(MappedKey::Single(SpectrumKey::V)),
        KeyCode::KeyW => Some(MappedKey::Single(SpectrumKey::W)),
        KeyCode::KeyX => Some(MappedKey::Single(SpectrumKey::X)),
        KeyCode::KeyY => Some(MappedKey::Single(SpectrumKey::Y)),
        KeyCode::KeyZ => Some(MappedKey::Single(SpectrumKey::Z)),

        // Digits
        KeyCode::Digit0 => Some(MappedKey::Single(SpectrumKey::N0)),
        KeyCode::Digit1 => Some(MappedKey::Single(SpectrumKey::N1)),
        KeyCode::Digit2 => Some(MappedKey::Single(SpectrumKey::N2)),
        KeyCode::Digit3 => Some(MappedKey::Single(SpectrumKey::N3)),
        KeyCode::Digit4 => Some(MappedKey::Single(SpectrumKey::N4)),
        KeyCode::Digit5 => Some(MappedKey::Single(SpectrumKey::N5)),
        KeyCode::Digit6 => Some(MappedKey::Single(SpectrumKey::N6)),
        KeyCode::Digit7 => Some(MappedKey::Single(SpectrumKey::N7)),
        KeyCode::Digit8 => Some(MappedKey::Single(SpectrumKey::N8)),
        KeyCode::Digit9 => Some(MappedKey::Single(SpectrumKey::N9)),

        // Modifiers
        KeyCode::ShiftLeft => Some(MappedKey::Single(SpectrumKey::CapsShift)),
        KeyCode::ShiftRight | KeyCode::ControlLeft | KeyCode::ControlRight => {
            Some(MappedKey::Single(SpectrumKey::SymShift))
        }

        // Special keys
        KeyCode::Enter => Some(MappedKey::Single(SpectrumKey::Enter)),
        KeyCode::Space => Some(MappedKey::Single(SpectrumKey::Space)),
        KeyCode::Backspace => Some(MappedKey::Combo([SpectrumKey::CapsShift, SpectrumKey::N0])),
        KeyCode::Tab => Some(MappedKey::Combo([SpectrumKey::CapsShift, SpectrumKey::Space])),

        // Symbolic punctuation (Symbol Shift + key)
        KeyCode::Semicolon => {
            Some(MappedKey::Combo([SpectrumKey::SymShift, SpectrumKey::O]))
        }
        KeyCode::Quote => {
            Some(MappedKey::Combo([SpectrumKey::SymShift, SpectrumKey::P]))
        }
        KeyCode::Comma => {
            Some(MappedKey::Combo([SpectrumKey::SymShift, SpectrumKey::N]))
        }
        KeyCode::Period => {
            Some(MappedKey::Combo([SpectrumKey::SymShift, SpectrumKey::M]))
        }
        KeyCode::Slash => {
            Some(MappedKey::Combo([SpectrumKey::SymShift, SpectrumKey::V]))
        }
        KeyCode::Minus => {
            Some(MappedKey::Combo([SpectrumKey::SymShift, SpectrumKey::J]))
        }
        KeyCode::Equal => {
            Some(MappedKey::Combo([SpectrumKey::SymShift, SpectrumKey::L]))
        }

        // Cursor keys → Spectrum cursor keys (CS+5/6/7/8)
        KeyCode::ArrowLeft => {
            Some(MappedKey::Combo([SpectrumKey::CapsShift, SpectrumKey::N5]))
        }
        KeyCode::ArrowDown => {
            Some(MappedKey::Combo([SpectrumKey::CapsShift, SpectrumKey::N6]))
        }
        KeyCode::ArrowUp => {
            Some(MappedKey::Combo([SpectrumKey::CapsShift, SpectrumKey::N7]))
        }
        KeyCode::ArrowRight => {
            Some(MappedKey::Combo([SpectrumKey::CapsShift, SpectrumKey::N8]))
        }

        _ => None,
    }
}
