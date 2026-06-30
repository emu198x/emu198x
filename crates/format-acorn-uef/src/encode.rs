//! UEF writer — the inverse of [`crate::parse`], used by cassette `SAVE`.
//!
//! Encodes recovered tape data blocks as a plain (uncompressed) UEF image so a
//! `SAVE` can be persisted and loaded back. Each block is a carrier-tone leader
//! (chunk `&0110`) followed by an implicit-data block (chunk `&0100`), the two
//! chunk types [`crate::parse`] reconstructs into the Kansas-City waveform.

/// UEF container magic (10 bytes, NUL-terminated).
const MAGIC: &[u8] = b"UEF File!\0";

/// Encode tape data blocks as a UEF image.
///
/// `leader_cycles` is the length of each block's carrier tone, in 2400 Hz cycles
/// — long enough to trip the receiver's carrier detection on load (a few hundred
/// is ample). The result round-trips through [`crate::parse`].
#[must_use]
pub fn encode_blocks(blocks: &[Vec<u8>], leader_cycles: u16) -> Vec<u8> {
    let mut image = MAGIC.to_vec();
    image.extend_from_slice(&[0x0a, 0x00]); // version 0.10
    for block in blocks {
        push_chunk(&mut image, 0x0110, &leader_cycles.to_le_bytes());
        push_chunk(&mut image, 0x0100, block);
    }
    image
}

/// Append one UEF chunk: id (`u16` LE), length (`u32` LE), then the payload.
fn push_chunk(image: &mut Vec<u8>, id: u16, payload: &[u8]) {
    image.extend_from_slice(&id.to_le_bytes());
    let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    image.extend_from_slice(&len.to_le_bytes());
    image.extend_from_slice(payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_acorn_cassette::{TapePulse, demodulate_blocks};

    /// Frame bytes into a Kansas-City waveform the way the COS bit-bangs a SAVE:
    /// a carrier leader, then each byte as start(0) + 8 data bits (LSB first) +
    /// stop(1). Stands in for the machine's captured cassette output.
    fn captured_save(blocks: &[&[u8]]) -> Vec<TapePulse> {
        const ONE_HALF: u32 = 208_333; // 2400 Hz
        const ZERO_HALF: u32 = 416_667; // 1200 Hz
        let bit = |pulses: &mut Vec<TapePulse>, set: bool| {
            pulses.push(if set {
                TapePulse::Cycles {
                    half_period_ns: ONE_HALF,
                    count: 2,
                }
            } else {
                TapePulse::Cycles {
                    half_period_ns: ZERO_HALF,
                    count: 1,
                }
            });
        };
        let mut pulses = Vec::new();
        for block in blocks {
            pulses.push(TapePulse::Cycles {
                half_period_ns: ONE_HALF,
                count: 256,
            }); // leader
            for &byte in *block {
                bit(&mut pulses, false);
                for i in 0..8 {
                    bit(&mut pulses, (byte >> i) & 1 == 1);
                }
                bit(&mut pulses, true);
            }
            pulses.push(TapePulse::Gap {
                duration_ns: 2_000_000,
            });
        }
        pulses
    }

    #[test]
    fn a_saved_program_round_trips_through_load() {
        // The full SAVE -> LOAD data path: a captured cassette-output waveform is
        // demodulated to blocks, written as a UEF, then parsed and demodulated
        // again (as LOAD would) — and the bytes survive intact.
        let program: &[u8] = &[0x2A, 0x12, 0x29, 0x00, 0x48, 0x49, 0xC3];
        let header: &[u8] = &[0x2A, b'Z', 0x00, 0x29];

        let captured = captured_save(&[header, program]);
        let saved_blocks = demodulate_blocks(captured); // the SAVE write path
        let uef = encode_blocks(&saved_blocks, 256); // the .uef writer

        let reloaded = demodulate_blocks(crate::parse(&uef).expect("UEF parses").pulses);
        assert_eq!(reloaded, vec![header.to_vec(), program.to_vec()]);
    }

    #[test]
    fn round_trips_through_parse_and_demodulate() {
        // Two carrier-separated blocks survive encode -> parse -> demodulate.
        let blocks = vec![vec![0x2A, 0x41, 0x42, 0xFF], vec![0x2A, 0x99, 0x00]];
        let uef = encode_blocks(&blocks, 256);
        let tape = crate::parse(&uef).expect("encoded UEF parses");
        let recovered = common_acorn_cassette::demodulate_blocks(tape.pulses);
        assert_eq!(recovered, blocks);
    }

    #[test]
    fn output_carries_the_uef_magic() {
        let uef = encode_blocks(&[vec![0x01]], 16);
        assert!(uef.starts_with(b"UEF File!\0"));
    }
}
