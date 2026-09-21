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
///
/// Redistributed under Amstrad's permission for emulator authors (Cliff
/// Lawson, Amstrad plc, comp.sys.sinclair, 31 August 1999). Two conditions
/// ride on that and are easy to break by accident: the image must not be
/// patched, because the permission turns on its copyright messages being
/// unaltered, and no charge may be made for the ROM itself. The
/// acknowledgement it asks for ships in this crate's README, which wasm-pack
/// includes in the published package.
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

/// Convert an editable numbered listing into a self-starting BASIC tape.
///
/// # Errors
/// Returns listing errors without changing the source or starting a machine.
pub fn basic_tape(source: &str, name: &str) -> Result<Vec<u8>, String> {
    use format198x_sinclair_zx_spectrum_tap::{Header, HeaderKind, TapBlock, encode};
    let program = format_sinclair_zx_spectrum_bas::tokenise_listing(source)?;
    let length = u16::try_from(program.bytes.len()).map_err(|_| "BASIC program too large")?;
    let start = u16::from_be_bytes([program.bytes[0], program.bytes[1]]);
    Ok(encode(&[
        Header::new(HeaderKind::Program, name, length, start, length).block(),
        TapBlock::data(program.bytes),
    ]))
}

#[cfg(test)]
mod basic_tape_tests {
    use super::basic_tape;
    use format198x_sinclair_zx_spectrum_tap::{Header, HeaderKind, decode};

    #[test]
    fn tape_header_starts_at_first_sorted_line_and_has_no_variables() {
        let tape = basic_tape("20 PRINT \"two\"\n10 PRINT \"one\"", "greeting").expect("tape");
        let blocks = decode(&tape).expect("valid TAP blocks and checksums");
        assert_eq!(blocks.len(), 2);
        let header = Header::from_payload(&blocks[0].data).expect("header");
        assert_eq!(header.kind, HeaderKind::Program);
        assert_eq!(header.param1, 10);
        assert_eq!(header.param2, header.length);
        assert_eq!(usize::from(header.length), blocks[1].data.len());
        assert_eq!(&blocks[1].data[..2], &[0, 10]);
    }
}

/// BASIC launcher for a direct-to-RAM machine-code program.
///
/// # Errors
/// Rejects code outside the supported RAM area or an entry outside its bytes.
pub fn code_launcher(length: usize, origin: u32, entry: u32) -> Result<String, String> {
    let end = u32::try_from(length)
        .ok()
        .and_then(|len| origin.checked_add(len));
    if length == 0
        || origin < 0x6000
        || end.is_none_or(|end| end > 0x10000 || entry < origin || entry >= end)
    {
        return Err(
            "Code must fit in RAM at 24576 or above, with its entry point inside the program."
                .to_owned(),
        );
    }
    Ok(format!("10 CLEAR {}: RANDOMIZE USR {}", origin - 1, entry))
}

#[cfg(test)]
mod launcher_tests {
    use super::code_launcher;
    #[test]
    fn launcher_reserves_code_and_preserves_the_requested_entry() {
        assert_eq!(
            code_launcher(8, 32768, 32770).expect("valid code"),
            "10 CLEAR 32767: RANDOMIZE USR 32770"
        );
        assert!(code_launcher(1, 65535, 65535).is_ok());
        for (length, origin, entry) in [
            (0, 32768, 32768),
            (8, 0x5ccb, 0x5ccb),
            (8, 65535, 65535),
            (8, 32768, 32776),
            (8, u32::MAX, u32::MAX),
        ] {
            assert!(code_launcher(length, origin, entry).is_err());
        }
    }
}
