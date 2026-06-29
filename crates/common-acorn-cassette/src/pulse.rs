//! The clock-neutral cassette waveform shared across Acorn machines.
//!
//! A tape decoder (e.g. the UEF parser) produces a [`TapePulse`] stream; each
//! machine samples or demodulates it at its own clock. Durations are in
//! nanoseconds so the stream is independent of any machine clock.

use serde::{Deserialize, Serialize};

/// One span of the cassette waveform.
///
/// A [`TapePulse::Cycles`] span is a square wave: each cycle holds the line low
/// for `half_period_ns`, then high for `half_period_ns`. In the Kansas-City
/// scheme a `0` bit is one cycle at the base frequency (1200 Hz) and a `1` bit
/// is two cycles at twice that (2400 Hz); carrier tone is a continuous run of
/// the high frequency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapePulse {
    /// `count` square-wave cycles with the given half-period.
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
