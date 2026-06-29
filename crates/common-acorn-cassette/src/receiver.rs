//! Kansas-City cassette demodulator.
//!
//! Recovers serial bytes from a [`TapePulse`] waveform exactly as an Acorn ULA
//! does in silicon: it classifies each square-wave cycle as the high (2400 Hz)
//! or low (1200 Hz) tone, groups them into Kansas-City bits — a `0` is one low
//! cycle, a `1` is two high cycles — and frames them as start / 8 data (LSB
//! first) / stop. Sustained high tone is reported as a carrier (high-tone)
//! event, which the OS waits for before reading a block.
//!
//! The receiver is driven in machine time: the caller advances it by however
//! many nanoseconds have elapsed (gated by the cassette motor relay) and is
//! handed each recovered byte and carrier edge through a callback, so it stays
//! agnostic to which register / interrupt the consuming machine wires them to.

use serde::{Deserialize, Serialize};

use crate::pulse::TapePulse;

/// Half-period threshold separating the high tone (≈208 µs) from the low tone
/// (≈417 µs). Anything shorter is a 2400 Hz cycle, anything longer 1200 Hz.
const HIGH_LOW_THRESHOLD_NS: u32 = 312_500;

/// Consecutive high-tone cycles that constitute a carrier. A `0xFF` data byte
/// contains at most eighteen consecutive high cycles, so this sits safely above
/// any in-data run and well below a real multi-thousand-cycle leader.
const CARRIER_CYCLES: u32 = 64;

/// An event recovered from the tape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CassetteEvent {
    /// A framed byte was received and is ready to read.
    ByteReady(u8),
    /// Sustained carrier (high tone) was detected — the leading edge only.
    HighTone,
}

/// Framing state of the serial receiver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum RxState {
    /// Hunting for a start bit; high cycles here are carrier, not data.
    #[default]
    Hunt,
    /// Collecting the eight data bits.
    Data,
    /// Expecting the stop bit.
    Stop,
}

/// A Kansas-City cassette tape demodulator.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CassetteReceiver {
    pulses: Vec<TapePulse>,
    /// Index of the span currently playing.
    span_idx: usize,
    /// Whole cycles already consumed within the current `Cycles` span.
    cycle_in_span: u32,
    /// Nanoseconds elapsed within the current cycle or gap.
    elapsed_ns: u64,
    state: RxState,
    /// Unpaired high cycles seen while assembling the current data/stop bit.
    pending_high: u8,
    /// Number of data bits collected so far (0..8).
    bit_index: u8,
    /// The byte being assembled, LSB first.
    shift: u8,
    /// Consecutive high cycles seen while hunting (carrier run length).
    high_run: u32,
}

impl CassetteReceiver {
    /// Creates an empty receiver with no tape loaded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a tape waveform, rewinding to the start.
    pub fn load(&mut self, pulses: Vec<TapePulse>) {
        self.pulses = pulses;
        self.rewind();
    }

    /// Ejects the tape and clears all demodulator state.
    pub fn eject(&mut self) {
        self.pulses.clear();
        self.rewind();
    }

    /// Returns `true` when a tape is loaded.
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        !self.pulses.is_empty()
    }

    /// Returns `true` once the tape has played to the end.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.span_idx >= self.pulses.len()
    }

    /// The current cassette line level — the raw square-wave sample at the
    /// playback position. For machines that bit-bang the waveform in software
    /// (the Acorn Atom reads it on PPI PC4) rather than demodulating to bytes:
    /// advance the receiver each tick, then sample this. Each cycle holds the
    /// line low for its first half-period then high; gaps and the end of the
    /// tape read low.
    #[must_use]
    pub fn level(&self) -> bool {
        match self.pulses.get(self.span_idx) {
            Some(&TapePulse::Cycles { half_period_ns, .. }) => {
                self.elapsed_ns >= u64::from(half_period_ns)
            }
            _ => false,
        }
    }

    /// Rewinds to the start of the tape and resets the demodulator.
    fn rewind(&mut self) {
        self.span_idx = 0;
        self.cycle_in_span = 0;
        self.elapsed_ns = 0;
        self.reset_framing();
    }

    /// Drops carrier and any partially-received byte (gap, or rewind).
    fn reset_framing(&mut self) {
        self.state = RxState::Hunt;
        self.pending_high = 0;
        self.bit_index = 0;
        self.shift = 0;
        self.high_run = 0;
    }

    /// Advances playback by `ns` nanoseconds, calling `on_event` for each
    /// recovered byte and carrier edge in tape order. The caller gates this on
    /// the cassette motor relay; a tape that has finished produces nothing.
    pub fn advance<F: FnMut(CassetteEvent)>(&mut self, ns: u64, on_event: &mut F) {
        let mut remaining = ns;
        while remaining > 0 {
            let Some(&span) = self.pulses.get(self.span_idx) else {
                break; // tape ended
            };
            match span {
                TapePulse::Cycles {
                    half_period_ns,
                    count,
                } => {
                    let cycle_ns = u64::from(half_period_ns) * 2;
                    if cycle_ns == 0 || count == 0 {
                        self.next_span();
                        continue;
                    }
                    let cycle_remaining = cycle_ns - self.elapsed_ns;
                    if remaining < cycle_remaining {
                        self.elapsed_ns += remaining;
                        return;
                    }
                    remaining -= cycle_remaining;
                    self.elapsed_ns = 0;
                    self.feed_cycle(half_period_ns < HIGH_LOW_THRESHOLD_NS, on_event);
                    self.cycle_in_span += 1;
                    if self.cycle_in_span >= count {
                        self.next_span();
                    }
                }
                TapePulse::Gap { duration_ns } => {
                    if self.elapsed_ns == 0 {
                        self.reset_framing();
                    }
                    let gap_remaining = u64::from(duration_ns) - self.elapsed_ns;
                    if remaining < gap_remaining {
                        self.elapsed_ns += remaining;
                        return;
                    }
                    remaining -= gap_remaining;
                    self.next_span();
                }
            }
        }
    }

    /// Advances to the next span, resetting the within-span counters.
    fn next_span(&mut self) {
        self.span_idx += 1;
        self.cycle_in_span = 0;
        self.elapsed_ns = 0;
    }

    /// Feeds one classified cycle into the framing state machine.
    fn feed_cycle<F: FnMut(CassetteEvent)>(&mut self, high: bool, on_event: &mut F) {
        match self.state {
            RxState::Hunt => {
                if high {
                    self.high_run += 1;
                    if self.high_run == CARRIER_CYCLES {
                        on_event(CassetteEvent::HighTone);
                    }
                } else {
                    // A low cycle is the start bit; begin a byte.
                    self.high_run = 0;
                    self.pending_high = 0;
                    self.bit_index = 0;
                    self.shift = 0;
                    self.state = RxState::Data;
                }
            }
            RxState::Data => {
                if high {
                    self.pending_high += 1;
                    if self.pending_high == 2 {
                        self.pending_high = 0;
                        self.push_data_bit(true);
                    }
                } else if self.pending_high != 0 {
                    // An odd high before a low is a framing glitch; resync.
                    self.reset_framing();
                } else {
                    self.push_data_bit(false);
                }
            }
            RxState::Stop => {
                if high {
                    self.pending_high += 1;
                    if self.pending_high == 2 {
                        self.pending_high = 0;
                        on_event(CassetteEvent::ByteReady(self.shift));
                        self.reset_framing();
                    }
                } else {
                    // No proper stop bit: deliver what we have, then treat this
                    // low as the next byte's start bit.
                    on_event(CassetteEvent::ByteReady(self.shift));
                    self.pending_high = 0;
                    self.bit_index = 0;
                    self.shift = 0;
                    self.high_run = 0;
                    self.state = RxState::Data;
                }
            }
        }
    }

    /// Records one recovered data bit (LSB first); moves to the stop bit after
    /// the eighth.
    fn push_data_bit(&mut self, set: bool) {
        if set {
            self.shift |= 1 << self.bit_index;
        }
        self.bit_index += 1;
        if self.bit_index == 8 {
            self.pending_high = 0;
            self.state = RxState::Stop;
        }
    }
}
