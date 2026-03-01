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
//! the low end has a ~200 Hz floor, kinks steeply through the midrange,
//! then ramps gradually. Modelled with a 32-point piecewise-linear table
//! that captures the inflection points better than a smooth polynomial.
//!
//! The 8580 has a wider, more linear range.

#![allow(clippy::cast_precision_loss)]

use crate::SidModel;

/// 6581 filter coefficient lookup table (32 entries).
///
/// Index 0 = cutoff register 0, index 31 = cutoff register 2047.
/// Values are the SVF frequency coefficient (fc) at each point.
///
/// Derived from reSID die-analysis of the 6581R3 filter. The curve
/// has three distinct regions:
/// - Low (reg 0–~200): near-flat floor at ~0.002 (200 Hz)
/// - Kink (reg ~200–~700): steep ramp from 0.002 to ~0.08
/// - High (reg ~700–2047): gradual ramp from 0.08 to ~0.36
const FC_6581_TABLE: [f32; 32] = [
    0.0020, 0.0020, 0.0020, 0.0022, // 0, 66, 132, 198
    0.0030, 0.0055, 0.0100, 0.0165, // 264, 330, 396, 462
    0.0250, 0.0360, 0.0480, 0.0600, // 528, 594, 660, 726
    0.0730, 0.0860, 0.0990, 0.1120, // 792, 858, 924, 990
    0.1250, 0.1380, 0.1510, 0.1640, // 1056, 1122, 1188, 1254
    0.1770, 0.1900, 0.2030, 0.2160, // 1320, 1386, 1452, 1518
    0.2290, 0.2430, 0.2580, 0.2740, // 1584, 1650, 1716, 1782
    0.2920, 0.3100, 0.3300, 0.3600, // 1848, 1914, 1980, 2047
];

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
    /// **6581**: Piecewise-linear lookup from 32-point table derived from
    /// reSID die analysis. Captures the distinctive low-end kink that a
    /// smooth polynomial misses.
    ///
    /// **8580**: Wider linear range, 0.001..0.55.
    fn cutoff_coefficient(&self) -> f32 {
        match self.model {
            SidModel::Mos6581 => {
                // Map 11-bit register (0–2047) to table index (0–31) with
                // linear interpolation between entries.
                let pos = f32::from(self.cutoff) * 31.0 / 2047.0;
                let idx = pos as usize;
                if idx >= 31 {
                    FC_6581_TABLE[31]
                } else {
                    let frac = pos - idx as f32;
                    FC_6581_TABLE[idx] + frac * (FC_6581_TABLE[idx + 1] - FC_6581_TABLE[idx])
                }
            }
            SidModel::Mos8580 => {
                // 8580: wider linear range
                let x = f32::from(self.cutoff) / 2047.0;
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
