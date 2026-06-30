//! UEF (Unified Emulator Format) cassette tape parser for the Acorn family —
//! BBC Micro, Acorn Electron, and Acorn Atom.
//!
//! UEF is a chunk-based, optionally gzip-compressed container for Kansas-City /
//! CUTS-format tape audio. This crate parses the container and synthesises the
//! cassette waveform as a compact, clock-neutral [`TapePulse`] stream that each
//! machine's cassette hardware samples at its own clock — the timing
//! reconstruction itself stays in the format crate, while feeding the level into
//! a 6850 ACIA / 8255 PPI / ULA shift register stays in the machine.
//!
//! ```
//! # use format_acorn_uef::{parse, TapePulse};
//! // A minimal UEF: magic, version, then a 4-cycle carrier tone.
//! let mut image = b"UEF File!\0\x0a\x00".to_vec();
//! image.extend_from_slice(&[0x10, 0x01, 0x02, 0x00, 0x00, 0x00, 0x04, 0x00]);
//! let tape = parse(&image).unwrap();
//! assert_eq!(tape.pulses, vec![TapePulse::Cycles { half_period_ns: 208_333, count: 4 }]);
//! ```
//!
//! Source reference: ported from MAME `src/lib/formats/uef_cas.cpp`
//! (Wilbert Pol, BSD-3-Clause).

mod decode;
mod encode;
mod error;
mod pulse;

pub use common_acorn_cassette::TapePulse;
pub use decode::parse;
pub use encode::encode_blocks;
pub use error::UefError;
pub use pulse::UefTape;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a UEF image from a list of `(chunk_id, payload)` pairs.
    fn build(chunks: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut image = b"UEF File!\0".to_vec();
        image.extend_from_slice(&[0x0a, 0x00]); // version 0.10
        for (id, payload) in chunks {
            image.extend_from_slice(&id.to_le_bytes());
            image.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            image.extend_from_slice(payload);
        }
        image
    }

    // The half-periods the decoder derives from the default 1200 Hz base.
    const ZERO_HALF: u32 = 416_667; // 1200 Hz half-period
    const ONE_HALF: u32 = 208_333; // 2400 Hz half-period

    #[test]
    fn rejects_non_uef() {
        assert_eq!(
            parse(b"not a tape at all").expect_err("non-UEF must fail"),
            UefError::BadMagic
        );
    }

    #[test]
    fn rejects_too_small() {
        assert_eq!(
            parse(b"UEF").expect_err("short input must fail"),
            UefError::TooSmall(3)
        );
    }

    #[test]
    fn empty_tape_has_no_pulses() {
        let tape = parse(&build(&[])).expect("header-only image parses");
        assert!(tape.is_empty());
    }

    #[test]
    fn carrier_tone_is_one_run() {
        let tape = parse(&build(&[(0x0110, 4u16.to_le_bytes().to_vec())])).expect("carrier parses");
        assert_eq!(
            tape.pulses,
            vec![TapePulse::Cycles {
                half_period_ns: ONE_HALF,
                count: 4
            }]
        );
    }

    #[test]
    fn zero_byte_frames_start_eight_zeros_and_stop() {
        // 0x00 = start(0) + eight 0 bits + stop(1).
        let tape = parse(&build(&[(0x0100, vec![0x00])])).expect("data block parses");
        let mut expected = vec![
            TapePulse::Cycles {
                half_period_ns: ZERO_HALF,
                count: 1
            };
            9 // start bit + eight data bits, all low
        ];
        expected.push(TapePulse::Cycles {
            half_period_ns: ONE_HALF,
            count: 2, // stop bit, high
        });
        assert_eq!(tape.pulses, expected);
    }

    #[test]
    fn one_bits_use_the_high_tone() {
        // 0xFF = start(0) + eight 1 bits + stop(1): one low cycle then nine highs.
        let tape = parse(&build(&[(0x0100, vec![0xFF])])).expect("data block parses");
        assert_eq!(tape.pulses.len(), 10);
        assert_eq!(
            tape.pulses[0],
            TapePulse::Cycles {
                half_period_ns: ZERO_HALF,
                count: 1
            }
        );
        for pulse in &tape.pulses[1..] {
            assert_eq!(
                *pulse,
                TapePulse::Cycles {
                    half_period_ns: ONE_HALF,
                    count: 2
                }
            );
        }
    }

    #[test]
    fn integer_gap_is_silence() {
        // 2400 half-base-period units at 1200 Hz base = exactly one second.
        let tape = parse(&build(&[(0x0112, 2400u16.to_le_bytes().to_vec())])).expect("gap parses");
        assert_eq!(
            tape.pulses,
            vec![TapePulse::Gap {
                duration_ns: 1_000_000_000
            }]
        );
    }

    #[test]
    fn float_gap_one_second() {
        // 1.0 as an IEEE-754 single, little-endian.
        let tape =
            parse(&build(&[(0x0116, vec![0x00, 0x00, 0x80, 0x3F])])).expect("float gap parses");
        assert_eq!(
            tape.pulses,
            vec![TapePulse::Gap {
                duration_ns: 1_000_000_000
            }]
        );
    }

    #[test]
    fn baud_change_to_300_quadruples_cycles_per_bit() {
        let tape = parse(&build(&[
            (0x0117, 300u16.to_le_bytes().to_vec()),
            (0x0110, 1u16.to_le_bytes().to_vec()),
            (0x0100, vec![0x00]),
        ]))
        .expect("baud-change tape parses");
        // Carrier is unaffected by baud (explicit cycle count)...
        assert_eq!(
            tape.pulses[0],
            TapePulse::Cycles {
                half_period_ns: ONE_HALF,
                count: 1
            }
        );
        // ...but each 0 bit now emits four cycles instead of one.
        assert_eq!(
            tape.pulses[1],
            TapePulse::Cycles {
                half_period_ns: ZERO_HALF,
                count: 4
            }
        );
    }

    #[test]
    fn unknown_chunks_are_recorded_not_fatal() {
        let tape = parse(&build(&[
            (0x0000, b"origin info".to_vec()),
            (0x0110, 2u16.to_le_bytes().to_vec()),
        ]))
        .expect("unknown-chunk tape parses");
        assert_eq!(tape.skipped_chunks, vec![0x0000]);
        assert_eq!(tape.pulses.len(), 1);
    }

    #[test]
    fn truncated_chunk_is_an_error() {
        let mut image = build(&[]);
        // A chunk header claiming 16 payload bytes with none present.
        image.extend_from_slice(&0x0110u16.to_le_bytes());
        image.extend_from_slice(&16u32.to_le_bytes());
        match parse(&image).expect_err("truncated chunk must fail") {
            UefError::TruncatedChunk { id, length, .. } => {
                assert_eq!(id, 0x0110);
                assert_eq!(length, 16);
            }
            other => panic!("expected TruncatedChunk, got {other:?}"),
        }
    }

    #[test]
    fn gzip_and_raw_decode_identically() {
        let raw = build(&[
            (0x0110, 100u16.to_le_bytes().to_vec()),
            (0x0100, vec![0x41, 0x42, 0x43]),
            (0x0112, 1200u16.to_le_bytes().to_vec()),
        ]);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw).expect("gzip write");
        let gzipped = encoder.finish().expect("gzip finish");

        assert_eq!(gzipped[..2], [0x1f, 0x8b]); // really compressed
        assert_eq!(
            parse(&gzipped).expect("gzip image parses"),
            parse(&raw).expect("raw image parses")
        );
    }

    #[test]
    fn total_duration_sums_spans() {
        let tape = parse(&build(&[
            (0x0110, 1u16.to_le_bytes().to_vec()), // one 2400 Hz cycle = 2 * 208_333 ns
            (0x0112, 2400u16.to_le_bytes().to_vec()), // one second gap
        ]))
        .expect("duration tape parses");
        assert_eq!(
            tape.total_duration_ns(),
            2 * u64::from(ONE_HALF) + 1_000_000_000
        );
    }
}
