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
        let pulse12 = if test_bit {
            0x0FFF
        } else {
            let pw12 = self.pulse_width & 0x0FFF;
            let acc12 = ((self.accumulator >> 12) & 0x0FFF) as u16;
            if acc12 < pw12 { 0x0FFF } else { 0x0000 }
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
            let idx = ((self.accumulator >> 12) & 0x0FFF) as usize;
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

    fn triangle_output(&self, ring_mod_source_msb: bool) -> u16 {
        let mut tri = self.accumulator;
        if self.control & 0x04 != 0 && ring_mod_source_msb {
            tri ^= 0x0080_0000;
        }
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
    fn pulse_is_high_below_the_pulse_width_and_low_above() {
        let mut v = Voice::new();
        v.control = PULSE;
        v.pulse_width = 0x800;
        v.accumulator = 0x0040_0000; // acc12 = 0x400 < 0x800
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0FFF);
        v.accumulator = 0x00C0_0000; // acc12 = 0xC00 >= 0x800
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0000);
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
    fn ring_mod_folds_the_triangle_on_the_source_msb() {
        let mut v = Voice::new();
        v.control = TRI | RING;
        v.accumulator = 0; // unfolded triangle = 0
        let no_fold = v.waveform_output(false, SidModel::Mos6581);
        let fold = v.waveform_output(true, SidModel::Mos6581);
        assert_eq!(no_fold, 0x000);
        assert_eq!(fold, 0xFFF, "source MSB folds the triangle");
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
