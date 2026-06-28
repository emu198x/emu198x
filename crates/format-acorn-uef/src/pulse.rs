//! The machine-facing waveform produced by decoding a UEF tape.
//!
//! UEF stores Kansas-City-format audio: a `0` bit is one cycle of the base
//! frequency (1200 Hz by default) and a `1` bit is two cycles at twice that
//! frequency (2400 Hz). Carrier tone is a continuous run of the high frequency.
//! Rather than emit raw PCM (as MAME does at a fixed 4800 Hz), the decoder emits
//! a compact, clock-neutral stream of [`TapePulse`] spans measured in
//! nanoseconds. Each Acorn machine samples this stream at its own clock and lets
//! its cassette hardware recover the bits, exactly as real hardware does.

use serde::{Deserialize, Serialize};

/// One span of the cassette waveform.
///
/// Durations are in nanoseconds so the stream is independent of any machine
/// clock. A [`TapePulse::Cycles`] span is a square wave: each cycle holds the
/// line low for `half_period_ns`, then high for `half_period_ns`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapePulse {
    /// `count` square-wave cycles with the given half-period. Carrier tone and
    /// the tone bursts that encode each data bit are all expressed this way.
    Cycles {
        /// Half the period of one cycle, in nanoseconds.
        half_period_ns: u32,
        /// Number of whole cycles.
        count: u32,
    },
    /// A flat gap with no carrier for `duration_ns` nanoseconds.
    Gap {
        /// Gap length in nanoseconds.
        duration_ns: u32,
    },
}

impl TapePulse {
    /// Total duration of this span in nanoseconds.
    #[must_use]
    pub fn duration_ns(&self) -> u64 {
        match *self {
            TapePulse::Cycles {
                half_period_ns,
                count,
            } => u64::from(half_period_ns) * 2 * u64::from(count),
            TapePulse::Gap { duration_ns } => u64::from(duration_ns),
        }
    }
}

/// A decoded UEF tape: the waveform plus light diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UefTape {
    /// The cassette waveform in tape order.
    pub pulses: Vec<TapePulse>,
    /// Identifiers of chunks that were recognised but not synthesised into the
    /// waveform (metadata, or not-yet-supported), in the order encountered.
    /// Purely diagnostic.
    pub skipped_chunks: Vec<u16>,
}

impl UefTape {
    /// Returns `true` when no waveform was produced.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pulses.is_empty()
    }

    /// Total playing time of the tape in nanoseconds.
    #[must_use]
    pub fn total_duration_ns(&self) -> u64 {
        self.pulses.iter().map(TapePulse::duration_ns).sum()
    }
}
