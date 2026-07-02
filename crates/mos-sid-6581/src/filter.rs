//! SID state-variable multi-mode filter.

#![allow(clippy::cast_precision_loss)]

use serde::{Deserialize, Serialize};

use crate::SidModel;

/// Filter-cutoff coefficient vs the 11-bit `fc` register (`$D415`/`$D416`),
/// sampled at 32 evenly-spaced points and interpolated in [`cutoff_coefficient`].
///
/// Provenance: this is a hand-fitted, monotonic **approximation** of the 6581's
/// cutoff-vs-`fc` response — chosen to give a plausible sweep shape — **not** a
/// measured dataset. The authoritative reference is reSID's measured 6581 op-amp
/// transfer function (`opamp_voltage_6581[]`, "measured on real chips", in the
/// vendored VICE tree at `emulators/multi-system/vice/vice/src/resid/filter.cc`;
/// the refined model lives in `.../lib/libresidfp/src/FilterModelConfig6581.cpp`).
/// The 6581's true response is non-linear and voltage-dependent, so replacing
/// this curve with the measured one is deliberately deferred to the
/// nonlinear-filter work in issue #19; until then, treat the audible cutoff as
/// approximate, not reSID-grade.
const FC_6581_TABLE: [f32; 32] = [
    0.0020, 0.0020, 0.0020, 0.0022, 0.0030, 0.0055, 0.0100, 0.0165, 0.0250, 0.0360, 0.0480, 0.0600,
    0.0730, 0.0860, 0.0990, 0.1120, 0.1250, 0.1380, 0.1510, 0.1640, 0.1770, 0.1900, 0.2030, 0.2160,
    0.2290, 0.2430, 0.2580, 0.2740, 0.2920, 0.3100, 0.3300, 0.3600,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Filter {
    lp: f32,
    bp: f32,
    hp: f32,
    pub cutoff: u16,
    pub resonance: u8,
    pub mode: u8,
    pub routing: u8,
    pub ext_in: bool,
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

    pub fn clock(&mut self, input: f32) -> f32 {
        let fc = self.cutoff_coefficient();
        let res = self.resonance_coefficient();

        self.hp = input - self.lp - res * self.bp;
        self.bp += fc * self.hp;
        self.lp += fc * self.bp;

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

    fn cutoff_coefficient(&self) -> f32 {
        match self.model {
            SidModel::Mos6581 => {
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
                // The 8580's cutoff is far more linear than the 6581's, so a
                // straight-line fit across the fc range is a fair first-order
                // approximation. Like the 6581 curve above, this is a fit, not a
                // measured dataset; the reSID-grade 8580 filter is issue #20.
                let x = f32::from(self.cutoff) / 2047.0;
                0.001 + x * 0.549
            }
        }
    }

    fn resonance_coefficient(&self) -> f32 {
        let r = f32::from(self.resonance);
        match self.model {
            SidModel::Mos6581 => 0.7 + r * (1.0 / 15.0),
            SidModel::Mos8580 => 0.7 + r * (0.7 / 15.0),
        }
    }

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
