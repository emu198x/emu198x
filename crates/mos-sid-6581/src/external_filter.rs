//! C64 audio output stage (reSID `extfilt.{h,cc}`, Dag Lem; vendored in
//! VICE 3.10 at `emulators/c64/vice-3.10/src/resid/`).
//!
//! The C64 board couples the SID's output through two STC networks: a
//! low-pass with 3 dB at 16 kHz (10 kΩ / 1000 pF) followed by a high-pass
//! with 3 dB at 16 Hz (10 µF into a 1 kΩ amplifier input). The high-pass is
//! what AC-couples the SID's considerable DC operating point away — and what
//! lets `$D418` master-volume steps through as the classic 4-bit "digi".
//!
//! Cutoff-frequency accuracy (4 bits) is traded for signal accuracy (27
//! bits) in the fixed-point coefficients, exactly as reSID does — the two
//! corner frequencies are far apart.

use serde::{Deserialize, Serialize};

/// Low-pass coefficient: `dt/(dt+RC) * 2^7` at a 1 MHz clock,
/// `1e-6/(1e-6 + 1e4*1e-9) * 128 + 0.5`.
const W0LP_1_S7: i32 = 12;

/// High-pass coefficient: `dt/(dt+RC) * 2^17` at a 1 MHz clock,
/// `1e-6/(1e-6 + 1e3*1e-5) * 131072 + 0.5`.
const W0HP_1_S17: i32 = 13;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExternalFilter {
    /// Low-pass state (27 bits).
    vlp: i32,
    /// High-pass state (27 bits).
    vhp: i32,
}

impl ExternalFilter {
    #[must_use]
    pub const fn new() -> Self {
        Self { vlp: 0, vhp: 0 }
    }

    pub fn reset(&mut self) {
        self.vlp = 0;
        self.vhp = 0;
    }

    /// Clock one SID cycle with the filter/mixer output sample.
    pub fn clock(&mut self, vi: i16) {
        // Vlp = Vlp + w0lp*(Vi - Vlp)*delta_t
        // Vhp = Vhp + w0hp*(Vlp - Vhp)*delta_t
        let dvlp = W0LP_1_S7.wrapping_mul((i32::from(vi) << 11).wrapping_sub(self.vlp)) >> 7;
        let dvhp = W0HP_1_S17.wrapping_mul(self.vlp.wrapping_sub(self.vhp)) >> 17;
        self.vlp = self.vlp.wrapping_add(dvlp);
        self.vhp = self.vhp.wrapping_add(dvhp);
    }

    /// Audio output (16 bits): `Vo = Vlp - Vhp`.
    #[must_use]
    pub const fn output(&self) -> i32 {
        (self.vlp - self.vhp) >> 11
    }
}
