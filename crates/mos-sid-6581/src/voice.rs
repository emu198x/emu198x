//! SID voice: oscillator and waveform generation.
//!
//! Each of the three SID voices has a 24-bit phase accumulator clocked at
//! the CPU rate, four selectable waveforms, ring modulation, hard sync,
//! and a test bit that holds the oscillator at zero.

#![allow(clippy::cast_possible_truncation)]

/// Noise LFSR seed value (matches real 6581 power-on state).
const NOISE_LFSR_SEED: u32 = 0x7F_FFFF;

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
    #[must_use]
    pub fn waveform_output(&self, ring_mod_source_msb: bool) -> u16 {
        let waveform_bits = (self.control >> 4) & 0x0F;

        if waveform_bits == 0 {
            return 0;
        }

        let mut output: u16 = 0xFFF;
        let mut any = false;

        // Triangle (bit 0 of waveform nibble)
        if waveform_bits & 0x01 != 0 {
            let mut tri = self.accumulator;
            // Ring modulation: XOR with source voice MSB
            if self.control & 0x04 != 0 && ring_mod_source_msb {
                tri ^= 0x0080_0000;
            }
            // Fold: if MSB set, invert lower 23 bits
            let val = if tri & 0x0080_0000 != 0 {
                (tri ^ 0x007F_FFFF) >> 11
            } else {
                tri >> 11
            };
            let tri12 = (val & 0xFFF) as u16;
            if any {
                output &= tri12;
            } else {
                output = tri12;
                any = true;
            }
        }

        // Sawtooth (bit 1 of waveform nibble)
        if waveform_bits & 0x02 != 0 {
            let saw12 = ((self.accumulator >> 12) & 0xFFF) as u16;
            if any {
                output &= saw12;
            } else {
                output = saw12;
                any = true;
            }
        }

        // Pulse (bit 2 of waveform nibble)
        if waveform_bits & 0x04 != 0 {
            let pw12 = self.pulse_width & 0xFFF;
            let acc12 = ((self.accumulator >> 12) & 0xFFF) as u16;
            let pulse12 = if acc12 < pw12 { 0xFFF } else { 0x000 };
            if any {
                output &= pulse12;
            } else {
                output = pulse12;
                any = true;
            }
        }

        // Noise (bit 3 of waveform nibble)
        if waveform_bits & 0x08 != 0 {
            // Extract 12 bits from the LFSR at specific tap positions
            // Bits: 20, 18, 14, 11, 9, 5, 2, 0 mapped to output bits 11..4
            let lfsr = self.noise_lfsr;
            let noise12 = (((lfsr >> 20) & 1) << 11)
                | (((lfsr >> 18) & 1) << 10)
                | (((lfsr >> 14) & 1) << 9)
                | (((lfsr >> 11) & 1) << 8)
                | (((lfsr >> 9) & 1) << 7)
                | (((lfsr >> 5) & 1) << 6)
                | (((lfsr >> 2) & 1) << 5)
                | ((lfsr & 1) << 4);
            let noise12 = noise12 as u16;
            if any {
                output &= noise12;
            } else {
                output = noise12;
            }
        }

        output
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
