//! Host keyboard → NES button mapping.
//!
//! Maps winit `KeyCode` values to `NesButton` for the windowed binary.
//!
//! Default mapping:
//! - Arrows → D-pad
//! - Z → A button
//! - X → B button
//! - Enter → Start
//! - Right Shift → Select

use winit::keyboard::KeyCode;

use crate::input::NesButton;

/// Map a host key to an NES button.
///
/// Returns `None` for unmapped keys.
#[must_use]
pub fn map_keycode(key: KeyCode) -> Option<NesButton> {
    match key {
        KeyCode::ArrowUp => Some(NesButton::Up),
        KeyCode::ArrowDown => Some(NesButton::Down),
        KeyCode::ArrowLeft => Some(NesButton::Left),
        KeyCode::ArrowRight => Some(NesButton::Right),
        KeyCode::KeyZ | KeyCode::KeyA => Some(NesButton::A),
        KeyCode::KeyX | KeyCode::KeyS => Some(NesButton::B),
        KeyCode::Enter => Some(NesButton::Start),
        KeyCode::ShiftRight => Some(NesButton::Select),
        _ => None,
    }
}
