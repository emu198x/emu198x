//! DOM keyboard events into machine key events.
//!
//! Maps `KeyboardEvent.code`, not `.key`. `code` names the physical key, so a
//! learner on an AZERTY or Dvorak layout presses the key that is physically
//! where the Spectrum's is, rather than the one their OS relabelled it as.
//! `.key` would also change with shift state, which the machine models itself.
//!
//! Only keys whose name means the same thing on every machine are mapped here:
//! letters, digits, Space, Enter, the arrows and Delete. Modifiers are
//! deliberately absent — the Spectrum calls them `CapsShift` and
//! `SymbolShift`, other machines call them other things, and guessing a name
//! the machine rejects is worse than declining to map it. A per-system binding
//! adds its own, exactly as the native UI's `map_keys` does.

/// Maps a DOM `KeyboardEvent.code` to a machine key name.
///
/// Returns `None` for a code with no machine-neutral name, which the caller
/// should treat as "not ours" and leave to the page.
#[must_use]
pub fn dom_code_to_key_name(code: &str) -> Option<&'static str> {
    let name = match code {
        "KeyA" => "A",
        "KeyB" => "B",
        "KeyC" => "C",
        "KeyD" => "D",
        "KeyE" => "E",
        "KeyF" => "F",
        "KeyG" => "G",
        "KeyH" => "H",
        "KeyI" => "I",
        "KeyJ" => "J",
        "KeyK" => "K",
        "KeyL" => "L",
        "KeyM" => "M",
        "KeyN" => "N",
        "KeyO" => "O",
        "KeyP" => "P",
        "KeyQ" => "Q",
        "KeyR" => "R",
        "KeyS" => "S",
        "KeyT" => "T",
        "KeyU" => "U",
        "KeyV" => "V",
        "KeyW" => "W",
        "KeyX" => "X",
        "KeyY" => "Y",
        "KeyZ" => "Z",
        "Digit0" | "Numpad0" => "0",
        "Digit1" | "Numpad1" => "1",
        "Digit2" | "Numpad2" => "2",
        "Digit3" | "Numpad3" => "3",
        "Digit4" | "Numpad4" => "4",
        "Digit5" | "Numpad5" => "5",
        "Digit6" | "Numpad6" => "6",
        "Digit7" | "Numpad7" => "7",
        "Digit8" | "Numpad8" => "8",
        "Digit9" | "Numpad9" => "9",
        "Space" => "Space",
        "Enter" | "NumpadEnter" => "Enter",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "Backspace" => "Delete",
        _ => return None,
    };
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_digits_map_to_their_keycap() {
        assert_eq!(dom_code_to_key_name("KeyA"), Some("A"));
        assert_eq!(dom_code_to_key_name("KeyZ"), Some("Z"));
        assert_eq!(dom_code_to_key_name("Digit0"), Some("0"));
        assert_eq!(dom_code_to_key_name("Digit9"), Some("9"));
    }

    #[test]
    fn the_numpad_reaches_the_same_keys() {
        assert_eq!(
            dom_code_to_key_name("Numpad5"),
            dom_code_to_key_name("Digit5")
        );
        assert_eq!(
            dom_code_to_key_name("NumpadEnter"),
            dom_code_to_key_name("Enter")
        );
    }

    #[test]
    fn an_unmapped_code_is_declined_rather_than_guessed() {
        assert_eq!(dom_code_to_key_name("F13"), None);
        assert_eq!(dom_code_to_key_name(""), None);
        assert_eq!(dom_code_to_key_name("Nonsense"), None);
    }

    #[test]
    fn modifiers_are_left_to_the_per_system_binding() {
        // The Spectrum calls these CapsShift and SymbolShift. Mapping them to
        // a neutral "Shift" here would produce a name the machine rejects.
        assert_eq!(dom_code_to_key_name("ShiftLeft"), None);
        assert_eq!(dom_code_to_key_name("ControlLeft"), None);
        assert_eq!(dom_code_to_key_name("AltLeft"), None);
    }

    #[test]
    fn the_mapping_is_case_sensitive_because_dom_codes_are() {
        assert_eq!(dom_code_to_key_name("keya"), None);
    }
}
