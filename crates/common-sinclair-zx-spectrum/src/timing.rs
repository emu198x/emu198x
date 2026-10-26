//! Timing constants for the ZX Spectrum family.
//!
//! The frame timing data and the `FrameTiming` descriptor ULAs report are
//! `emu198x-bus-sinclair-zx-spectrum`'s, re-exported here so existing paths
//! keep working. This module adds what only machines use: a position within
//! the frame, and the 48K's contended-address test.

pub use emu198x_bus_sinclair_zx_spectrum::timing::*;

/// A position within a machine's master-clock frame.
///
/// This is the migration seam for replacing anonymous `hc` counters. The
/// value is always normalised to `0..timing.halfcycles_per_frame`; conversion
/// to T-states is deliberately explicit because divide-by-five machines have
/// master ticks that do not land on CPU edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePosition {
    halfcycles: u32,
}

impl FramePosition {
    #[must_use]
    pub const fn new(halfcycles: u32, timing: &FrameTiming) -> Self {
        Self {
            halfcycles: halfcycles % timing.halfcycles_per_frame,
        }
    }

    #[must_use]
    pub const fn halfcycles(self) -> u32 {
        self.halfcycles
    }

    #[must_use]
    pub const fn tstate(self, timing: &FrameTiming) -> u32 {
        timing.hc_to_tstates(self.halfcycles)
    }

    /// Project scheduled CPU half-cycles before rounding to a raster T-state.
    /// The driver schedules edges at master phases 0 and divisor / 2, including
    /// odd divisors. This predicts an unstalled clock, not future contention.
    #[must_use]
    pub const fn tstate_after_cpu_halfcycles(self, lead: u32, timing: &FrameTiming) -> u32 {
        let divisor = timing.cpu_divisor;
        let edge = if self.halfcycles % divisor >= divisor / 2 {
            1
        } else {
            0
        };
        ((self.halfcycles / divisor * 2 + edge + lead) / 2) % timing.tstates_per_frame
    }

    #[must_use]
    pub const fn line(self, timing: &FrameTiming) -> u32 {
        self.tstate(timing) / timing.tstates_per_line
    }

    #[must_use]
    pub const fn tstate_in_line(self, timing: &FrameTiming) -> u32 {
        self.tstate(timing) % timing.tstates_per_line
    }

    #[must_use]
    pub const fn advance(self, halfcycles: u32, timing: &FrameTiming) -> Self {
        Self::new(self.halfcycles.wrapping_add(halfcycles), timing)
    }
}

/// Returns `true` if the address lies in the 48K machine's contended RAM.
#[must_use]
pub const fn is_contended_address_48k(addr: u16) -> bool {
    addr >= 0x4000 && addr <= 0x7fff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contended_range_is_screen_ram_bank() {
        assert!(!is_contended_address_48k(0x3fff));
        assert!(is_contended_address_48k(0x4000));
        assert!(is_contended_address_48k(0x7fff));
        assert!(!is_contended_address_48k(0x8000));
    }

    #[test]
    fn frame_position_normalises_and_reports_geometry() {
        let pos = FramePosition::new(TIMING_128K.halfcycles_per_frame + 1200, &TIMING_128K);
        assert_eq!(pos.halfcycles(), 1200);
        assert_eq!(pos.tstate(&TIMING_128K), 240);
        assert_eq!(pos.line(&TIMING_128K), 1);
        assert_eq!(pos.tstate_in_line(&TIMING_128K), 12);
    }

    #[test]
    fn cpu_lead_preserves_edge_phase_and_wraps_for_even_and_odd_divisors() {
        for timing in [&TIMING_48K, &TIMING_128K] {
            let start = timing.cpu_divisor * 100;
            assert_eq!(
                FramePosition::new(start, timing).tstate_after_cpu_halfcycles(5, timing),
                102
            );
            assert_eq!(
                FramePosition::new(start + timing.cpu_divisor / 2, timing)
                    .tstate_after_cpu_halfcycles(5, timing),
                103
            );
            let last_edge =
                timing.halfcycles_per_frame - timing.cpu_divisor + timing.cpu_divisor / 2;
            assert_eq!(
                FramePosition::new(last_edge, timing).tstate_after_cpu_halfcycles(5, timing),
                2
            );
        }
    }

    #[test]
    fn frame_position_advance_wraps_at_frame_boundary() {
        let pos = FramePosition::new(TIMING_48K.halfcycles_per_frame - 2, &TIMING_48K);
        assert_eq!(pos.advance(4, &TIMING_48K).halfcycles(), 2);
    }
}
