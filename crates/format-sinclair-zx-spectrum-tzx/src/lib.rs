//! TZX tapes for the Sinclair ZX Spectrum.
//!
//! The block parsing lives in [`format_tzx`]; this crate is the Spectrum's
//! interpretation of it. That interpretation happens to be the identity
//! function, because TZX quotes its pulse lengths against a 3.5 MHz clock and
//! the Spectrum *is* a 3.5 MHz machine — the format was designed around it.
//!
//! The crate is not therefore redundant. It is where the Spectrum's reading of
//! the format would go if it ever needed one, and it keeps machines depending
//! on a system-named crate rather than reaching past it into the shared
//! parser, which `knowledge/decisions/crate-naming.md` names as the drift to
//! watch for. The Amstrad's `format-amstrad-cpc-cdt` is the same shape with a
//! 40/35 scale where this one has nothing.

use common_sinclair_zx_spectrum::TapeSpan;

/// Parses a TZX file into a Spectrum-facing timing stream.
///
/// Spans are in the Spectrum's own T-states, which for this format needs no
/// conversion.
///
/// # Errors
///
/// Returns an error if the file header is invalid, the version is unsupported,
/// a block overruns the supplied bytes, or the file contains an unknown block.
pub fn tzx_to_stream(data: &[u8]) -> Result<Vec<TapeSpan>, String> {
    format_tzx::tzx_to_stream(data)
}

#[cfg(test)]
mod tests {
    /// The Spectrum reads TZX at the format's own reference clock, so a pilot
    /// pulse must arrive as the 2,168 T-states the file quotes — unscaled. If
    /// a conversion ever appears in the shared parser, this fails.
    #[test]
    fn spectrum_timings_pass_through_unscaled() {
        // Minimal TZX: header, then a standard-speed block (0x10) holding one
        // byte with a 0 ms pause.
        let mut tzx = b"ZXTape!\x1a\x01\x14".to_vec();
        tzx.extend_from_slice(&[0x10, 0x00, 0x00, 0x01, 0x00, 0xFF]);

        let spans = super::tzx_to_stream(&tzx).expect("parse");
        let first = spans.first().expect("at least one span");
        assert_eq!(
            *first,
            common_sinclair_zx_spectrum::TapeSpan::Pulse(2_168),
            "pilot pulse should be the file's own figure"
        );
    }
}
