//! SID state-variable multi-mode filter.
//!
//! The SID filter is a two-integrator-loop state-variable filter (SVF)
//! that can operate in low-pass, band-pass, and high-pass modes
//! simultaneously. The cutoff frequency and resonance are controlled
//! by registers $D415–$D417.
//!
//! # Model differences
//!
//! The 6581 has a non-linear cutoff curve derived from reSID die analysis:
//! the low end kinks steeply (real 6581 has ~200 Hz minimum), then ramps.
//! The 8580 has a wider, more linear range.

#![allow(clippy::cast_precision_loss)]

use crate::SidModel;

/// State-variable filter.
pub struct Filter {
    /// Low-pass output.
    lp: f32,
    /// Band-pass output.
    bp: f32,
    /// High-pass output (computed each tick, not stored as state).
    hp: f32,

    /// 11-bit cutoff frequency register.
    pub cutoff: u16,
    /// 4-bit resonance register.
    pub resonance: u8,
    /// Filter mode bits (from $D418): bit 4 = LP, bit 5 = BP, bit 6 = HP.
    pub mode: u8,
    /// Filter routing: which voices pass through the filter (bits 0–2 of $D417).
    pub routing: u8,
    /// Filter external input (bit 3 of $D417).
    pub ext_in: bool,

    /// Chip model (determines cutoff curve and resonance range).
    model: SidModel,
}

impl Filter {
    #[must_use]
    pub fn new(model: SidModel) -> Self {
        Self {
            lp: 0.0,
            bp: 0.0,
            hp: 0.0,
            cutoff: 0,
            resonance: 0,
            mode: 0,
            routing: 0,
            ext_in: false,
            model,
        }
    }

    /// Process one sample through the filter.
    ///
    /// Returns the filtered output (sum of active filter modes).
    pub fn clock(&mut self, input: f32) -> f32 {
        let fc = self.cutoff_coefficient();
        let res = self.resonance_coefficient();

        // State-variable filter equations
        self.hp = input - self.lp - res * self.bp;
        self.bp += fc * self.hp;
        self.lp += fc * self.bp;

        // Sum active modes
        let mut output = 0.0;
        if self.mode & 0x10 != 0 {
            output += self.lp;
        }
        if self.mode & 0x20 != 0 {
            output += self.bp;
        }
        if self.mode & 0x40 != 0 {
            output += self.hp;
        }
        output
    }

    /// Convert the 11-bit cutoff register to a filter coefficient.
    ///
    /// **6581**: Non-linear curve from reSID die analysis. The low end has a
    /// ~200 Hz floor, then ramps steeply through the midrange. Approximated
    /// with a piecewise polynomial.
    ///
    /// **8580**: Wider linear range, 0.001..0.55.
    fn cutoff_coefficient(&self) -> f32 {
        let raw = f32::from(self.cutoff);

        match self.model {
            SidModel::Mos6581 => {
                // reSID-derived non-linear curve for the 6581.
                // The 6581 filter has a ~200 Hz floor at cutoff=0, a steep
                // ramp from cutoff ~200 to ~800, then a more gradual rise.
                //
                // Polynomial approximation: fc = a + b*x + c*x^2
                // where x = cutoff / 2047.0, fitted to reSID reference data.
                let x = raw / 2047.0;
                let fc = 0.003 + 0.02 * x + 0.33 * x * x;
                fc.clamp(0.002, 0.36)
            }
            SidModel::Mos8580 => {
                // 8580: wider linear range
                let x = raw / 2047.0;
                0.001 + x * 0.549
            }
        }
    }

    /// Convert the 4-bit resonance register to a feedback coefficient.
    ///
    /// **6581**: 0.7..1.7 (high resonance, can self-oscillate).
    /// **8580**: 0.7..1.4 (lower Q ceiling).
    fn resonance_coefficient(&self) -> f32 {
        let r = f32::from(self.resonance);
        match self.model {
            SidModel::Mos6581 => 0.7 + r * (1.0 / 15.0),
            SidModel::Mos8580 => 0.7 + r * (0.7 / 15.0),
        }
    }

    /// Returns true if voice `n` (0–2) is routed through the filter.
    #[must_use]
    pub fn voice_routed(&self, voice: usize) -> bool {
        self.routing & (1 << voice) != 0
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::new(SidModel::Mos6581)
    }
}
