//! Band-limited conversion of a held stereo signal at caller-owned times.
//!
//! A normalised Blackman-windowed sinc step response places each level change
//! into a fixed output ring. No transition queue can overflow. The caller
//! retains its integer sample phase and emits at its existing host cadence.
//! At 48 kHz, the cutoff is 22 kHz and the linear-phase delay is 1 ms.
//! Design/evidence: Amiga `paula-audio/bandwidth-probe` in the test corpus.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use std::sync::LazyLock;

/// Number of future host frames affected by a signal transition.
pub const SUPPORT: usize = 96;
/// Fixed signal delay, in host frames; align time-varying controls to this.
pub const DELAY: usize = SUPPORT / 2;
const PHASES: usize = 256;
const CUTOFF: f64 = 22.0 / 48.0;
// Generous bound for normalised input: |correction| <= 1 + ||h||_1 < 4.
const MAX_CORRECTION: f64 = 8.0;
type Stereo = [f64; 2];
type Row = [f64; SUPPORT];

static KERNEL: LazyLock<Box<[Row]>> = LazyLock::new(|| {
    // Allocate on the heap, including during construction (also on Wasm).
    let mut integral = vec![0.0; SUPPORT * PHASES + 1];
    let mut previous = 0.0;
    let mut area = 0.0;
    for (i, sample) in integral.iter_mut().enumerate() {
        let t = i as f64 / PHASES as f64;
        let x = t - DELAY as f64;
        let window = 0.42 - 0.5 * (2.0 * PI * t / SUPPORT as f64).cos()
            + 0.08 * (4.0 * PI * t / SUPPORT as f64).cos();
        let sinc = if x == 0.0 {
            2.0 * CUTOFF
        } else {
            (2.0 * PI * CUTOFF * x).sin() / (PI * x)
        };
        let value = window * sinc;
        if i != 0 {
            area += (previous + value) / (2 * PHASES) as f64;
        }
        *sample = area;
        previous = value;
    }
    for sample in &mut integral {
        *sample /= area;
    }
    let mut rows = vec![[0.0; SUPPORT]; PHASES + 1].into_boxed_slice();
    for (phase, row) in rows.iter_mut().enumerate() {
        for (sample, correction) in row.iter_mut().enumerate() {
            *correction = integral[sample * PHASES + phase] - 1.0;
        }
    }
    rows
});

/// Persistable signal history, excluding immutable filter coefficients.
/// Decode with [`BandLimitedStereo::from_history`] before using saved data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BandLimitedHistory {
    /// Most recently observed normalised stereo level.
    pub level: Stereo,
    /// Exactly [`SUPPORT`] pending stereo corrections, in ring storage order.
    pub pending: Vec<Stereo>,
    /// Ring slot consumed by the next host frame.
    pub cursor: u8,
}

/// Fixed-capacity stereo resampler for signals bounded to -1..=1.
#[derive(Clone, Debug)]
pub struct BandLimitedStereo {
    level: Stereo,
    pending: [Stereo; SUPPORT],
    cursor: usize,
    kernel: &'static [Row],
}

impl Default for BandLimitedStereo {
    fn default() -> Self {
        Self {
            level: [0.0; 2],
            pending: [[0.0; 2]; SUPPORT],
            cursor: 0,
            kernel: &KERNEL,
        }
    }
}

impl BandLimitedStereo {
    /// Observe the level held from the current fractional host phase onward.
    /// `phase / period` is the elapsed fraction of the current host interval.
    /// The caller must call [`Self::emit`] at each subsequent host boundary.
    ///
    /// # Panics
    /// Panics if a changed level is non-finite/outside -1..=1, or `phase`
    /// is outside `0..period`. These are caller programming errors.
    #[inline]
    pub fn observe(&mut self, level: Stereo, phase: u64, period: u64) {
        if level == self.level {
            return;
        }
        assert!(phase < period);
        assert!(level.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        let position = (period - phase) as f64 / period as f64 * PHASES as f64;
        let index = (position as usize).min(PHASES - 1);
        let fraction = position - index as f64;
        let first = &self.kernel[index];
        let second = &self.kernel[index + 1];
        let delta = [level[0] - self.level[0], level[1] - self.level[1]];
        self.level = level;
        let (before, after) = self.pending.split_at_mut(self.cursor);
        let split = SUPPORT - self.cursor;
        add_corrections(after, &first[..split], &second[..split], fraction, delta);
        add_corrections(before, &first[split..], &second[split..], fraction, delta);
    }

    /// Emit one stereo frame at the caller's host boundary. Ringing may
    /// exceed unity; leave final clipping to the downstream output stage.
    #[must_use]
    pub fn emit(&mut self) -> Stereo {
        let correction = &mut self.pending[self.cursor];
        let sample = [self.level[0] + correction[0], self.level[1] + correction[1]];
        *correction = [0.0; 2];
        self.cursor = (self.cursor + 1) % SUPPORT;
        sample
    }

    /// Capture all history required for exact continuation.
    #[must_use]
    pub fn history(&self) -> BandLimitedHistory {
        BandLimitedHistory {
            level: self.level,
            pending: self.pending.to_vec(),
            cursor: self.cursor as u8,
        }
    }

    /// Reconstruct validated history, returning `None` for invalid lengths,
    /// cursor, non-finite values or signal outside the supported bounds.
    #[must_use]
    pub fn from_history(history: &BandLimitedHistory) -> Option<Self> {
        if history.pending.len() != SUPPORT
            || usize::from(history.cursor) >= SUPPORT
            || history
                .level
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1.0)
            || history
                .pending
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > MAX_CORRECTION)
        {
            return None;
        }
        let mut result = Self::default();
        result.pending.copy_from_slice(&history.pending);
        result.level = history.level;
        result.cursor = usize::from(history.cursor);
        Some(result)
    }
}

fn add_corrections(
    pending: &mut [Stereo],
    first: &[f64],
    second: &[f64],
    fraction: f64,
    delta: Stereo,
) {
    for ((slot, first), second) in pending.iter_mut().zip(first).zip(second) {
        let c = first + fraction * (second - first);
        slot[0] += delta[0] * c;
        slot[1] += delta[1] * c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_fractional_pulse_area_and_stereo_polarity() {
        for phase in [0, 100, 500, 990] {
            let mut filter = BandLimitedStereo::default();
            filter.observe([0.25, -0.125], phase, 1000);
            filter.observe([0.0; 2], phase + 1, 1000);
            let mut area = [0.0; 2];
            for _ in 0..SUPPORT + 2 {
                let sample = filter.emit();
                area[0] += sample[0];
                area[1] += sample[1];
            }
            assert!((area[0] - 0.00025).abs() < 1e-8);
            assert!((area[1] + 0.000125).abs() < 1e-8);
        }
    }

    #[test]
    fn settles_to_dc_and_restores_pending_tail_exactly() {
        let mut source = BandLimitedStereo::default();
        source.observe([0.75, -0.25], 117, 148);
        for _ in 0..17 {
            let _ = source.emit();
        }
        let mut restored = BandLimitedStereo::from_history(&source.history()).expect("valid tail");
        for n in 0..200 {
            if n == 11 {
                source.observe([-0.5, 1.0], 37, 148);
                restored.observe([-0.5, 1.0], 37, 148);
            }
            assert_eq!(source.emit(), restored.emit());
        }
        assert_eq!(source.emit(), [-0.5, 1.0]);
        assert_eq!(restored.emit(), [-0.5, 1.0]);
        assert_eq!(source.history(), restored.history());
    }

    #[test]
    fn dense_transitions_remain_bounded_and_do_not_drop_short_pulses() {
        // Total step variation is ||h||_1. This checks the generous restore
        // bound against the generated kernel, not just observed workloads.
        let mut previous = -1.0;
        let mut variation = 0.0;
        for n in 0..SUPPORT {
            for row in KERNEL.iter().take(PHASES) {
                variation += (row[n] - previous).abs();
                previous = row[n];
            }
        }
        variation += previous.abs();
        assert!(1.0 + variation < MAX_CORRECTION / 2.0);

        let mut filter = BandLimitedStereo::default();
        let mut area = [0.0; 2];
        // 1024 edges within a single interval exceeds typical reference
        // event queues; all positive pulses must still contribute their area.
        for phase in 0..1024 {
            let level = if phase % 2 == 0 {
                [1.0, -0.5]
            } else {
                [0.0; 2]
            };
            filter.observe(level, phase, 1024);
        }
        assert!(BandLimitedStereo::from_history(&filter.history()).is_some());
        for _ in 0..SUPPORT + 2 {
            let sample = filter.emit();
            area[0] += sample[0];
            area[1] += sample[1];
        }
        assert!((area[0] - 0.5).abs() < 1e-6);
        assert!((area[1] + 0.25).abs() < 1e-6);
        assert_eq!(filter.emit(), [0.0; 2]);
    }
}
