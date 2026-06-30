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

/// Demodulate a captured cassette waveform into its data blocks.
///
/// Runs the [`CassetteReceiver`] over `pulses` and splits the recovered bytes at
/// each carrier leader ([`CassetteEvent::HighTone`]) — so a `SAVE` that writes
/// several carrier-separated blocks comes back as one `Vec<u8>` per block, ready
/// for a `.uef`/`.tap` writer. The inverse of playing a loaded tape.
#[must_use]
pub fn demodulate_blocks(pulses: Vec<TapePulse>) -> Vec<Vec<u8>> {
    let mut receiver = CassetteReceiver::new();
    receiver.load(pulses);
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    while !receiver.finished() {
        receiver.advance(1000, &mut |event| match event {
            // A fresh carrier leader starts a new block; flush the previous one.
            CassetteEvent::HighTone => {
                if !current.is_empty() {
                    blocks.push(std::mem::take(&mut current));
                }
            }
            CassetteEvent::ByteReady(byte) => current.push(byte),
        });
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

#[cfg(test)]
mod block_tests {
    use super::*;

    const ONE_HALF: u32 = 208_333;

    fn push_byte(pulses: &mut Vec<TapePulse>, byte: u8) {
        let push_bit = |pulses: &mut Vec<TapePulse>, set: bool| {
            pulses.push(if set {
                TapePulse::Cycles {
                    half_period_ns: ONE_HALF,
                    count: 2,
                }
            } else {
                TapePulse::Cycles {
                    half_period_ns: 416_667,
                    count: 1,
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
