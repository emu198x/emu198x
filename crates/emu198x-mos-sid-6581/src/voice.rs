//! SID voice: oscillator, waveform generation, ring mod, sync, and noise.

#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

use crate::SidModel;

use crate::combined_wave_tables::{
    COMBINED_P_T_6581, COMBINED_PS_6581, COMBINED_PST_6581, COMBINED_TRI_SAW_6581,
};

const NOISE_LFSR_SEED: u32 = 0x7F_FFFF;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    pub accumulator: u32,
    pub frequency: u16,
    pub pulse_width: u16,
    pub control: u8,
    pub noise_lfsr: u32,
    pub prev_msb: bool,
}

impl Voice {
    #[must_use]
    pub fn new() -> Self {
        Self {
            accumulator: 0,
            frequency: 0,
            pulse_width: 0,
            control: 0,
            noise_lfsr: NOISE_LFSR_SEED,
            prev_msb: false,
        }
    }

    pub fn clock_accumulator(&mut self) {
        if self.control & 0x08 != 0 {
            self.accumulator = 0;
            self.noise_lfsr = NOISE_LFSR_SEED;
            return;
        }

        self.accumulator = self.accumulator.wrapping_add(u32::from(self.frequency)) & 0x00FF_FFFF;
    }

    pub fn clock_noise(&mut self) {
        let msb19 = self.accumulator & (1 << 19) != 0;
        let prev19 = self.accumulator.wrapping_sub(u32::from(self.frequency)) & (1 << 19) != 0;

        if msb19 && !prev19 {
            let bit17 = (self.noise_lfsr >> 17) & 1;
            let bit22 = (self.noise_lfsr >> 22) & 1;
            let feedback = bit17 ^ bit22;
            self.noise_lfsr = ((self.noise_lfsr << 1) | feedback) & 0x7F_FFFF;
        }
    }

    pub fn apply_sync(&mut self, source_prev_msb: bool, source_curr_msb: bool) {
        if source_curr_msb && !source_prev_msb {
            self.accumulator = 0;
        }
    }

    #[must_use]
    pub fn waveform_output(&self, ring_mod_source_msb: bool, model: SidModel) -> u16 {
        let waveform_bits = (self.control >> 4) & 0x0F;

        if waveform_bits == 0 {
            return 0;
        }

        // TEST bit (control bit 3) holds pulse output HIGH, zeros the
        // accumulator, and reseeds the noise LFSR. Per 6581 datasheet.
        let test_bit = self.control & 0x08 != 0;

        let tri12 = self.triangle_output(ring_mod_source_msb);
        let saw12 = ((self.accumulator >> 12) & 0xFFF) as u16;
        // The 12-bit comparator drives the pulse line high once the upper
        // accumulator bits reach the pulse width: high while `acc >= PW`.
        // reSID `wave.h` (`pulse_output = (accumulator >> 12) >= pw`), reSIDfp
        // alike, and VICE `testprogs/SID/osc3-wave0` on hardware: PW $000
        // reads OSC3 $FF, PW $FFF reads $00. The datasheet's "0 or $FFF ...
        // constant DC" holds for either polarity, so it cannot settle this.
        let pulse12 = if test_bit {
            0x0FFF
        } else {
            let pw12 = self.pulse_width & 0x0FFF;
            let acc12 = ((self.accumulator >> 12) & 0x0FFF) as u16;
            if acc12 >= pw12 { 0x0FFF } else { 0x0000 }
        };
        let noise12 = self.noise_output();

        let non_noise = waveform_bits & 0x07;
        let count = non_noise.count_ones();

        if waveform_bits.is_power_of_two() {
            return match waveform_bits {
                0x01 => tri12,
                0x02 => saw12,
                0x04 => pulse12,
                0x08 => noise12,
                _ => 0,
            };
        }

        if model == SidModel::Mos6581 && count >= 2 {
            // reSID combined-waveform tables are 4096-entry ROM samples
            // from real 6581 chips, indexed by the upper 12 bits of the
            // 24-bit accumulator. Pulse is a separate 0x000/0xFFF mask
            // ANDed with the table output (matches reSID wave.h:467).
            // The index carries the ring-modulated MSB, as reSID's does, so
            // pulse+triangle with RING set reads the substituted half.
            let idx =
                ((self.ring_modulated_accumulator(ring_mod_source_msb) >> 12) & 0x0FFF) as usize;
            let lut_output = match non_noise {
                0x03 => Some(COMBINED_TRI_SAW_6581[idx]),
                0x05 => Some(COMBINED_P_T_6581[idx] & pulse12),
                0x06 => Some(COMBINED_PS_6581[idx] & pulse12),
                0x07 => Some(COMBINED_PST_6581[idx] & pulse12),
                _ => None,
            };
            if let Some(value) = lut_output {
                if waveform_bits & 0x08 != 0 {
                    return value & noise12;
                }
                return value;
            }
        }

        let mut output: u16 = 0x0FFF;
        if waveform_bits & 0x01 != 0 {
            output &= tri12;
        }
        if waveform_bits & 0x02 != 0 {
            output &= saw12;
        }
        if waveform_bits & 0x04 != 0 {
            output &= pulse12;
        }
        if waveform_bits & 0x08 != 0 {
            output &= noise12;
        }
        output
    }

    /// The accumulator as the triangle XOR logic sees it.
    ///
    /// Ring modulation substitutes this voice's MSB with
    /// `MSB EOR NOT source-MSB`. Die analysis gives the XOR input as
    /// `TriXOR = !Saw & ((!V3 & Ring) ^ bit23)`, so the substitution is
    /// suppressed when sawtooth is co-selected (sawtooth blocks the MSB out of
    /// the EOR). Per reSID `wave.cc` `writeCONTROL_REG` (`ring_msb_mask`) and
    /// `wave.h` `set_waveform_output` (`accumulator ^ (~sync_source &
    /// ring_msb_mask)`), and VICE `testprogs/SID/ringmod`, which reads `$FF`
    /// from OSC3 for a ring-modulated triangle with both oscillators at zero.
    /// reSIDfp shares the expression. The datasheet only says RING "replaces
    /// the Triangle waveform output ... with a Ring Modulated combination";
    /// it does not give the polarity. Same on the 6581 and 8580.
    fn ring_modulated_accumulator(&self, ring_mod_source_msb: bool) -> u32 {
        let ring = self.control & 0x04 != 0;
        let saw = self.control & 0x20 != 0;
        if ring && !saw && !ring_mod_source_msb {
            self.accumulator ^ 0x0080_0000
        } else {
            self.accumulator
        }
    }

    fn triangle_output(&self, ring_mod_source_msb: bool) -> u16 {
        let tri = self.ring_modulated_accumulator(ring_mod_source_msb);
        let value = if tri & 0x0080_0000 != 0 {
            (tri ^ 0x007F_FFFF) >> 11
        } else {
            tri >> 11
        };
        (value & 0x0FFF) as u16
    }

    fn noise_output(&self) -> u16 {
        // 6581 noise waveform samples LFSR bits
        // 22, 20, 16, 13, 11, 7, 4, 2 into output bits 11..=4 (MSB-aligned
        // 12-bit waveform). Per 6581 datasheet / reSID reference.
        let lfsr = self.noise_lfsr;
        (((lfsr >> 22) & 1) << 11
            | ((lfsr >> 20) & 1) << 10
            | ((lfsr >> 16) & 1) << 9
            | ((lfsr >> 13) & 1) << 8
            | ((lfsr >> 11) & 1) << 7
            | ((lfsr >> 7) & 1) << 6
            | ((lfsr >> 4) & 1) << 5
            | ((lfsr >> 2) & 1) << 4) as u16
    }

    #[must_use]
    pub fn msb(&self) -> bool {
        self.accumulator & 0x0080_0000 != 0
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Control-register waveform-select bits (control >> 4).
    const TRI: u8 = 0x10;
    const SAW: u8 = 0x20;
    const PULSE: u8 = 0x40;
    const NOISE: u8 = 0x80;
    const RING: u8 = 0x04;
    const TEST: u8 = 0x08;

    #[test]
    fn sawtooth_is_the_upper_12_accumulator_bits() {
        let mut v = Voice::new();
        v.control = SAW;
        v.accumulator = 0x00AB_C000;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0xABC);
    }

    #[test]
    fn pulse_is_high_from_the_pulse_width_up_and_low_below() {
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.control = PULSE;
            v.pulse_width = 0x800;
            v.accumulator = 0x0040_0000; // acc12 = 0x400 < 0x800
            assert_eq!(v.waveform_output(false, model), 0x0000);
            v.accumulator = 0x0080_0000; // acc12 = 0x800 == PW
            assert_eq!(v.waveform_output(false, model), 0x0FFF);
            v.accumulator = 0x00C0_0000; // acc12 = 0xC00 > 0x800
            assert_eq!(v.waveform_output(false, model), 0x0FFF);
        }
    }

    #[test]
    fn pulse_width_extremes_match_the_osc3_wave0_testprog() {
        // VICE testprogs/SID/osc3-wave0: PW $FFF reads OSC3 $00 and PW $000
        // reads $FF with the oscillator stopped at zero.
        let mut v = Voice::new();
        v.control = PULSE;
        v.accumulator = 0;
        v.pulse_width = 0xFFF;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0000);
        v.pulse_width = 0x000;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0FFF);
    }

    #[test]
    fn no_waveform_selected_outputs_zero() {
        let v = Voice::new();
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0);
    }

    #[test]
    fn test_bit_holds_pulse_high_and_resets_the_oscillator() {
        let mut v = Voice::new();
        v.control = PULSE | TEST;
        v.accumulator = 0x00C0_0000; // would read low without TEST
        v.noise_lfsr = 0x0000_1234;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0FFF);
        v.clock_accumulator();
        assert_eq!(v.accumulator, 0, "TEST zeros the accumulator");
        assert_eq!(v.noise_lfsr, NOISE_LFSR_SEED, "TEST reseeds the noise LFSR");
    }

    #[test]
    fn ring_mod_substitutes_msb_eor_not_source_msb() {
        // TriXOR = !Saw & ((!V3 & Ring) ^ bit23): with both MSBs clear the
        // triangle is inverted (VICE testprogs/SID/ringmod reads OSC3 = $FF),
        // and a set source MSB leaves it unfolded.
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.control = TRI | RING;
            v.accumulator = 0;
            assert_eq!(
                v.waveform_output(false, model),
                0xFFF,
                "clear source MSB inverts the triangle ({model:?})"
            );
            assert_eq!(
                v.waveform_output(true, model),
                0x000,
                "set source MSB leaves the triangle unfolded ({model:?})"
            );
        }
    }

    #[test]
    fn ring_mod_without_ring_bit_ignores_the_source() {
        let mut v = Voice::new();
        v.control = TRI;
        v.accumulator = 0;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x000);
        assert_eq!(v.waveform_output(true, SidModel::Mos6581), 0x000);
    }

    #[test]
    fn sawtooth_blocks_the_ring_mod_substitution() {
        // With sawtooth co-selected the MSB never reaches the EOR, so the
        // tri+saw table is indexed by the raw accumulator whatever the source.
        let mut v = Voice::new();
        v.control = TRI | SAW | RING;
        v.accumulator = 0x0055_5000;
        let raw = COMBINED_TRI_SAW_6581[0x555];
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), raw);
        assert_eq!(v.waveform_output(true, SidModel::Mos6581), raw);
    }

    #[test]
    fn ring_mod_flips_the_pulse_triangle_table_index() {
        // reSID indexes every combined table with the ring-substituted
        // accumulator; pulse+triangle has no sawtooth, so RING applies.
        let mut v = Voice::new();
        v.control = TRI | PULSE | RING;
        v.pulse_width = 0x000; // pulse held high
        v.accumulator = 0x009F_F000;
        assert_eq!(
            v.waveform_output(false, SidModel::Mos6581),
            COMBINED_P_T_6581[0x1FF],
            "clear source MSB reads the opposite half of the table"
        );
        assert_ne!(COMBINED_P_T_6581[0x1FF], COMBINED_P_T_6581[0x9FF]);
        assert_eq!(
            v.waveform_output(true, SidModel::Mos6581),
            COMBINED_P_T_6581[0x9FF]
        );
    }

    #[test]
    fn hard_sync_zeros_the_accumulator_only_on_the_source_rising_edge() {
        let mut v = Voice::new();
        v.accumulator = 0x0012_3456;
        v.apply_sync(true, true); // no rising edge
        assert_eq!(v.accumulator, 0x0012_3456);
        v.apply_sync(false, true); // rising edge
        assert_eq!(v.accumulator, 0);
    }

    #[test]
    fn combined_tri_saw_uses_the_6581_sampled_table() {
        let mut v = Voice::new();
        v.control = TRI | SAW;
        v.accumulator = 0x0055_5000;
        let idx = ((v.accumulator >> 12) & 0x0FFF) as usize;
        assert_eq!(
            v.waveform_output(false, SidModel::Mos6581),
            COMBINED_TRI_SAW_6581[idx],
            "6581 combined waveform reads the sampled ROM table, not a bitwise AND"
        );
    }

    #[test]
    fn noise_lfsr_advances_only_on_the_bit19_rising_edge() {
        let mut v = Voice::new();
        v.frequency = 1;
        v.accumulator = 0x0008_0000; // bit19 set, prev (acc-1) clear → rising
        let before = v.noise_lfsr;
        v.clock_noise();
        assert_ne!(v.noise_lfsr, before, "LFSR clocks on the rising edge");
        v.accumulator = 0x0008_0002; // bit19 stays set → no edge
        let held = v.noise_lfsr;
        v.clock_noise();
        assert_eq!(v.noise_lfsr, held, "no clock without an edge");
    }

    #[test]
    fn msb_reflects_accumulator_bit_23() {
        let mut v = Voice::new();
        v.accumulator = 0x0080_0000;
        assert!(v.msb());
        v.accumulator = 0x007F_FFFF;
        assert!(!v.msb());
    }

    #[test]
    fn noise_waveform_selects_lfsr_bits() {
        let mut v = Voice::new();
        v.control = NOISE;
        v.noise_lfsr = NOISE_LFSR_SEED; // all ones → all sampled bits set
        // Sampled into output bits 11..=4, so 0xFF0.
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0xFF0);
    }
}
