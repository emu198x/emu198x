//! The ZX Spectrum in a browser.
//!
//! Binds the generic [`emu198x_web`] host layer to the Spectrum runtime and
//! exposes it to JavaScript. Published to npm as `@emu198x/zx-spectrum`.
//!
//! Everything machine-independent lives in `emu198x-web`; this crate supplies
//! the runtime, the model, the firmware id, and the Spectrum's own names for
//! the keys a browser cannot express generically.

use format_sinclair_zx_spectrum_snapshot::Snapshot;

#[cfg(target_arch = "wasm32")]
mod browser;

#[cfg(target_arch = "wasm32")]
pub use browser::Spectrum;

/// The Sinclair 48K ROM, embedded at build time.
///
/// Present only under the `bundled-rom` feature, which the npm publish step
/// enables and nothing else does. The image is read from
/// `EMU198X_SPECTRUM_48K_ROM` at compile time, so it never enters this
/// repository — see `knowledge/decisions/test-rom-policy.md`
/// § Firmware in a published browser build for why that distinction matters.
#[cfg(feature = "bundled-rom")]
pub const BUNDLED_ROM: &[u8] = include_bytes!(env!("EMU198X_SPECTRUM_48K_ROM"));

/// Parses a portable Spectrum snapshot from bytes.
///
/// The curriculum's capture pipeline builds `.sna` files, so a lesson embed
/// has to load one — and the browser has no path to hand to the binary's
/// `parse_portable_snapshot_at`, which reads from disk. This is the same
/// parse, from bytes.
///
/// `format` is `sna` or `z80`, taken from the caller rather than sniffed:
/// the page knows what it fetched, and the two formats share no magic number
/// that would make guessing safe.
///
/// # Errors
///
/// Returns a message naming the format when it is unrecognised, or the
/// parser's own error when the bytes do not parse.
pub fn parse_snapshot(bytes: &[u8], format: &str) -> Result<Snapshot, String> {
    match format {
        "sna" => format_sinclair_zx_spectrum_sna::parse_sna(bytes),
        "z80" => format_sinclair_zx_spectrum_z80::parse_z80(bytes),
        other => Err(format!(
            "unknown snapshot format {other:?}; expected sna or z80"
        )),
    }
}

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

    #[cfg(feature = "bundled-rom")]
    #[test]
    fn the_bundled_rom_is_a_16k_image() {
        // Guards against the environment variable pointing at the wrong file:
        // a 128K ROM pair, a zip, or a snapshot would all compile fine and
        // then fail to boot in a browser with nothing to explain why.
        assert_eq!(
            BUNDLED_ROM.len(),
            16 * 1024,
            "EMU198X_SPECTRUM_48K_ROM is not a 16 KiB image"
        );
    }

    #[test]
    fn an_unknown_snapshot_format_is_named_rather_than_guessed() {
        let error = parse_snapshot(&[0u8; 49179], "tap").expect_err("tap is not a snapshot");
        assert!(
            error.contains("tap"),
            "the error should name what was asked for: {error}"
        );
    }

    #[test]
    fn a_truncated_sna_is_rejected() {
        // A short file must fail loudly rather than loading a machine whose
        // RAM is whatever happened to follow.
        assert!(parse_snapshot(&[0u8; 128], "sna").is_err());
    }

    #[test]
    fn a_full_length_sna_parses() {
        // 27-byte header plus 49152 bytes of RAM.
        let mut sna = [0u8; 49179];
        // SP at header offset 23, little-endian. The 48K format reads PC off
        // the stack, so SP has to point into RAM: 0x4000 is its first byte.
        // A zeroed SP points into ROM and makes the parser panic rather than
        // fail — see the note in the pull request.
        sna[23] = 0x00;
        sna[24] = 0x40;
        assert!(parse_snapshot(&sna, "sna").is_ok());
    }

    #[test]
    fn ordinary_keys_are_left_to_the_generic_mapping() {
        assert_eq!(spectrum_key_name("KeyA"), None);
        assert_eq!(spectrum_key_name("Enter"), None);
    }
}
