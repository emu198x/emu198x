//! Shared cassette support for the Acorn family.
//!
//! Holds the clock-neutral [`TapePulse`] waveform that tape decoders (e.g. the
//! UEF parser) emit, and the [`CassetteReceiver`] Kansas-City demodulator that
//! the BBC Micro and Electron ULAs use to recover serial bytes from it. The
//! demodulator is machine-agnostic: it hands recovered bytes and carrier edges
//! to a callback, leaving each machine to wire them to its own register and
//! interrupt.

mod pulse;
mod receiver;

pub use pulse::TapePulse;
pub use receiver::{CassetteEvent, CassetteReceiver};

/// A 2400 Hz cycle's half-period is shorter than this; anything longer is the
/// 1200 Hz tone. Halfway between 208 µs (2400 Hz) and 417 µs (1200 Hz).
const TONE_THRESHOLD_NS: u32 = 312_500;
/// 300-baud Kansas City: a `1` bit is 8 cycles of 2400 Hz, a `0` is 4 of 1200 Hz.
const CYCLES_PER_ONE: u32 = 8;
const CYCLES_PER_ZERO: u32 = 4;
/// A high-tone run longer than any in-data run is a carrier leader (a block
/// boundary), not data. The longest in-data run is a `&FF` byte: 8 data `1`s
/// plus the stop bit = 9 high bits = 72 cycles, so anything past ~12 bits is
/// unambiguously carrier.
const CARRIER_MIN_CYCLES: u32 = 96;

/// Demodulate a captured 300-baud Atom cassette `SAVE` waveform into data blocks.
///
/// The Atom COS records Kansas-City at **300 baud**: each bit spans ~3.33 ms — a
/// `1` is 8 cycles of 2400 Hz, a `0` is 4 cycles of 1200 Hz — framed start (0) +
/// 8 data bits (LSB first) + stop (1), with a long carrier leader before each
/// block. Tone runs are turned back into bits (`round(cycles / 8)` ones per high
/// run, `round(cycles / 4)` zeros per low run, which absorbs ±1-cycle capture
/// jitter), a leader or silence ends the current block, and each block's bits are
/// reframed into bytes — one `Vec<u8>` per block, ready for a `.uef` writer.
///
/// Distinct from the 1200-baud [`CassetteReceiver`], which the BBC Micro and
/// Electron ULAs use for LOAD; this is the Atom's SAVE-side inverse.
#[must_use]
pub fn demodulate_blocks(pulses: Vec<TapePulse>) -> Vec<Vec<u8>> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut bits: Vec<bool> = Vec::new();

    for run in tone_runs(&pulses) {
        let boundary = match run {
            Run::Silence => true,
            Run::Tone { high: true, cycles } if cycles >= CARRIER_MIN_CYCLES => true,
            Run::Tone { high, cycles } => {
                let per_bit = if high {
                    CYCLES_PER_ONE
                } else {
                    CYCLES_PER_ZERO
                };
                for _ in 0..div_round(cycles, per_bit) {
                    bits.push(high);
                }
                false
            }
        };
        if boundary {
            flush_block(&mut bits, &mut blocks);
        }
    }
    flush_block(&mut bits, &mut blocks);
    blocks
}

/// One merged stretch of the captured waveform: a same-tone cycle run or silence.
enum Run {
    Tone { high: bool, cycles: u32 },
    Silence,
}

/// Collapse the pulse stream into tone runs, merging adjacent same-tone cycles so
/// a whole carrier leader (or a multi-cycle bit) is one run.
fn tone_runs(pulses: &[TapePulse]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for pulse in pulses {
        match *pulse {
            TapePulse::Cycles {
                half_period_ns,
                count,
            } => {
                let high = half_period_ns < TONE_THRESHOLD_NS;
                match runs.last_mut() {
                    Some(Run::Tone { high: last, cycles }) if *last == high => *cycles += count,
                    _ => runs.push(Run::Tone {
                        high,
                        cycles: count,
                    }),
                }
            }
            TapePulse::Gap { .. } => runs.push(Run::Silence),
        }
    }
    runs
}

/// Frame `bits` into bytes and, if any survive, push them as one block; clears
/// `bits` either way.
fn flush_block(bits: &mut Vec<bool>, blocks: &mut Vec<Vec<u8>>) {
    let bytes = frame_bits(bits);
    if !bytes.is_empty() {
        blocks.push(bytes);
    }
    bits.clear();
}

/// Frame a block's bit stream into bytes: skip to a start bit (0), read 8 data
/// bits (LSB first), then consume the stop bit (1) if present — the final byte's
/// stop is often swallowed by the trailing carrier, so it is optional.
fn frame_bits(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < bits.len() {
        if bits[i] {
            i += 1; // carrier tail or a stray stop bit; wait for a start bit
            continue;
        }
        if i + 9 > bits.len() {
            break; // not enough left for start + 8 data bits
        }
        let mut byte = 0u8;
        for j in 0..8 {
            if bits[i + 1 + j] {
                byte |= 1 << j;
            }
        }
        bytes.push(byte);
        i += 9; // start + 8 data
        if i < bits.len() && bits[i] {
            i += 1; // stop bit
        }
    }
    bytes
}

/// Divide rounding to the nearest integer — `round(n / d)`.
fn div_round(n: u32, d: u32) -> u32 {
    (n + d / 2) / d
}

#[cfg(test)]
mod block_tests {
    use super::*;

    const ONE_HALF: u32 = 208_333;

    // 300-baud framing: a `1` is 8 cycles of 2400 Hz, a `0` is 4 of 1200 Hz.
    fn push_byte(pulses: &mut Vec<TapePulse>, byte: u8) {
        let push_bit = |pulses: &mut Vec<TapePulse>, set: bool| {
            pulses.push(if set {
                TapePulse::Cycles {
                    half_period_ns: ONE_HALF,
                    count: 8,
                }
            } else {
                TapePulse::Cycles {
                    half_period_ns: 416_667,
                    count: 4,
                }
            });
        };
        push_bit(pulses, false);
        for i in 0..8 {
            push_bit(pulses, (byte >> i) & 1 == 1);
        }
        push_bit(pulses, true);
    }

    fn block(bytes: &[u8]) -> Vec<TapePulse> {
        let mut pulses = vec![TapePulse::Cycles {
            half_period_ns: ONE_HALF,
            count: 128,
        }];
        for &byte in bytes {
            push_byte(&mut pulses, byte);
        }
        pulses
    }

    #[test]
    fn one_carrier_one_block() {
        assert_eq!(
            demodulate_blocks(block(&[0x2A, 0x41])),
            vec![vec![0x2A, 0x41]]
        );
    }

    #[test]
    fn each_carrier_leader_starts_a_new_block() {
        let mut pulses = block(&[0x12, 0x34]);
        pulses.push(TapePulse::Gap {
            duration_ns: 5_000_000,
        });
        pulses.extend(block(&[0x56]));
        assert_eq!(
            demodulate_blocks(pulses),
            vec![vec![0x12, 0x34], vec![0x56]]
        );
    }

    #[test]
    fn silence_yields_no_blocks() {
        assert!(demodulate_blocks(vec![TapePulse::Gap { duration_ns: 1_000 }]).is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_HALF: u32 = 416_667; // 1200 Hz half-period
    const ONE_HALF: u32 = 208_333; // 2400 Hz half-period

    fn push_bit(pulses: &mut Vec<TapePulse>, set: bool) {
        if set {
            pulses.push(TapePulse::Cycles {
                half_period_ns: ONE_HALF,
                count: 2,
            });
        } else {
            pulses.push(TapePulse::Cycles {
                half_period_ns: ZERO_HALF,
                count: 1,
            });
        }
    }

    fn push_byte(pulses: &mut Vec<TapePulse>, byte: u8) {
        push_bit(pulses, false); // start
        for i in 0..8 {
            push_bit(pulses, (byte >> i) & 1 == 1);
        }
        push_bit(pulses, true); // stop
    }

    /// A tape: a carrier leader long enough to trip high-tone detection, then
    /// the framed bytes back to back.
    fn tape(bytes: &[u8]) -> Vec<TapePulse> {
        let mut pulses = vec![TapePulse::Cycles {
            half_period_ns: ONE_HALF,
            count: 128,
        }];
        for &byte in bytes {
            push_byte(&mut pulses, byte);
        }
        pulses
    }

    /// Plays a loaded receiver to the end in 500 ns (one 2 MHz tick) steps,
    /// collecting every event.
    fn play(receiver: &mut CassetteReceiver) -> Vec<CassetteEvent> {
        let mut events = Vec::new();
        let mut guard = 0u32;
        while !receiver.finished() {
            receiver.advance(500, &mut |event| events.push(event));
            guard += 1;
            assert!(guard < 10_000_000, "receiver did not finish");
        }
        events
    }

    fn bytes_of(events: &[CassetteEvent]) -> Vec<u8> {
        events
            .iter()
            .filter_map(|event| match event {
                CassetteEvent::ByteReady(byte) => Some(*byte),
                CassetteEvent::HighTone => None,
            })
            .collect()
    }

    #[test]
    fn recovers_framed_bytes() {
        let mut receiver = CassetteReceiver::new();
        receiver.load(tape(&[0x41, 0x42, 0x00, 0xFF, 0x55]));
        let events = play(&mut receiver);
        assert_eq!(bytes_of(&events), vec![0x41, 0x42, 0x00, 0xFF, 0x55]);
    }

    #[test]
    fn detects_carrier_once_per_leader() {
        let mut receiver = CassetteReceiver::new();
        receiver.load(tape(&[0x01]));
        let events = play(&mut receiver);
        let high_tones = events
            .iter()
            .filter(|event| matches!(event, CassetteEvent::HighTone))
            .count();
        assert_eq!(high_tones, 1);
        // The carrier edge is reported before the first byte arrives.
        assert!(matches!(events.first(), Some(CassetteEvent::HighTone)));
    }

    #[test]
    fn a_gap_between_blocks_resyncs_cleanly() {
        let mut pulses = tape(&[0x12]);
        pulses.push(TapePulse::Gap {
            duration_ns: 5_000_000,
        });
        pulses.extend(tape(&[0x34]));
        let mut receiver = CassetteReceiver::new();
        receiver.load(pulses);
        let events = play(&mut receiver);
        assert_eq!(bytes_of(&events), vec![0x12, 0x34]);
    }

    #[test]
    fn empty_receiver_yields_nothing() {
        let mut receiver = CassetteReceiver::new();
        assert!(!receiver.is_loaded());
        assert!(receiver.finished());
        let mut fired = false;
        receiver.advance(1_000_000, &mut |_| fired = true);
        assert!(!fired);
    }

    #[test]
    fn level_tracks_the_square_wave() {
        let mut receiver = CassetteReceiver::new();
        // One cycle with a 1 µs half-period: low for the first half, high for
        // the second.
        receiver.load(vec![TapePulse::Cycles {
            half_period_ns: 1000,
            count: 1,
        }]);
        assert!(!receiver.level(), "each cycle starts low");
        receiver.advance(500, &mut |_| {});
        assert!(!receiver.level(), "still in the low half");
        receiver.advance(600, &mut |_| {}); // 1100 ns in
        assert!(receiver.level(), "now in the high half");
        receiver.advance(1000, &mut |_| {}); // past the cycle
        assert!(!receiver.level(), "end of tape reads low");
    }

    #[test]
    fn level_is_low_with_no_tape() {
        let receiver = CassetteReceiver::new();
        assert!(!receiver.level());
    }

    #[test]
    fn eject_clears_the_tape() {
        let mut receiver = CassetteReceiver::new();
        receiver.load(tape(&[0xAA]));
        assert!(receiver.is_loaded());
        receiver.eject();
        assert!(!receiver.is_loaded());
        assert!(receiver.finished());
    }
}
