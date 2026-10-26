//! GTK key events in the winit vocabulary the machines' key maps speak.
//!
//! The machine key maps are written against winit's physical [`KeyCode`],
//! so the GTK host translates into it rather than asking 30 systems to learn
//! a second set of names.

use gtk4::gdk;
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKey, PhysicalKey};
use winit::platform::scancode::PhysicalKeyExtScancode;

/// The physical key for a GDK hardware keycode. On both X11 and Wayland that
/// keycode is the evdev scancode plus 8, which is what winit decodes too.
pub(super) fn key_code(hardware_keycode: u32) -> Option<KeyCode> {
    match PhysicalKey::from_scancode(hardware_keycode.saturating_sub(8)) {
        PhysicalKey::Code(code) => Some(code),
        PhysicalKey::Unidentified(_) => None,
    }
}

/// What the host layout makes of a key, as winit would report it: a printed
/// character, a dead key, or something else. Space is a named key in winit,
/// not a character, so it is one here too.
pub(super) fn logical_key(keyval: gdk::Key) -> Key {
    if keyval == gdk::Key::space {
        return Key::Named(NamedKey::Space);
    }
    if keyval.name().is_some_and(|name| name.starts_with("dead_")) {
        return Key::Dead(None);
    }
    match keyval.to_unicode() {
        Some(ch) if !ch.is_control() => Key::Character(ch.to_string().into()),
        _ => Key::Unidentified(NativeKey::Unidentified),
    }
}

/// The modifier keys held, as winit names them.
pub(super) fn modifiers(state: gdk::ModifierType) -> ModifiersState {
    let mut modifiers = ModifiersState::empty();
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        modifiers |= ModifiersState::SHIFT;
    }
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        modifiers |= ModifiersState::CONTROL;
    }
    if state.contains(gdk::ModifierType::ALT_MASK) {
        modifiers |= ModifiersState::ALT;
    }
    if state.contains(gdk::ModifierType::SUPER_MASK) {
        modifiers |= ModifiersState::SUPER;
    }
    modifiers
}

/// The winit button for a GDK button number.
pub(super) fn mouse_button(button: u32) -> winit::event::MouseButton {
    use winit::event::MouseButton;
    match button {
        1 => MouseButton::Left,
        2 => MouseButton::Middle,
        3 => MouseButton::Right,
        8 => MouseButton::Back,
        9 => MouseButton::Forward,
        other => MouseButton::Other(u16::try_from(other).unwrap_or(u16::MAX)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_keycodes_are_evdev_plus_eight() {
        // evdev KEY_A is 30, KEY_F12 is 88, KEY_SPACE is 57.
        assert_eq!(key_code(38), Some(KeyCode::KeyA));
        assert_eq!(key_code(96), Some(KeyCode::F12));
        assert_eq!(key_code(65), Some(KeyCode::Space));
        assert_eq!(key_code(0), None);
    }

    #[test]
    fn modifiers_map_one_to_one() {
        let state = gdk::ModifierType::SHIFT_MASK | gdk::ModifierType::CONTROL_MASK;
        assert_eq!(
            modifiers(state),
            ModifiersState::SHIFT | ModifiersState::CONTROL
        );
        assert_eq!(
            modifiers(gdk::ModifierType::empty()),
            ModifiersState::empty()
        );
    }

    #[test]
    fn mouse_buttons_follow_gdk_numbering() {
        use winit::event::MouseButton;
        assert_eq!(mouse_button(1), MouseButton::Left);
        assert_eq!(mouse_button(2), MouseButton::Middle);
        assert_eq!(mouse_button(3), MouseButton::Right);
    }
}
