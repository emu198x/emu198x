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
    fn eject_clears_the_tape() {
        let mut receiver = CassetteReceiver::new();
        receiver.load(tape(&[0xAA]));
        assert!(receiver.is_loaded());
        receiver.eject();
        assert!(!receiver.is_loaded());
        assert!(receiver.finished());
    }
}
