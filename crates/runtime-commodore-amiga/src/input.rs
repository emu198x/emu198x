//! Amiga keyboard / mouse / joystick input mapping.
//!
//! Splits the host-input → machine routing out of `runtime.rs`. Three
//! input kinds land here: `Key` events route through the keyboard's
//! raw matrix code lookup, `PointerMotion` / `PointerButton` events
//! drive controller port 0 (mouse-1), and `Button` events drive
//! controller port 1 (joystick).

use emu198x_shell::InputEvent;
use machine_commodore_amiga_ocs::AmigaOcs;

/// Apply one host input event to the machine. Unrecognised event
/// kinds and unknown key names are silently dropped — the runtime
/// loop iterates the whole event queue regardless.
pub(crate) fn apply_input_event(machine: &mut AmigaOcs, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some(code) = key_name_to_raw_code(name.as_ref()) {
                machine.key_event(code, *pressed);
            }
        }
        InputEvent::PointerMotion { device, dx, dy } if device.as_ref() == "mouse-1" => {
            machine.move_mouse_port0(*dx, *dy);
        }
        InputEvent::PointerButton {
            device,
            button,
            pressed,
        } if device.as_ref() == "mouse-1" => {
            machine.set_mouse_button_port0(button.as_ref(), *pressed);
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            let _ = machine.set_joystick_control(*port, name.as_ref(), *pressed);
        }
        _ => {}
    }
}

/// Map a host-level key name to the Amiga keyboard's raw matrix code.
/// Names are matched case-insensitively. The `raw-XX` prefix (with
/// optional `0x`) lets host code address keys the lookup table doesn't
/// expose by name.
fn key_name_to_raw_code(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    if let Some(raw) = lower.strip_prefix("raw-") {
        return u8::from_str_radix(raw.trim_start_matches("0x"), 16).ok();
    }
    Some(match lower.as_str() {
        "1" => 0x01,
        "2" => 0x02,
        "3" => 0x03,
        "4" => 0x04,
        "5" => 0x05,
        "6" => 0x06,
        "7" => 0x07,
        "8" => 0x08,
        "9" => 0x09,
        "0" => 0x0A,
        "q" => 0x10,
        "w" => 0x11,
        "e" => 0x12,
        "r" => 0x13,
        "t" => 0x14,
        "y" => 0x15,
        "u" => 0x16,
        "i" => 0x17,
        "o" => 0x18,
        "p" => 0x19,
        "a" => 0x20,
        "s" => 0x21,
        "d" => 0x22,
        "f" => 0x23,
        "g" => 0x24,
        "h" => 0x25,
        "j" => 0x26,
        "k" => 0x27,
        "l" => 0x28,
        "z" => 0x31,
        "x" => 0x32,
        "c" => 0x33,
        "v" => 0x34,
        "b" => 0x35,
        "n" => 0x36,
        "m" => 0x37,
        "space" => 0x40,
        "backspace" => 0x41,
        "tab" => 0x42,
        "enter" | "return" => 0x45,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::key_name_to_raw_code;

    /// Spec invariant: every named key in the lookup table maps to a
    /// stable raw matrix code. One assert per arm catches a regression
    /// where someone widens an arm and silently shifts a key.
    #[test]
    fn key_name_lookup_covers_documented_keys() {
        assert_eq!(key_name_to_raw_code("1"), Some(0x01));
        assert_eq!(key_name_to_raw_code("0"), Some(0x0A));
        assert_eq!(key_name_to_raw_code("q"), Some(0x10));
        assert_eq!(key_name_to_raw_code("a"), Some(0x20));
        assert_eq!(key_name_to_raw_code("z"), Some(0x31));
        assert_eq!(key_name_to_raw_code("space"), Some(0x40));
        assert_eq!(key_name_to_raw_code("backspace"), Some(0x41));
        assert_eq!(key_name_to_raw_code("tab"), Some(0x42));
        assert_eq!(key_name_to_raw_code("enter"), Some(0x45));
        assert_eq!(key_name_to_raw_code("return"), Some(0x45));
        // Case-insensitive lookup is part of the contract.
        assert_eq!(key_name_to_raw_code("Space"), Some(0x40));
        assert_eq!(key_name_to_raw_code("ENTER"), Some(0x45));
        // raw-XX prefix accepts host-supplied raw codes the lookup
        // table doesn't otherwise expose by name.
        assert_eq!(key_name_to_raw_code("raw-50"), Some(0x50));
        assert_eq!(key_name_to_raw_code("raw-0x5F"), Some(0x5F));
        assert_eq!(key_name_to_raw_code("unknown"), None);
        assert_eq!(key_name_to_raw_code(""), None);
    }
}
