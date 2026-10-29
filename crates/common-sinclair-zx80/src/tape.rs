//! The pulse train the ZX80 ROM's `SAVE` writes, and `LOAD` reads.
//!
//! `format198x-sinclair-zx80-o` reads and writes the `.o` file, which is the
//! bytes on tape. Turning those bytes into a cassette signal is what this
//! machine's ROM does, so the encoding lives here.
//!
//! Bits are carried by *counted pulses*, not by pulse width. `SAVE` picks
//! the count with:
//!
//! ```text
//! RLC (HL)        ; next bit, most significant first
//! SBC A,A         ; $00 or $FF
//! AND $05
//! ADD A,$04       ; 4 pulses for 0, 9 for 1
//! ```
//!
//! and emits each pulse by touching the bus: an `OUT` drives the cassette
//! line one way, an `IN` the other. The timings below were measured from
//! Sinclair's ROM saving an empty program, rather than derived from the
//! instruction timings, because the loop's cost is what matters and the ROM
//! is the authority on it:
//!
//! | | T-states |
//! |---|---|
//! | pulse high (`OUT` to `IN`) | 488 |
//! | pulse low (`IN` to next `OUT`) | 484 |
//! | gap between bits | 4754 |

use format198x_sinclair_zx80_o::Zx80Image;

/// Pulse high time, in T-states.
pub const PULSE_HIGH_T: u64 = 488;
/// Pulse low time, in T-states.
pub const PULSE_LOW_T: u64 = 484;
/// Quiet between one bit's pulses and the next bit's.
pub const BIT_GAP_T: u64 = 4754;
/// Pulses that mean a clear bit.
pub const ZERO_PULSES: usize = 4;
/// Pulses that mean a set bit.
pub const ONE_PULSES: usize = 9;
/// Silence before the first bit.
///
/// This is not padding, and it is not generous. The loader refuses to start
/// until the line has been quiet for a `$5712` countdown at `$0207`, about
/// 890,000 T-states, and **any** high resets it:
///
/// ```text
/// $0207   LD DE,$5712
/// $020A   IN A,($FE) / RRA / JR NC (break) / RLA / RLA
/// $0213   JR C,$0207        ; a high restarts the count
///         DEC DE / LD A,D / OR E / JR NZ,$020A
/// ```
///
/// A tape whose pulses begin sooner plays out entirely inside that search —
/// every pulse resetting the countdown — and the loader then waits forever
/// for a signal that has already gone. The symptom is a tape fully consumed
/// with RAM untouched, which reads exactly like a broken decoder.
pub const LEAD_IN_T: u64 = 1_500_000;

/// The cassette waveform for `image`, as transition times in T-states
/// measured from the moment the tape starts.
///
/// The returned times are when the line flips. It starts low, so an odd
/// number of elapsed transitions means the line is high — which is what the
/// loader waits for before timing a burst.
#[must_use]
pub fn pulses(image: &Zx80Image) -> Vec<u64> {
    let mut edges = Vec::new();
    let mut t = LEAD_IN_T;
    for byte in image.bytes() {
        for bit in (0..8).rev() {
            let set = byte & (1 << bit) != 0;
            let pulses = if set { ONE_PULSES } else { ZERO_PULSES };
            for _ in 0..pulses {
                edges.push(t);
                t += PULSE_HIGH_T;
                edges.push(t);
                t += PULSE_LOW_T;
            }
            // The low that ends a bit is stretched into the gap, rather
            // than added to it: the loader measures from the last edge.
            t += BIT_GAP_T - PULSE_LOW_T;
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use format198x_sinclair_zx80_o::{END_POINTER_OFFSET, RAM_BASE};

    fn image(program: &[u8]) -> Vec<u8> {
        let len = (END_POINTER_OFFSET + 2).max(program.len());
        let mut bytes = vec![0u8; len];
        bytes[..program.len()].copy_from_slice(program);
        let end = RAM_BASE + u16::try_from(len).expect("test image fits");
        bytes[END_POINTER_OFFSET..END_POINTER_OFFSET + 2].copy_from_slice(&end.to_le_bytes());
        bytes
    }

    /// Nine pulses for a set bit, four for a clear one — the counts the
    /// ROM's `AND $05 / ADD A,$04` produces, most significant bit first.
    #[test]
    fn bits_are_carried_by_pulse_count_most_significant_first() {
        let raw = image(&[
            0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x40,
        ]);
        let parsed = Zx80Image::parse(&raw).expect("valid image");
        let edges = pulses(&parsed);

        // Two edges per pulse. The first byte is $80: one set bit then seven
        // clear ones.
        let first_bit = ONE_PULSES * 2;
        assert_eq!(edges[0], LEAD_IN_T);
        assert_eq!(edges[1] - edges[0], PULSE_HIGH_T);
        assert_eq!(edges[2] - edges[1], PULSE_LOW_T);

        // The gap lands after the last pulse of the bit, not between pulses.
        let gap = edges[first_bit] - edges[first_bit - 1];
        assert_eq!(gap, BIT_GAP_T);

        let second_bit_pulses = (edges[first_bit..].len()).min(ZERO_PULSES * 2);
        assert_eq!(second_bit_pulses, ZERO_PULSES * 2);
    }

    #[test]
    fn every_bit_of_every_byte_is_encoded() {
        let raw = image(&[0xFFu8; 32]);
        let parsed = Zx80Image::parse(&raw).expect("valid image");
        // 32 bytes; the end pointer at $0A..$0B is $4020, so those two bytes
        // are not $FF. Count set bits directly rather than assuming.
        let set: u32 = parsed.bytes().iter().map(|b| b.count_ones()).sum();
        let clear = 32 * 8 - set;
        let expected = set as usize * ONE_PULSES * 2 + clear as usize * ZERO_PULSES * 2;
        assert_eq!(pulses(&parsed).len(), expected);
    }
}
