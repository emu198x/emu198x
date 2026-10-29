//! The pulse train the ZX81 ROM's `SAVE` writes, and `LOAD` reads.
//!
//! `format198x-sinclair-zx81-p` reads and writes the `.p` file, which is the
//! bytes on tape after the name. Turning the name and those bytes into a
//! cassette signal is what this machine's ROM does, so the encoding lives here.
//!
//! The ROM emits each pulse by touching the bus -- an `OUT` drives the
//! cassette line one way, an `IN` the other -- so the waveform is the times at
//! which `SAVE` did that.
//!
//! The timings below were measured from Sinclair's own ROM, by recording the
//! port accesses while it saved an empty program under the name `A`. Saving
//! with an empty name does not work on this machine: the ROM reports `F`, a
//! file-name error, and never reaches the tape.
//!
//! | | T-states | Occurrences in that recording |
//! |---|---|---|
//! | pulse high (`OUT` to `IN`) | 492 | 13,953 |
//! | pulse low (`IN` to next `OUT`) | 483 | 10,681 |
//! | gap between bits | 4,872 | 2,863 |
//! | gap at a byte boundary | 5,008 | 408 |
//!
//! The bit counts in the same recording were 3,099 four-pulse runs and 173
//! nine-pulse runs -- 3,272 bits, or 409 bytes, matching the 408 byte-boundary
//! gaps.
//!
//! The ZX80's figures are 488 / 484 / 4,754 for the same three, which is the
//! corroboration available: two machines a year apart, the same technique in
//! the same house, landing within ten T-states.

use format198x_sinclair_zx81_p::Zx81Image;

/// Pulse high time, in T-states.
pub const PULSE_HIGH_T: u64 = 492;
/// Pulse low time, in T-states.
pub const PULSE_LOW_T: u64 = 483;
/// Quiet between one bit's pulses and the next bit's.
pub const BIT_GAP_T: u64 = 4_872;
/// The longer quiet the ROM leaves at a byte boundary.
pub const BYTE_GAP_T: u64 = 5_008;
/// Pulses that mean a clear bit.
pub const ZERO_PULSES: usize = 4;
/// Pulses that mean a set bit.
pub const ONE_PULSES: usize = 9;

/// Silence before the first bit.
///
/// The loader will not start decoding until the line has been quiet, so the
/// train has to open with more silence than any gap inside it. Too little and
/// the countdown never completes, which looks exactly like a broken decoder:
/// the tape runs out with RAM untouched.
pub const LEAD_IN_T: u64 = 1_500_000;

/// The cassette waveform for `image` saved under `name`, as transition times
/// in T-states measured from the moment the tape is threaded.
///
/// `name` is the program's name in ZX81 character codes, not ASCII. The ROM
/// marks the end of the name by setting bit 7 of its last character, which
/// this does for you, so pass the name unmarked. An empty name is allowed
/// here even though `SAVE ""` is not: the ROM refuses to *write* one, but
/// `LOAD ""` will take whatever it finds.
#[must_use]
pub fn pulses(image: &Zx81Image, name: &[u8]) -> Vec<u64> {
    let mut edges = Vec::new();
    let mut t = LEAD_IN_T;

    let mut stream = Vec::with_capacity(name.len() + image.bytes().len());
    stream.extend_from_slice(name);
    if let Some(last) = stream.last_mut() {
        *last |= 0x80;
    }
    stream.extend_from_slice(image.bytes());

    for byte in &stream {
        for bit in (0..8).rev() {
            let pulses = if byte & (1 << bit) != 0 {
                ONE_PULSES
            } else {
                ZERO_PULSES
            };
            for _ in 0..pulses {
                edges.push(t);
                t += PULSE_HIGH_T;
                edges.push(t);
                t += PULSE_LOW_T;
            }
            // The low that ends a bit is stretched into the gap rather
            // than added to it: the loader measures from the last edge.
            t += BIT_GAP_T - PULSE_LOW_T;
        }
        // The ROM leaves a little longer at a byte boundary.
        t += BYTE_GAP_T - BIT_GAP_T;
    }
    edges
}
