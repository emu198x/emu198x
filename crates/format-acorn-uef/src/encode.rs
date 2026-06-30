//! UEF writer — the inverse of [`crate::parse`], used by cassette `SAVE`.
//!
//! Encodes recovered tape data blocks as a plain (uncompressed) UEF image so a
//! `SAVE` can be persisted and loaded back. Each block is a carrier-tone leader
//! (chunk `&0110`) followed by an implicit-data block (chunk `&0100`), the two
//! chunk types [`crate::parse`] reconstructs into the Kansas-City waveform.

/// UEF container magic (10 bytes, NUL-terminated).
const MAGIC: &[u8] = b"UEF File!\0";

/// Inter-block silence written after each block, as the integer-gap chunk
/// (`&0112`) the parser reads. Its unit is the 1200 Hz base half-period, so
/// 2400 ≈ 1 s; ~0.3 s separates blocks like a real tape and lets a loader resync.
const INTER_BLOCK_GAP_UNITS: u16 = 720;

/// Encode tape data blocks as a UEF image.
///
/// Opens with a baud-rate chunk (`&0117` = 300), then each block is a carrier-tone
/// leader (`&0110`, `leader_cycles` 2400 Hz cycles — long enough to trip carrier
/// detection on load), the data (`&0100`), then a silent gap (`&0112`) before the
/// next block. The result round-trips through [`crate::parse`].
///
/// The Atom records at **300 baud** (a `1` is 8 cycles of 2400 Hz, a `0` is 4 of
/// 1200 Hz), so the baud chunk is essential: [`crate::parse`] defaults to 1200
/// baud and would otherwise frame each byte four times too fast for the Atom COS.
#[must_use]
pub fn encode_blocks(blocks: &[Vec<u8>], leader_cycles: u16) -> Vec<u8> {
    const ATOM_BAUD: u16 = 300;
    let mut image = MAGIC.to_vec();
    image.extend_from_slice(&[0x0a, 0x00]); // version 0.10
    push_chunk(&mut image, 0x0117, &ATOM_BAUD.to_le_bytes());
    for block in blocks {
        push_chunk(&mut image, 0x0110, &leader_cycles.to_le_bytes());
        push_chunk(&mut image, 0x0100, block);
        push_chunk(&mut image, 0x0112, &INTER_BLOCK_GAP_UNITS.to_le_bytes());
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

    /// Frame bytes into a 300-baud Kansas-City waveform the way the COS bit-bangs
    /// a SAVE: a carrier leader, then each byte as start(0) + 8 data bits (LSB
    /// first) + stop(1), a `1` being 8 cycles of 2400 Hz and a `0` four of 1200 Hz.
    /// Stands in for the machine's captured cassette output.
    fn captured_save(blocks: &[&[u8]]) -> Vec<TapePulse> {
        const ONE_HALF: u32 = 208_333; // 2400 Hz
        const ZERO_HALF: u32 = 416_667; // 1200 Hz
        let bit = |pulses: &mut Vec<TapePulse>, set: bool| {
            pulses.push(if set {
                TapePulse::Cycles {
                    half_period_ns: ONE_HALF,
                    count: 8,
                }
            } else {
                TapePulse::Cycles {
                    half_period_ns: ZERO_HALF,
                    count: 4,
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

    #[test]
    fn blocks_are_separated_by_gaps() {
        // Each block is followed by a silent gap, so the parsed waveform carries a
        // TapePulse::Gap between blocks (the timing structure a real tape has).
        let uef = encode_blocks(&[vec![0x01], vec![0x02]], 256);
        let gaps = crate::parse(&uef)
            .expect("parses")
            .pulses
            .iter()
            .filter(|p| matches!(p, TapePulse::Gap { .. }))
            .count();
        assert_eq!(gaps, 2, "one gap chunk per block");
    }
}
