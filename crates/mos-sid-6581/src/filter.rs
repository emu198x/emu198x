//! SID op-amp filter model (issues #19/#20).
//!
//! Ported from reSID's `filter8580new.h` (Dag Lem) as vendored in VICE 3.10
//! (`emulators/c64/vice-3.10/src/resid/`) — the unified op-amp model for both
//! the 6581 and the 8580. Replaces the previous piecewise-linear
//! state-variable approximation.
//!
//! The SID filter is a two-integrator-loop biquad (confirmed by Bob Yannes),
//! but its active stages are self-biased NMOS inverters, not ideal op-amps.
//! reSID models the loop with the *measured* op-amp voltage transfer of each
//! model plus transistor physics for the cutoff VCRs:
//!
//! - `Vhp` (summer output), `Vbp` and `Vlp` (integrator outputs) live in
//!   16-bit translated table units.
//! - Each integrator solves `vc = vc0 - n*(I_snake + I_vcr)` per cycle with a
//!   single fixpoint step over the reverse op-amp table (6581), or the
//!   parallel-NMOS DAC resistance (8580).
//! - The summer, resonance ladder, audio mixer, and master-volume ladder are
//!   all precomputed op-amp solutions in [`crate::filter_tables`].
//!
//! This module also owns the `$D417`/`$D418` register semantics (routing,
//! resonance, mode, volume): the mixer and master volume are physically part
//! of the filter/output stage, so `voice3off` and the volume DAC "digi"
//! behaviour fall out of the model instead of being host-side special cases.
//!
//! Determinism note: reSID dithers the voice inputs with `rand()`-filled
//! noise (±2 LSB after scaling) to decorrelate quantisation. Catalogue audio
//! hashes require reproducible output, so the dither here is a fixed-seed
//! xorshift PRNG — same statistics, deterministic stream.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use serde::{Deserialize, Serialize};

use crate::SidModel;
use crate::filter_tables::{MIXER_OFFSET, ModelTables, SUMMER_OFFSET, tables};

/// Voice-mask default: voices 1-3 connected, EXT IN (bit 3) disconnected —
/// matches reSID's `set_voice_mask(0x07)` with no external input wired.
const VOICE_MASK: u8 = 0xF7;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Filter {
    model: SidModel,

    // Register state.
    /// 11-bit cutoff (`$D415`/`$D416`).
    fc: u16,
    /// Resonance nibble (`$D417` bits 4-7).
    res: u8,
    /// Filter routing nibble (`$D417` bits 0-3; bit 3 = EXT IN).
    filt: u8,
    /// Mode bits (`$D418` bits 4-7; bit 7 = voice3off).
    mode: u8,
    /// Master volume nibble (`$D418` bits 0-3).
    vol: u8,

    // Routing derived from filt/mode/voice mask.
    sum: u8,
    mix: u8,

    // Filter state, in translated 16-bit table units.
    vhp: i32,
    vbp: i32,
    vbp_x: i32,
    vbp_vc: i32,
    vlp: i32,
    vlp_x: i32,
    vlp_vc: i32,

    // Filter / mixer inputs.
    ve: i32,
    v1: i32,
    v2: i32,
    v3: i32,

    // 6581 cutoff state: (kVddt - Vw)^2 / 2 and the user filter-bias term.
    vddt_vw_2: u32,
    vw_bias: i32,

    // 8580 cutoff state: normalized DAC current and gate voltage.
    n_dac: i32,
    nvgt: i32,

    /// Fixed-seed xorshift state for the voice-input dither.
    dither: u32,
}

impl Filter {
    #[must_use]
    pub fn new(model: SidModel) -> Self {
        let t = tables(model);
        let mut filter = Self {
            model,
            fc: 0,
            res: 0,
            filt: 0,
            mode: 0,
            vol: 0,
            sum: 0,
            mix: 0,
            vhp: 0,
            vbp: 0,
            vbp_x: 0,
            vbp_vc: 0,
            vlp: 0,
            vlp_x: 0,
            vlp_vc: 0,
            // EXT IN is disconnected; its resting level is the mixer op-amp
            // zero (reSID `input(0)`).
            ve: i32::from(t.mixer[0]),
            v1: 0,
            v2: 0,
            v3: 0,
            vddt_vw_2: 0,
            vw_bias: 0,
            n_dac: 0,
            nvgt: t.nvgt_default,
            dither: 0x7A19_2C4B,
        };
        filter.set_w0();
        filter.set_sum_mix();
        filter
    }

    #[must_use]
    pub const fn model(&self) -> SidModel {
        self.model
    }

    /// 11-bit cutoff register value.
    #[must_use]
    pub const fn cutoff(&self) -> u16 {
        self.fc
    }

    /// Resonance nibble.
    #[must_use]
    pub const fn resonance(&self) -> u8 {
        self.res
    }

    /// Routing nibble (bit 3 = EXT IN).
    #[must_use]
    pub const fn routing(&self) -> u8 {
        self.filt
    }

    /// Mode bits (`$D418` bits 4-7, bit 7 = voice3off).
    #[must_use]
    pub const fn mode(&self) -> u8 {
        self.mode
    }

    /// Master volume nibble.
    #[must_use]
    pub const fn volume(&self) -> u8 {
        self.vol
    }

    /// Whether `voice` (0-2) is routed through the filter.
    #[must_use]
    pub const fn voice_routed(&self, voice: usize) -> bool {
        self.filt & (1 << voice) != 0
    }

    pub fn reset(&mut self) {
        self.fc = 0;
        self.res = 0;
        self.filt = 0;
        self.mode = 0;
        self.vol = 0;
        self.vhp = 0;
        self.vbp = 0;
        self.vbp_x = 0;
        self.vbp_vc = 0;
        self.vlp = 0;
        self.vlp_x = 0;
        self.vlp_vc = 0;
        self.set_w0();
        self.set_sum_mix();
    }

    /// `$D415` — cutoff low three bits.
    pub fn write_fc_lo(&mut self, value: u8) {
        self.fc = (self.fc & 0x7F8) | u16::from(value & 0x07);
        self.set_w0();
    }

    /// `$D416` — cutoff high eight bits.
    pub fn write_fc_hi(&mut self, value: u8) {
        self.fc = ((u16::from(value) << 3) & 0x7F8) | (self.fc & 0x007);
        self.set_w0();
    }

    /// `$D417` — resonance + filter routing.
    pub fn write_res_filt(&mut self, value: u8) {
        self.res = (value >> 4) & 0x0F;
        self.filt = value & 0x0F;
        self.set_sum_mix();
    }

    /// `$D418` — mode (incl. voice3off) + master volume.
    pub fn write_mode_vol(&mut self, value: u8) {
        self.mode = value & 0xF0;
        self.vol = value & 0x0F;
        self.set_sum_mix();
    }

    /// Clock the filter one SID cycle with the three 20-bit voice values
    /// (waveform DAC level, zero-centred, times envelope DAC level).
    pub fn clock(&mut self, voice1: i32, voice2: i32, voice3: i32) {
        let t = tables(self.model);

        // Widen the scale multiply to i64: |voice| reaches ~816k and the scale
        // is 2442, so the i32 product overflows past ~879k. Real audio stays
        // just under, but the public clock() API accepts any i32. For every
        // non-overflowing input this is bit-identical to a plain i32 multiply.
        let scale = i64::from(t.voice_scale_s14);
        self.v1 =
            ((i64::from(voice1) * scale + i64::from(self.dither())) >> 18) as i32 + t.voice_dc;
        self.v2 =
            ((i64::from(voice2) * scale + i64::from(self.dither())) >> 18) as i32 + t.voice_dc;
        self.v3 =
            ((i64::from(voice3) * scale + i64::from(self.dither())) >> 18) as i32 + t.voice_dc;

        // Sum the inputs routed into the filter.
        let mut vi = 0i32;
        let mut inputs = 0usize;
        for (bit, v) in [
            (0x01, self.v1),
            (0x02, self.v2),
            (0x04, self.v3),
            (0x08, self.ve),
        ] {
            if self.sum & bit != 0 {
                vi += v;
                inputs += 1;
            }
        }
        let offset = SUMMER_OFFSET[inputs];

        // Calculate filter outputs: both integrators, then the summer with
        // the resonance ladder feeding back the bandpass output.
        match self.model {
            SidModel::Mos6581 => {
                self.vlp = solve_integrate_6581(
                    self.vbp,
                    &mut self.vlp_x,
                    &mut self.vlp_vc,
                    self.vddt_vw_2,
                    t,
                );
                self.vbp = solve_integrate_6581(
                    self.vhp,
                    &mut self.vbp_x,
                    &mut self.vbp_vc,
                    self.vddt_vw_2,
                    t,
                );
            }
            SidModel::Mos8580 => {
                self.vlp = solve_integrate_8580(
                    self.vbp,
                    &mut self.vlp_x,
                    &mut self.vlp_vc,
                    self.n_dac,
                    self.nvgt,
                    t,
                );
                self.vbp = solve_integrate_8580(
                    self.vhp,
                    &mut self.vbp_x,
                    &mut self.vbp_vc,
                    self.n_dac,
                    self.nvgt,
                    t,
                );
            }
        }

        debug_assert!((0..1 << 16).contains(&self.vbp));
        let resonance = i32::from(t.resonance[(usize::from(self.res) << 16) + clamp_u16(self.vbp)]);
        let idx = offset as i32 + resonance + self.vlp + vi;
        debug_assert!((0..t.summer.len() as i32).contains(&idx));
        self.vhp = i32::from(t.summer[clamp_index(idx, t.summer.len())]);
    }

    /// 16-bit audio output: mixer + master-volume ladder (reSID
    /// `Filter::output`).
    #[must_use]
    pub fn output(&self) -> i16 {
        let t = tables(self.model);

        // Sum the inputs routed into the mixer. Filter components pass the
        // 6581's slightly-larger mixer input "resistors" (filter_gain), with
        // one DC-recentring term regardless of how many are selected —
        // faithful to reSID's generated mixer switch.
        let mut vi = 0i32;
        let mut inputs = 0usize;
        for (bit, v) in [
            (0x01, self.v1),
            (0x02, self.v2),
            (0x04, self.v3),
            (0x08, self.ve),
        ] {
            if self.mix & bit != 0 {
                vi += v;
                inputs += 1;
            }
        }
        let mut vf = 0i32;
        let mut filter_inputs = 0usize;
        for (bit, v) in [(0x10, self.vlp), (0x20, self.vbp), (0x40, self.vhp)] {
            if self.mix & bit != 0 {
                vf += v;
                filter_inputs += 1;
            }
        }
        if filter_inputs > 0 {
            let dc_offset = 32767 * ((1 << 12) - t.filter_gain);
            vi += (vf * t.filter_gain + dc_offset) >> 12;
            inputs += filter_inputs;
        }

        let idx = MIXER_OFFSET[inputs] as i32 + vi;
        debug_assert!((0..t.mixer.len() as i32).contains(&idx));
        let vo = usize::from(t.mixer[clamp_index(idx, t.mixer.len())]);
        (i32::from(t.gain[(usize::from(self.vol) << 16) + vo]) - (1 << 15)) as i16
    }

    /// Set the filter cutoff operating point from `fc` (reSID `set_w0`).
    fn set_w0(&mut self) {
        let t = tables(self.model);
        match self.model {
            SidModel::Mos6581 => {
                let vw = self.vw_bias + i32::from(t.f0_dac[usize::from(self.fc)]);
                let d = (t.kvddt - vw) as u32;
                self.vddt_vw_2 = d.wrapping_mul(d) >> 1;
            }
            SidModel::Mos8580 => {
                // MOS 8580 cutoff: 0 - 12.5 kHz.
                self.n_dac = (t.n_param * i32::from(t.f0_dac[usize::from(self.fc)])) >> 11;
            }
        }
    }

    /// Derive the summer / mixer input routing (reSID `set_sum_mix`).
    ///
    /// NB: voice3off (mode bit 7) only silences voice 3 when it is routed
    /// directly to the mixer — a filtered voice 3 still sounds.
    fn set_sum_mix(&mut self) {
        self.sum = self.filt & VOICE_MASK;
        self.mix =
            ((self.mode & 0x70) | (!(self.filt | ((self.mode & 0x80) >> 5)) & 0x0F)) & VOICE_MASK;
    }

    /// Next dither term: uniform in `0..2^19`, i.e. up to ±2 LSB of the
    /// scaled voice input (reSID `Randomnoise`, made deterministic).
    fn dither(&mut self) -> i32 {
        let mut x = self.dither;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.dither = x;
        (x & 0x7_FFFF) as i32
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::new(SidModel::Mos6581)
    }
}

/// Clamp a bandpass value into its table domain. reSID asserts this instead;
/// out-of-domain values are unreachable in normal operation.
fn clamp_u16(value: i32) -> usize {
    value.clamp(0, 0xFFFF) as usize
}

/// Clamp a summer / mixer index into its table. reSID asserts and would read
/// out of bounds if violated; saturating at the table ends corresponds to
/// pinning the op-amp at its rail.
fn clamp_index(idx: i32, len: usize) -> usize {
    (idx.max(0) as usize).min(len - 1)
}

/// One fixpoint step of the 6581 integrator (reSID `solve_integrate_6581`):
///
/// ```text
///          ---C---
///         |       |
/// vi -----Rw--[A>----- vo
///      |      | vx
///       --Rs--
/// ```
///
/// `Rs` (the "snake") is a triode-mode NMOS at Vdd; `Rw` is the VCR whose gate
/// is driven by the cutoff DAC, crossing subthreshold, triode, and saturation
/// modes — blended continuously by the EKV model tables.
fn solve_integrate_6581(
    vi: i32,
    vx: &mut i32,
    vc: &mut i32,
    vddt_vw_2: u32,
    t: &ModelTables,
) -> i32 {
    // All variables are translated and scaled to fit 16 bits; translations
    // cancel in the subtractions: (a - t) - (b - t) = a - b.
    let kvddt = t.kvddt;

    // "Snake" voltages for triode-mode calculation.
    let vgst = (kvddt - *vx) as u32;
    let vgdt = (kvddt - vi) as u32;
    let vgdt_2 = vgdt.wrapping_mul(vgdt);

    // "Snake" current, scaled by (1/m)*2^13*m*2^16*m*2^16*2^-15 = m*2^30.
    let n_i_snake = t.n_snake * ((vgst.wrapping_mul(vgst).wrapping_sub(vgdt_2) as i32) >> 15);

    // VCR gate voltage: Vg = Vddt - sqrt(((Vddt - Vw)^2 + Vgdt^2)/2).
    let kvg = i32::from(t.vcr_kvg[(vddt_vw_2.wrapping_add(vgdt_2 >> 1) >> 16) as usize]);

    // VCR voltages for the EKV-model table lookup (translated by 2^15).
    let vgs = kvg - *vx + (1 << 15);
    let vgd = kvg - vi + (1 << 15);

    // VCR current, scaled by m*2^15*2^15 = m*2^30.
    let ids_s = u32::from(t.vcr_n_ids_term[clamp_u16(vgs)]);
    let ids_d = u32::from(t.vcr_n_ids_term[clamp_u16(vgd)]);
    let n_i_vcr = (ids_s.wrapping_sub(ids_d) << 15) as i32;

    // Change in capacitor charge.
    *vc = vc.wrapping_sub(n_i_snake.wrapping_add(n_i_vcr));

    // vx = g(vc)
    *vx = i32::from(t.opamp_rev[clamp_u16((*vc >> 15) + (1 << 15))]);

    // Return vo.
    *vx + (*vc >> 14)
}

/// One fixpoint step of the 8580 integrator (reSID `solve_integrate_8580`):
/// the resistance is multiple parallel NMOS transistors selected by the fc
/// bits, with a temperature-compensated gate voltage divider.
fn solve_integrate_8580(
    vi: i32,
    vx: &mut i32,
    vc: &mut i32,
    n_dac: i32,
    nvgt: i32,
    t: &ModelTables,
) -> i32 {
    // DAC voltages.
    let vgst = (nvgt - *vx) as u32;
    let vgdt = if vi < nvgt { (nvgt - vi) as u32 } else { 0 }; // triode/saturation

    // DAC current, scaled by (1/m)*2^13*m*2^16*m*2^16*2^-15 = m*2^30.
    let n_i_rfc = (n_dac
        * ((vgst
            .wrapping_mul(vgst)
            .wrapping_sub(vgdt.wrapping_mul(vgdt)) as i32)
            >> 15))
        >> 4;

    // Change in capacitor charge.
    *vc = vc.wrapping_sub(n_i_rfc);

    // vx = g(vc)
    *vx = i32::from(t.opamp_rev[clamp_u16((*vc >> 15) + (1 << 15))]);

    // Return vo.
    *vx + (*vc >> 14)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_register_writes_pack_the_11_bit_value() {
        let mut f = Filter::new(SidModel::Mos6581);
        f.write_fc_hi(0xFF);
        f.write_fc_lo(0x05);
        assert_eq!(f.cutoff(), (0xFF << 3) | 0x05);
    }

    #[test]
    fn res_filt_write_splits_resonance_and_routing() {
        let mut f = Filter::new(SidModel::Mos6581);
        f.write_res_filt(0x35); // resonance 3, routing 0b0101
        assert_eq!(f.resonance(), 0x3);
        assert_eq!(f.routing(), 0x5);
        assert!(f.voice_routed(0));
        assert!(!f.voice_routed(1));
        assert!(f.voice_routed(2));
    }

    #[test]
    fn mode_vol_write_splits_mode_and_volume() {
        let mut f = Filter::new(SidModel::Mos6581);
        f.write_mode_vol(0x9F); // mode 0x90 (incl. voice3off), volume 0x0F
        assert_eq!(f.mode(), 0x90);
        assert_eq!(f.volume(), 0x0F);
    }

    #[test]
    fn reset_clears_the_registers() {
        let mut f = Filter::new(SidModel::Mos6581);
        f.write_fc_hi(0xFF);
        f.write_res_filt(0xFF);
        f.write_mode_vol(0xFF);
        f.reset();
        assert_eq!(f.cutoff(), 0);
        assert_eq!(f.resonance(), 0);
        assert_eq!(f.routing(), 0);
        assert_eq!(f.volume(), 0);
        assert!(!f.voice_routed(0));
    }

    #[test]
    fn master_volume_attenuates_the_output() {
        // Peak-to-peak swing of an AC square input must shrink as the master
        // volume drops (voices routed straight to the mixer, not the filter).
        let swing_at = |vol: u8| {
            let mut f = Filter::new(SidModel::Mos6581);
            f.write_res_filt(0x00);
            f.write_mode_vol(vol);
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            for i in 0..4000 {
                // Within the real voice-value range (|v| * voice_scale_s14
                // 2442 must fit i32; loud voices reach a few hundred thousand).
                let s = if (i / 50) % 2 == 0 { 500_000 } else { -500_000 };
                f.clock(s, 0, 0);
                let o = i32::from(f.output());
                lo = lo.min(o);
                hi = hi.max(o);
            }
            i64::from(hi - lo)
        };
        assert!(
            swing_at(0x0F) > swing_at(0x00),
            "volume 15 must swing more than volume 0"
        );
    }

    #[test]
    fn clock_does_not_overflow_on_extreme_voice_values() {
        // Regression for #785: a voice past ~879k overflowed the i32 scale
        // multiply (debug panic). The public clock() API accepts any i32.
        let mut f = Filter::new(SidModel::Mos6581);
        f.write_mode_vol(0x0F);
        f.clock(i32::MAX, i32::MIN, 0); // must not panic
        let _ = f.output();
    }
}
