//! The decoded UEF tape.
//!
//! The waveform itself is a [`TapePulse`] stream from the shared
//! [`common_acorn_cassette`] crate; this module adds the UEF-specific container
//! around it. Rather than emit raw PCM (as MAME does at a fixed 4800 Hz), the
//! decoder produces a compact stream of pulse spans that each Acorn machine
//! samples or demodulates at its own clock.

use common_acorn_cassette::TapePulse;
use serde::{Deserialize, Serialize};

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
