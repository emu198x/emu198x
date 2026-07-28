//! Exact clock-domain conversion for stock and accelerated CPUs.
//!
//! The Amiga chipset remains on its authoritative system clock. A processor
//! with a different input clock consumes the integer number of edges emitted
//! by [`CpuClock`] for each system tick. The retained remainder is part of
//! deterministic machine state and is therefore serialized.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// Progress through the CPU portion of one Amiga system tick.
///
/// Normally the shared machine driver consumes every emitted CPU edge before
/// returning. An instruction debugger may stop at an instruction boundary
/// between two edges of the same system tick. Retaining the unconsumed edges
/// prevents that stop from either discarding processor time or advancing the
/// chipset twice when execution resumes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CpuDomainPhase {
    edges_remaining: u64,
    motherboard_slot_pending: bool,
}

impl CpuDomainPhase {
    /// Whether no partially consumed system tick remains.
    #[must_use]
    pub const fn is_idle(self) -> bool {
        self.edges_remaining == 0
    }

    /// Number of processor edges still due in the current system tick.
    #[must_use]
    pub const fn edges_remaining(self) -> u64 {
        self.edges_remaining
    }

    /// Begin the CPU portion of a newly advanced system tick.
    ///
    /// # Panics
    ///
    /// Panics if the preceding system tick still has unconsumed CPU edges.
    pub fn begin_tick(&mut self, edges: u64) {
        assert!(self.is_idle(), "CPU domain tick is already in progress");
        self.edges_remaining = edges;
        self.motherboard_slot_pending = edges != 0;
    }

    /// Consume one processor edge and report whether it receives this system
    /// tick's motherboard admission slot.
    ///
    /// Only the first edge receives the slot. Accelerator-local accesses do
    /// not consume it in the bridge itself, but later edges still cannot claim
    /// another motherboard slot in the same system tick.
    pub fn take_edge(&mut self) -> Option<bool> {
        if self.edges_remaining == 0 {
            return None;
        }

        let motherboard_slot = self.motherboard_slot_pending;
        self.motherboard_slot_pending = false;
        self.edges_remaining -= 1;
        Some(motherboard_slot)
    }

    /// Whether this value can occur at an externally observable snapshot
    /// boundary for the supplied CPU clock.
    ///
    /// The shared driver can stop only after consuming an edge. A persisted
    /// partial tick therefore cannot retain the first-edge motherboard slot,
    /// and must retain fewer edges than the current tick emitted. The clock
    /// accumulator must also describe exactly the number of completed ticks,
    /// plus the one started tick when partial CPU work remains.
    #[must_use]
    pub fn snapshot_is_coherent(self, clock: CpuClock, completed_ticks: u64) -> bool {
        if self.motherboard_slot_pending {
            return false;
        }

        let tick_in_progress = !self.is_idle();
        if !clock.phase_matches_tick_count(completed_ticks, tick_in_progress) {
            return false;
        }

        self.is_idle() || self.edges_remaining < clock.edges_emitted_by_most_recent_started_tick()
    }
}

impl<'de> Deserialize<'de> for CpuDomainPhase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StoredPhase {
            edges_remaining: u64,
            motherboard_slot_pending: bool,
        }

        let stored = StoredPhase::deserialize(deserializer)?;
        if stored.edges_remaining == 0 && stored.motherboard_slot_pending {
            return Err(D::Error::custom(
                "idle CPU domain retains a motherboard slot",
            ));
        }

        Ok(Self {
            edges_remaining: stored.edges_remaining,
            motherboard_slot_pending: stored.motherboard_slot_pending,
        })
    }
}

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

    /// Maximum integer number of processor edges any one system tick can
    /// produce at this rational rate.
    #[must_use]
    pub const fn maximum_edges_per_tick(self) -> u64 {
        let whole = self.numerator / self.denominator;
        if self.numerator.is_multiple_of(self.denominator) {
            whole
        } else {
            whole + 1
        }
    }

    /// Whether the retained accumulator phase agrees with machine time.
    ///
    /// The clock is advanced when a system tick starts. A partial CPU domain
    /// therefore accounts for one more started tick than the machine's
    /// completed-tick counter.
    #[must_use]
    pub fn phase_matches_tick_count(self, completed_ticks: u64, tick_in_progress: bool) -> bool {
        let started_ticks = u128::from(completed_ticks) + u128::from(tick_in_progress);
        let expected = (u128::from(self.numerator) * started_ticks) % u128::from(self.denominator);
        u128::from(self.phase) == expected
    }

    /// Processor edges emitted when the current accumulator phase was
    /// produced.
    ///
    /// This is used only while a system tick is in progress. Reversing one
    /// accumulator step identifies the exact edge count for that tick,
    /// including ratios that alternate between adjacent integer counts.
    #[must_use]
    fn edges_emitted_by_most_recent_started_tick(self) -> u64 {
        let step = self.numerator % self.denominator;
        let previous_phase = if self.phase >= step {
            self.phase - step
        } else {
            self.denominator - (step - self.phase)
        };
        let edges = (u128::from(previous_phase) + u128::from(self.numerator))
            / u128::from(self.denominator);
        u64::try_from(edges).expect("one clock tick cannot emit more than u64::MAX edges")
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
    fn cpu_domain_phase_retains_unconsumed_edges() {
        let mut phase = CpuDomainPhase::default();
        phase.begin_tick(3);

        assert_eq!(phase.take_edge(), Some(true));
        assert_eq!(phase.edges_remaining(), 2);

        let encoded = postcard::to_allocvec(&phase).expect("serialize CPU domain phase");
        let mut restored: CpuDomainPhase =
            postcard::from_bytes(&encoded).expect("deserialize CPU domain phase");

        assert_eq!(restored.take_edge(), Some(false));
        assert_eq!(restored.take_edge(), Some(false));
        assert_eq!(restored.take_edge(), None);
        assert!(restored.is_idle());
    }

    #[test]
    fn zero_edge_tick_remains_idle() {
        let mut phase = CpuDomainPhase::default();
        phase.begin_tick(0);

        assert!(phase.is_idle());
        assert_eq!(phase.take_edge(), None);
    }

    #[test]
    fn snapshot_coherence_rejects_unconsumed_first_slot_and_excess_edges() {
        let mut clock = CpuClock::from_ratio(5, 2);
        let mut phase = CpuDomainPhase::default();
        phase.begin_tick(clock.edges_for_tick());
        assert!(!phase.snapshot_is_coherent(clock, 0));

        assert_eq!(phase.take_edge(), Some(true));
        assert!(phase.snapshot_is_coherent(clock, 0));

        let too_many = CpuDomainPhase {
            edges_remaining: 4,
            motherboard_slot_pending: false,
        };
        assert!(!too_many.snapshot_is_coherent(clock, 0));
    }

    #[test]
    fn snapshot_coherence_rejects_unreachable_integer_ratio_progress() {
        let mut stock = CpuClock::from_ratio(1, 1);
        assert_eq!(stock.edges_for_tick(), 1);
        let forged_stock = CpuDomainPhase {
            edges_remaining: 1,
            motherboard_slot_pending: false,
        };
        assert!(!forged_stock.snapshot_is_coherent(stock, 0));

        let mut a1200 = CpuClock::from_ratio(2, 1);
        assert_eq!(a1200.edges_for_tick(), 2);
        let forged_a1200 = CpuDomainPhase {
            edges_remaining: 2,
            motherboard_slot_pending: false,
        };
        assert!(!forged_a1200.snapshot_is_coherent(a1200, 0));
    }

    #[test]
    fn snapshot_coherence_rejects_clock_phase_unrelated_to_machine_ticks() {
        let mut clock = CpuClock::from_ratio(5, 2);
        assert_eq!(clock.edges_for_tick(), 2);
        assert_eq!(clock.phase(), 1);

        assert!(!CpuDomainPhase::default().snapshot_is_coherent(clock, 0));
        assert!(CpuDomainPhase::default().snapshot_is_coherent(clock, 1));
    }

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
