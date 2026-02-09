//! Host keyboard → C64 key mapping.
//!
//! Maps winit `KeyCode` values to `C64Key` for the windowed binary.

use winit::keyboard::KeyCode;

use crate::input::C64Key;

/// Map a host key to a C64 key.
///
/// Returns `None` for unmapped keys.
#[must_use]
pub fn map_keycode(key: KeyCode) -> Option<C64Key> {
    match key {
        // Letters
        KeyCode::KeyA => Some(C64Key::A),
        KeyCode::KeyB => Some(C64Key::B),
        KeyCode::KeyC => Some(C64Key::C),
        KeyCode::KeyD => Some(C64Key::D),
        KeyCode::KeyE => Some(C64Key::E),
        KeyCode::KeyF => Some(C64Key::F),
        KeyCode::KeyG => Some(C64Key::G),
        KeyCode::KeyH => Some(C64Key::H),
        KeyCode::KeyI => Some(C64Key::I),
        KeyCode::KeyJ => Some(C64Key::J),
        KeyCode::KeyK => Some(C64Key::K),
        KeyCode::KeyL => Some(C64Key::L),
        KeyCode::KeyM => Some(C64Key::M),
        KeyCode::KeyN => Some(C64Key::N),
        KeyCode::KeyO => Some(C64Key::O),
        KeyCode::KeyP => Some(C64Key::P),
        KeyCode::KeyQ => Some(C64Key::Q),
        KeyCode::KeyR => Some(C64Key::R),
        KeyCode::KeyS => Some(C64Key::S),
        KeyCode::KeyT => Some(C64Key::T),
        KeyCode::KeyU => Some(C64Key::U),
        KeyCode::KeyV => Some(C64Key::V),
        KeyCode::KeyW => Some(C64Key::W),
        KeyCode::KeyX => Some(C64Key::X),
        KeyCode::KeyY => Some(C64Key::Y),
        KeyCode::KeyZ => Some(C64Key::Z),

        // Digits
        KeyCode::Digit0 => Some(C64Key::N0),
        KeyCode::Digit1 => Some(C64Key::N1),
        KeyCode::Digit2 => Some(C64Key::N2),
        KeyCode::Digit3 => Some(C64Key::N3),
        KeyCode::Digit4 => Some(C64Key::N4),
        KeyCode::Digit5 => Some(C64Key::N5),
        KeyCode::Digit6 => Some(C64Key::N6),
        KeyCode::Digit7 => Some(C64Key::N7),
        KeyCode::Digit8 => Some(C64Key::N8),
        KeyCode::Digit9 => Some(C64Key::N9),

        // Modifiers
        KeyCode::ShiftLeft => Some(C64Key::LShift),
        KeyCode::ShiftRight => Some(C64Key::RShift),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some(C64Key::Ctrl),
        KeyCode::Tab => Some(C64Key::Commodore),

        // Special keys
        KeyCode::Enter => Some(C64Key::Return),
        KeyCode::Space => Some(C64Key::Space),
        KeyCode::Backspace => Some(C64Key::Delete),
        KeyCode::Home => Some(C64Key::Home),

        // Function keys
        KeyCode::F1 => Some(C64Key::F1),
        KeyCode::F3 => Some(C64Key::F3),
        KeyCode::F5 => Some(C64Key::F5),
        KeyCode::F7 => Some(C64Key::F7),

        // Cursor keys
        KeyCode::ArrowDown => Some(C64Key::CursorDown),
        KeyCode::ArrowRight => Some(C64Key::CursorRight),

        // Punctuation
        KeyCode::Period => Some(C64Key::Period),
        KeyCode::Comma => Some(C64Key::Comma),
        KeyCode::Slash => Some(C64Key::Slash),
        KeyCode::Semicolon => Some(C64Key::Semicolon),
        KeyCode::Equal => Some(C64Key::Equals),
        KeyCode::Minus => Some(C64Key::Minus),

        _ => None,
    }
}

/// Keys to press when cursor-up is needed (SHIFT + cursor-down on C64).
#[must_use]
pub fn cursor_up_keys() -> [C64Key; 2] {
    [C64Key::LShift, C64Key::CursorDown]
}

/// Keys to press when cursor-left is needed (SHIFT + cursor-right on C64).
#[must_use]
pub fn cursor_left_keys() -> [C64Key; 2] {
    [C64Key::LShift, C64Key::CursorRight]
}
