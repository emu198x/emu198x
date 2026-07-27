//! Exact clock-domain conversion for stock and accelerated CPUs.
//!
//! The Amiga chipset remains on its authoritative system clock. A processor
//! with a different input clock consumes the integer number of edges emitted
//! by [`CpuClock`] for each system tick. The retained remainder is part of
//! deterministic machine state and is therefore serialized.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// A rational CPU-edge rate and its current integer accumulator phase.
///
/// `numerator / denominator` is the number of processor edges per system
/// tick. The ratio is stored in lowest terms and `phase` is always less than
/// `denominator`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CpuClock {
    numerator: u64,
    denominator: u64,
    phase: u64,
}

impl CpuClock {
    /// Create a clock that emits `cpu_edges / system_ticks` edges per tick.
    ///
    /// The ratio is reduced exactly using integer arithmetic and starts at
    /// phase zero.
    ///
    /// # Panics
    ///
    /// Panics when `system_ticks` is zero.
    #[must_use]
    pub const fn from_ratio(cpu_edges: u64, system_ticks: u64) -> Self {
        assert!(
            system_ticks != 0,
            "system clock denominator must be non-zero"
        );
        let divisor = greatest_common_divisor(cpu_edges, system_ticks);
        Self {
            numerator: cpu_edges / divisor,
            denominator: system_ticks / divisor,
            phase: 0,
        }
    }

    /// Return the reduced CPU-edge numerator.
    #[must_use]
    pub const fn numerator(self) -> u64 {
        self.numerator
    }

    /// Return the reduced system-tick denominator.
    #[must_use]
    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    /// Return the retained fractional phase in denominator units.
    #[must_use]
    pub const fn phase(self) -> u64 {
        self.phase
    }

    /// Reset the fractional phase without changing the configured rate.
    pub const fn reset_phase(&mut self) {
        self.phase = 0;
    }

    /// Advance by one system tick and return the CPU edges it produces.
    pub fn edges_for_tick(&mut self) -> u64 {
        self.edges_for_ticks(1)
    }

    /// Advance by a chunk of system ticks and return the CPU edges produced.
    ///
    /// Advancing a chunk is exactly equivalent to calling
    /// [`Self::edges_for_tick`] repeatedly. A `u128` intermediate keeps the
    /// multiplication exact for every pair of `u64` inputs.
    ///
    /// # Panics
    ///
    /// Panics when the resulting edge count does not fit in a `u64`.
    pub fn edges_for_ticks(&mut self, system_ticks: u64) -> u64 {
        let total = u128::from(self.phase) + u128::from(self.numerator) * u128::from(system_ticks);
        let denominator = u128::from(self.denominator);
        let edges = total / denominator;
        let phase = total % denominator;
        let edges = match u64::try_from(edges) {
            Ok(edges) => edges,
            Err(_) => panic!("CPU edge count exceeds u64"),
        };

        self.phase = phase as u64;
        edges
    }
}

impl Default for CpuClock {
    fn default() -> Self {
        Self::from_ratio(1, 1)
    }
}

impl<'de> Deserialize<'de> for CpuClock {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StoredClock {
            numerator: u64,
            denominator: u64,
            phase: u64,
        }

        let stored = StoredClock::deserialize(deserializer)?;
        if stored.denominator == 0 {
            return Err(D::Error::custom("CPU clock denominator is zero"));
        }
        if greatest_common_divisor(stored.numerator, stored.denominator) != 1 {
            return Err(D::Error::custom("CPU clock ratio is not reduced"));
        }
        if stored.phase >= stored.denominator {
            return Err(D::Error::custom("CPU clock phase exceeds its denominator"));
        }

        Ok(Self {
            numerator: stored.numerator,
            denominator: stored.denominator,
            phase: stored.phase,
        })
    }
}

const fn greatest_common_divisor(mut lhs: u64, mut rhs: u64) -> u64 {
    while rhs != 0 {
        let remainder = lhs % rhs;
        lhs = rhs;
        rhs = remainder;
    }
    lhs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_are_reduced_without_losing_frequency() {
        let stock_a1200 = CpuClock::from_ratio(14_187_580, 7_093_790);
        assert_eq!(stock_a1200.numerator(), 2);
        assert_eq!(stock_a1200.denominator(), 1);

        let a530_40_mhz = CpuClock::from_ratio(40_000_000, 7_093_790);
        assert_eq!(a530_40_mhz.numerator(), 4_000_000);
        assert_eq!(a530_40_mhz.denominator(), 709_379);
    }

    #[test]
    fn fractional_phase_produces_the_exact_edge_sequence() {
        let mut clock = CpuClock::from_ratio(5, 2);

        assert_eq!(clock.edges_for_tick(), 2);
        assert_eq!(clock.phase(), 1);
        assert_eq!(clock.edges_for_tick(), 3);
        assert_eq!(clock.phase(), 0);
    }

    #[test]
    fn one_complete_ratio_period_has_no_drift() {
        let mut clock = CpuClock::from_ratio(40_000_000, 7_093_790);

        assert_eq!(clock.edges_for_ticks(709_379), 4_000_000);
        assert_eq!(clock.phase(), 0);
    }

    #[test]
    fn chunking_matches_one_tick_at_a_time_from_a_nonzero_phase() {
        let mut per_tick = CpuClock::from_ratio(4_000_000, 709_379);
        let mut chunked = per_tick;
        assert_eq!(per_tick.edges_for_tick(), chunked.edges_for_tick());

        let expected: u64 = (0..10_000).map(|_| per_tick.edges_for_tick()).sum();
        let actual = chunked.edges_for_ticks(10_000);

        assert_eq!(actual, expected);
        assert_eq!(chunked.phase(), per_tick.phase());
    }

    #[test]
    fn serde_preserves_fractional_phase() {
        let mut clock = CpuClock::from_ratio(5, 2);
        assert_eq!(clock.edges_for_tick(), 2);
        let encoded = postcard::to_allocvec(&clock).expect("serialize CPU clock");
        let mut restored: CpuClock = postcard::from_bytes(&encoded).expect("deserialize CPU clock");

        assert_eq!(restored, clock);
        assert_eq!(restored.edges_for_tick(), 3);
    }

    #[test]
    fn zero_cpu_rate_is_a_canonical_stopped_clock() {
        let mut clock = CpuClock::from_ratio(0, 7_093_790);

        assert_eq!(clock.numerator(), 0);
        assert_eq!(clock.denominator(), 1);
        assert_eq!(clock.edges_for_ticks(u64::MAX), 0);
    }
}
