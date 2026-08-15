//! CDT tapes for the Amstrad CPC.
//!
//! A CDT is a TZX. The block parsing is [`format_tzx`]'s; this crate is the
//! CPC's reading of it, and that reading is one multiplication.
//!
//! # Why the scale exists
//!
//! TZX quotes every pulse length in T-states of a **3.5 MHz** clock. That is a
//! property of the format, not of the machine holding the tape: a CDT written
//! for a 4 MHz Amstrad still expresses its pulses against 3.5 MHz, because the
//! format came from the Spectrum world and kept its units.
//!
//! A pulse is a real duration though — so many microseconds of one level on a
//! wire — and the CPC counts its own time in 4 MHz T-states. Handing it the
//! file's figures unscaled would run every tape about 14% fast, which is not
//! subtle: a loader looking for a 2,168-cycle pilot would measure 1,897 and
//! reject the tape.
//!
//! Caprice32 does the same multiplication, as `CYCLE_SCALE ((40 << 16) / 35)`
//! in `tape.cpp`, with `MS_TO_CYCLES(p) ((dword)(p) * 4000)` where the
//! Spectrum would use 3,500.

use common_tape::TapeSpan;

/// The CPC's Z80 clock, in T-states per millisecond.
const CPC_TSTATES_PER_MS: u32 = 4_000;

/// The clock TZX quotes its pulse lengths against, in T-states per millisecond.
const TZX_REFERENCE_TSTATES_PER_MS: u32 = 3_500;

/// Converts one reference-clock duration to the CPC's clock.
///
/// Done in `u64` because a long TZX pause block can hold several seconds —
/// millions of T-states — and multiplying by 40 before dividing would overflow
/// `u32` for anything past about 27 seconds. Dividing first would instead throw
/// away the remainder on every short pulse, which is where accuracy actually
/// matters.
#[must_use]
fn scale(duration: u32) -> u32 {
    let scaled = u64::from(duration) * u64::from(CPC_TSTATES_PER_MS)
        / u64::from(TZX_REFERENCE_TSTATES_PER_MS);
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

/// Parses a CDT file into a CPC-facing timing stream.
///
/// Spans come back in the CPC's own 4 MHz T-states, ready to hand to a
/// [`common_tape::TapePlayer`] the machine advances at its CPU rate.
///
/// # Errors
///
/// Returns an error if the file header is invalid, the version is unsupported,
/// a block overruns the supplied bytes, or the file contains an unknown block.
pub fn cdt_to_stream(data: &[u8]) -> Result<Vec<TapeSpan>, String> {
    let spans = format_tzx::tzx_to_stream(data)?;
    Ok(spans
        .into_iter()
        .map(|span| match span {
            TapeSpan::Pulse(duration) => TapeSpan::Pulse(scale(duration)),
            TapeSpan::Level { duration, level } => TapeSpan::Level {
                duration: scale(duration),
                level,
            },
            TapeSpan::Stop => TapeSpan::Stop,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal TZX: header, then a standard-speed block holding one byte.
    fn minimal_cdt() -> Vec<u8> {
        let mut cdt = b"ZXTape!\x1a\x01\x14".to_vec();
        cdt.extend_from_slice(&[0x10, 0x00, 0x00, 0x01, 0x00, 0xFF]);
        cdt
    }

    #[test]
    fn a_pilot_pulse_arrives_on_the_cpc_clock() {
        // The file says 2,168 — the Spectrum's figure. The same real duration
        // on a 4 MHz machine is 2168 * 8/7 = 2477.71, truncating to 2,477. A
        // tape loaded unscaled runs 14% fast and a loader rejects it.
        let spans = cdt_to_stream(&minimal_cdt()).expect("parse");
        assert_eq!(spans.first(), Some(&TapeSpan::Pulse(2_477)));
    }

    #[test]
    fn scaling_is_the_ratio_of_the_two_clocks() {
        assert_eq!(scale(3_500), 4_000, "one millisecond, either way");
        assert_eq!(scale(0), 0);
        assert_eq!(scale(35), 40);
    }

    #[test]
    fn a_long_pause_does_not_overflow() {
        // Ten seconds of reference T-states. Multiplying by 40 before dividing
        // exceeds u32 here, which is why the arithmetic is done in u64.
        let ten_seconds = 35_000_000;
        assert_eq!(scale(ten_seconds), 40_000_000);
    }

    #[test]
    fn stop_spans_survive_the_scaling_pass() {
        // A Pause block (0x20) with a zero duration means "stop the tape and
        // wait for the user", and the parser emits Stop for it. Stop carries no
        // duration so there is nothing to scale — but a mapping that dropped or
        // mangled it would silently un-pause every multi-load game.
        let mut cdt = b"ZXTape!\x1a\x01\x14".to_vec();
        cdt.extend_from_slice(&[0x20, 0x00, 0x00]);

        let spans = cdt_to_stream(&cdt).expect("parse");
        assert!(
            spans.contains(&TapeSpan::Stop),
            "expected a Stop from the zero-length pause, got {spans:?}"
        );
    }

    #[test]
    fn a_bad_header_is_rejected_by_the_shared_parser() {
        assert!(cdt_to_stream(b"not a tape at all").is_err());
    }
}
