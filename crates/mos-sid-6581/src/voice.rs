//! SID voice: oscillator and waveform generation.
//!
//! Each of the three SID voices has a 24-bit phase accumulator clocked at
//! the CPU rate, four selectable waveforms, ring modulation, hard sync,
//! and a test bit that holds the oscillator at zero.
//!
//! # Combined waveforms
//!
//! When multiple waveform bits are set simultaneously, the 8580 ANDs the
//! individual outputs (simple logic). The 6581 produces specific bit
//! patterns from die analysis that differ from a pure AND — these are
//! stored as 8-entry lookup tables per combination.

#![allow(clippy::cast_possible_truncation)]

use crate::SidModel;

/// Noise LFSR seed value (matches real 6581 power-on state).
const NOISE_LFSR_SEED: u32 = 0x7F_FFFF;

/// 6581 combined waveform: triangle + sawtooth.
/// 8 entries indexed by top 3 bits of the 12-bit output.
/// Values from reSID die analysis (top 8 bits; we scale to 12-bit).
const COMBINED_TRI_SAW_6581: [u8; 8] = [0x00, 0x00, 0x00, 0x18, 0x00, 0x58, 0x78, 0xE8];

/// 6581 combined waveform: triangle + pulse.
const COMBINED_TRI_PULSE_6581: [u8; 8] = [0x00, 0x00, 0x00, 0x08, 0x00, 0x48, 0x68, 0xE8];

/// 6581 combined waveform: sawtooth + pulse.
const COMBINED_SAW_PULSE_6581: [u8; 8] = [0x00, 0x00, 0x00, 0x28, 0x00, 0x68, 0x88, 0xE8];

/// A single SID voice oscillator.
pub struct Voice {
    /// 24-bit phase accumulator.
    pub accumulator: u32,
    /// 16-bit frequency register (`freq_lo | freq_hi << 8`).
    pub frequency: u16,
    /// 12-bit pulse width register (`pw_lo | pw_hi << 4`).
    pub pulse_width: u16,
    /// Control register ($04/$0B/$12).
    pub control: u8,
    /// 23-bit noise LFSR.
    pub noise_lfsr: u32,
    /// Previous MSB of accumulator (for sync/noise clock detection).
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

    /// Step the phase accumulator by the frequency register.
    ///
    /// If the test bit is set, hold the accumulator at 0 and reset the
    /// noise LFSR.
    pub fn clock_accumulator(&mut self) {
        if self.control & 0x08 != 0 {
            // Test bit: hold at 0, reset noise LFSR
            self.accumulator = 0;
            self.noise_lfsr = NOISE_LFSR_SEED;
            return;
        }

        self.accumulator = (self.accumulator.wrapping_add(u32::from(self.frequency))) & 0x00FF_FFFF;
    }

    /// Clock the noise LFSR when bit 19 of the accumulator has a rising edge.
    pub fn clock_noise(&mut self) {
        let msb19 = self.accumulator & (1 << 19) != 0;
        let prev19 = (self.accumulator.wrapping_sub(u32::from(self.frequency))) & (1 << 19) != 0;

        if msb19 && !prev19 {
            // Feedback: bit 17 XOR bit 22
            let bit17 = (self.noise_lfsr >> 17) & 1;
            let bit22 = (self.noise_lfsr >> 22) & 1;
            let feedback = bit17 ^ bit22;
            self.noise_lfsr = ((self.noise_lfsr << 1) | feedback) & 0x7F_FFFF;
        }
    }

    /// Apply hard sync: if the sync source's MSB had a rising edge,
    /// reset this voice's accumulator.
    pub fn apply_sync(&mut self, source_prev_msb: bool, source_curr_msb: bool) {
        if source_curr_msb && !source_prev_msb {
            self.accumulator = 0;
        }
    }

    /// Compute the 12-bit waveform output.
    ///
    /// `ring_mod_source_msb` is the MSB of the ring-modulation source voice's
    /// accumulator (voice 2 for voice 0, etc.).
    ///
    /// `model` selects 6581 (lookup-table) or 8580 (AND) combined waveforms.
    #[must_use]
    pub fn waveform_output(&self, ring_mod_source_msb: bool, model: SidModel) -> u16 {
        let waveform_bits = (self.control >> 4) & 0x0F;

        if waveform_bits == 0 {
            return 0;
        }

        // Compute individual waveforms
        let tri12 = self.triangle_output(ring_mod_source_msb);
        let saw12 = ((self.accumulator >> 12) & 0xFFF) as u16;
        let pulse12 = {
            let pw12 = self.pulse_width & 0xFFF;
            let acc12 = ((self.accumulator >> 12) & 0xFFF) as u16;
            if acc12 < pw12 { 0xFFF } else { 0x000 }
        };
        let noise12 = self.noise_output();

        // Count how many non-noise waveforms are selected
        let non_noise = waveform_bits & 0x07;
        let count = non_noise.count_ones();

        // Single waveform: return it directly
        if waveform_bits.count_ones() == 1 {
            return match waveform_bits {
                0x01 => tri12,
                0x02 => saw12,
                0x04 => pulse12,
                0x08 => noise12,
                _ => 0,
            };
        }

        // Combined waveforms: 6581 uses lookup tables, 8580 uses AND
        if model == SidModel::Mos6581 && count >= 2 {
            // For 6581, use the die-analysis lookup tables
            let lut_output = match non_noise {
                0x03 => Some(lookup_combined(&COMBINED_TRI_SAW_6581, tri12, saw12)),
                0x05 => Some(lookup_combined(&COMBINED_TRI_PULSE_6581, tri12, pulse12)),
                0x06 => Some(lookup_combined(&COMBINED_SAW_PULSE_6581, saw12, pulse12)),
                0x07 => {
                    // tri+saw+pulse: AND the tri+saw LUT result with pulse
                    let ts = lookup_combined(&COMBINED_TRI_SAW_6581, tri12, saw12);
                    Some(ts & pulse12)
                }
                _ => None,
            };
            if let Some(val) = lut_output {
                // AND with noise if noise bit also set
                if waveform_bits & 0x08 != 0 {
                    return val & noise12;
                }
                return val;
            }
        }

        // 8580 or fallback: AND all selected waveforms together
        let mut output: u16 = 0xFFF;
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

    /// Compute the 12-bit triangle output (with ring modulation).
    fn triangle_output(&self, ring_mod_source_msb: bool) -> u16 {
        let mut tri = self.accumulator;
        if self.control & 0x04 != 0 && ring_mod_source_msb {
            tri ^= 0x0080_0000;
        }
        let val = if tri & 0x0080_0000 != 0 {
            (tri ^ 0x007F_FFFF) >> 11
        } else {
            tri >> 11
        };
        (val & 0xFFF) as u16
    }

    /// Compute the 12-bit noise output from the LFSR.
    fn noise_output(&self) -> u16 {
        let lfsr = self.noise_lfsr;
        (((lfsr >> 20) & 1) << 11
            | ((lfsr >> 18) & 1) << 10
            | ((lfsr >> 14) & 1) << 9
            | ((lfsr >> 11) & 1) << 8
            | ((lfsr >> 9) & 1) << 7
            | ((lfsr >> 5) & 1) << 6
            | ((lfsr >> 2) & 1) << 5
            | (lfsr & 1) << 4) as u16
    }

    /// MSB of the accumulator (bit 23).
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

/// Look up a 6581 combined waveform from an 8-entry table.
///
/// The table is indexed by the top 3 bits of the AND of the two waveforms.
/// This approximates the analog interaction on the 6581 die.
fn lookup_combined(table: &[u8; 8], a: u16, b: u16) -> u16 {
    let anded = a & b;
    let idx = ((anded >> 9) & 0x07) as usize;
    // Scale the 8-bit table entry to 12-bit
    u16::from(table[idx]) << 4
}
