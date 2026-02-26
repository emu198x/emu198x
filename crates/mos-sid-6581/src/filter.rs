//! SID state-variable multi-mode filter.
//!
//! The 6581 filter is a two-integrator-loop state-variable filter (SVF)
//! that can operate in low-pass, band-pass, and high-pass modes
//! simultaneously. The cutoff frequency and resonance are controlled
//! by registers $D415–$D417.

#![allow(clippy::cast_precision_loss)]

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
}

impl Filter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            lp: 0.0,
            bp: 0.0,
            hp: 0.0,
            cutoff: 0,
            resonance: 0,
            mode: 0,
            routing: 0,
            ext_in: false,
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
    /// For v1: linear mapping. The real 6581 has a non-linear curve
    /// that varies between chips, but linear is close enough for
    /// recognisable audio.
    fn cutoff_coefficient(&self) -> f32 {
        // Map 0..2047 to approximately 0.0..1.0
        // The coefficient determines how much of hp/bp feeds into the
        // integrators. Real SID range is roughly 30 Hz to 12 kHz.
        let raw = f32::from(self.cutoff) / 2047.0;
        // Scale to useful range: 0.002..0.35 (matches ~30 Hz to ~12 kHz at 1 MHz)
        0.002 + raw * 0.348
    }

    /// Convert the 4-bit resonance register to a feedback coefficient.
    fn resonance_coefficient(&self) -> f32 {
        // Map 0..15 to approximately 0.7..1.7 (higher = more resonance)
        // At resonance=0, minimal feedback; at 15, close to self-oscillation
        0.7 + f32::from(self.resonance) * (1.0 / 15.0)
    }

    /// Returns true if voice `n` (0–2) is routed through the filter.
    #[must_use]
    pub fn voice_routed(&self, voice: usize) -> bool {
        self.routing & (1 << voice) != 0
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::new()
    }
}
