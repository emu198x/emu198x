//! The ZX Spectrum in a browser.
//!
//! Binds the generic [`emu198x_web`] host layer to the Spectrum runtime and
//! exposes it to JavaScript. Published to npm as `@emu198x/zx-spectrum`.
//!
//! Everything machine-independent lives in `emu198x-web`; this crate supplies
//! the runtime, the model, the firmware id, and the Spectrum's own names for
//! the keys a browser cannot express generically.

#[cfg(target_arch = "wasm32")]
mod browser;

#[cfg(target_arch = "wasm32")]
pub use browser::Spectrum;

/// The Spectrum's names for keys no generic mapping can supply.
///
/// `emu198x-web` maps only names that mean the same thing on every machine,
/// which leaves out the modifiers: this machine calls them `CapsShift` and
/// `SymbolShift`. Shift is the obvious home for `CapsShift`; `SymbolShift`
/// takes Control and Alt because a browser gives us no better key for it and
/// both sit where a thumb expects.
pub fn spectrum_key_name(code: &str) -> Option<&'static str> {
    match code {
        "ShiftLeft" | "ShiftRight" => Some("CapsShift"),
        "ControlLeft" | "ControlRight" | "AltLeft" | "AltRight" => Some("SymbolShift"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_reaches_caps_shift_because_the_spectrum_has_no_plain_shift() {
        assert_eq!(spectrum_key_name("ShiftLeft"), Some("CapsShift"));
        assert_eq!(spectrum_key_name("ShiftRight"), Some("CapsShift"));
    }

    #[test]
    fn control_and_alt_both_reach_symbol_shift() {
        assert_eq!(spectrum_key_name("ControlLeft"), Some("SymbolShift"));
        assert_eq!(spectrum_key_name("AltRight"), Some("SymbolShift"));
    }

    #[test]
    fn ordinary_keys_are_left_to_the_generic_mapping() {
        assert_eq!(spectrum_key_name("KeyA"), None);
        assert_eq!(spectrum_key_name("Enter"), None);
    }
}
