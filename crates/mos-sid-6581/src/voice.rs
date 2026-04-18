//! SID voice: oscillator, waveform generation, ring mod, sync, and noise.

#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

use crate::SidModel;

const NOISE_LFSR_SEED: u32 = 0x7F_FFFF;
const COMBINED_TRI_SAW_6581: [u8; 8] = [0x00, 0x00, 0x00, 0x18, 0x00, 0x58, 0x78, 0xE8];
const COMBINED_TRI_PULSE_6581: [u8; 8] = [0x00, 0x00, 0x00, 0x08, 0x00, 0x48, 0x68, 0xE8];
const COMBINED_SAW_PULSE_6581: [u8; 8] = [0x00, 0x00, 0x00, 0x28, 0x00, 0x68, 0x88, 0xE8];

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
            let lut_output = match non_noise {
                0x03 => Some(lookup_combined(&COMBINED_TRI_SAW_6581, tri12, saw12)),
                0x05 => Some(lookup_combined(&COMBINED_TRI_PULSE_6581, tri12, pulse12)),
                0x06 => Some(lookup_combined(&COMBINED_SAW_PULSE_6581, saw12, pulse12)),
                0x07 => {
                    let ts = lookup_combined(&COMBINED_TRI_SAW_6581, tri12, saw12);
                    Some(ts & pulse12)
                }
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

fn lookup_combined(table: &[u8; 8], a: u16, b: u16) -> u16 {
    let anded = a & b;
    let idx = ((anded >> 9) & 0x07) as usize;
    u16::from(table[idx]) << 4
}
