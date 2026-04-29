//! Game Boy joypad input mapping.
//!
//! Splits the joypad button-name lookup out of `runtime.rs` so the
//! eight-button DMG joypad has one home. Both `InputEvent::Key` and
//! `InputEvent::Button` route through the same name table — the DMG
//! has no keyboard, so a host-key event with a recognised name is
//! treated as a button press on the joypad.

use common_nintendo_game_boy::JoypadButton;
use emu198x_shell::InputEvent;
use machine_nintendo_game_boy::GameBoy;

/// Apply one host input event to the machine: either a recognised
/// joypad button name lands as a press, or the event is dropped. Does
/// nothing when no cartridge is loaded — there is no machine to
/// receive the input.
pub(crate) fn apply_input_event(machine: Option<&mut GameBoy>, event: &InputEvent) {
    let Some(machine) = machine else {
        return;
    };
    let (name, pressed) = match event {
        InputEvent::Key { name, pressed } => (name.as_ref(), *pressed),
        InputEvent::Button { name, pressed, .. } => (name.as_ref(), *pressed),
        _ => return,
    };
    if let Some(button) = button_from_name(name) {
        machine.set_button(button, pressed);
    }
}

/// Map a host-level button or key name to its DMG joypad button.
/// Names are matched case-insensitively. Returns `None` for names
/// that don't have a joypad button; callers silently drop those.
fn button_from_name(name: &str) -> Option<JoypadButton> {
    Some(match name.to_ascii_lowercase().as_str() {
        "a" => JoypadButton::A,
        "b" => JoypadButton::B,
        "select" => JoypadButton::Select,
        "start" => JoypadButton::Start,
        "up" => JoypadButton::Up,
        "down" => JoypadButton::Down,
        "left" => JoypadButton::Left,
        "right" => JoypadButton::Right,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::button_from_name;
    use common_nintendo_game_boy::JoypadButton;

    /// Spec invariant: every DMG joypad button has a name lookup.
    /// One assert per arm catches a regression where a rename or a
    /// re-cased lookup silently stops mapping a button.
    #[test]
    fn every_joypad_button_has_a_name() {
        assert_eq!(button_from_name("a"), Some(JoypadButton::A));
        assert_eq!(button_from_name("b"), Some(JoypadButton::B));
        assert_eq!(button_from_name("select"), Some(JoypadButton::Select));
        assert_eq!(button_from_name("start"), Some(JoypadButton::Start));
        assert_eq!(button_from_name("up"), Some(JoypadButton::Up));
        assert_eq!(button_from_name("down"), Some(JoypadButton::Down));
        assert_eq!(button_from_name("left"), Some(JoypadButton::Left));
        assert_eq!(button_from_name("right"), Some(JoypadButton::Right));
        // Case-insensitive lookup is part of the contract — the
        // native shell sends mixed-case names and the script DSL
        // sends uppercase names.
        assert_eq!(button_from_name("Start"), Some(JoypadButton::Start));
        assert_eq!(button_from_name("LEFT"), Some(JoypadButton::Left));
        assert_eq!(button_from_name("unknown"), None);
        assert_eq!(button_from_name(""), None);
    }
}
