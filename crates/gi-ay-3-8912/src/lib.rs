//! General Instrument AY-3-8912 Programmable Sound Generator.
//!
//! Source references:
//! - `wiki/chips/gi-ay-3-8912.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/gi-ay-3-8912/src/lib.rs`
//!
//! 3 square-wave tone channels, 1 noise generator, and an envelope
//! generator. Used in the ZX Spectrum 128K, Amstrad CPC, Atari ST
//! (as the Yamaha YM2149 clone), and many arcade machines.
//!
//! On the Spectrum 128K:
//! - AY clock = CPU clock / 2 = 1.7734 MHz
//! - Register select: OUT to port $FFFD
//! - Data write: OUT to port $BFFD
//! - Data read: IN from port $FFFD

/// Logarithmic volume table for the AY-3-8912.
/// 16 levels (0 = silent, 15 = maximum). The curve approximates
/// the real chip's DAC output measured by various sources.
static VOLUME: [f32; 16] = [
    0.0000, 0.0137, 0.0205, 0.0291, 0.0423, 0.0618, 0.0847, 0.1369, 0.1691, 0.2647, 0.3527, 0.4499,
    0.5704, 0.6873, 0.8482, 1.0000,
];

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Ay3_8912 {
    /// The 16 registers (directly readable/writable).
    regs: [u8; 16],
    /// Currently selected register (0-15).
    selected: u8,

    // Tone generators (3 channels)
    tone_counter: [u16; 3],
    tone_output: [bool; 3],

    // Noise generator
    noise_counter: u16,
    noise_output: bool,
    /// 17-bit LFSR, taps at bits 0 and 3.
    noise_lfsr: u32,

    // Envelope generator
    env_counter: u32,
    env_step: u8,
    env_holding: bool,
    /// Current envelope output level (0-15).
    env_level: u8,

    /// Internal clock prescaler (0-7). The AY divides its input clock
    /// by 8 before driving tone, noise, and envelope counters.
    prescaler: u8,

    // Audio output accumulation (Bresenham-style integer timing)
    /// Accumulated output level for the current audio sample.
    sample_accum: f32,
    /// Number of AY ticks accumulated in the current sample.
    sample_ticks: u32,
    /// Bresenham error accumulator for sample timing.
    sample_error: u32,
    /// AY clock rate (for Bresenham division).
    ay_clock_hz: u32,
    /// Audio sample rate (for Bresenham division).
    sample_rate: u32,
    /// Output sample buffer for the current frame.
    samples: Vec<f32>,
    /// Number of samples written this frame.
    samples_written: usize,
}

impl Ay3_8912 {
    /// Create a new AY chip.
    ///
    /// - `ay_clock_hz`: AY clock frequency (e.g., 1_773_400 for 128K Spectrum)
    /// - `sample_rate`: audio output sample rate (e.g., 44100)
    /// - `samples_per_frame`: pre-allocated buffer size
    pub fn new(ay_clock_hz: u32, sample_rate: u32, samples_per_frame: usize) -> Self {
        Self {
            regs: [0; 16],
            selected: 0,
            tone_counter: [0; 3],
            tone_output: [false; 3],
            noise_counter: 0,
            noise_output: false,
            noise_lfsr: 1, // Must be non-zero
            env_counter: 0,
            env_step: 0,
            env_holding: false,
            env_level: 0,
            prescaler: 0,
            sample_accum: 0.0,
            sample_ticks: 0,
            sample_error: 0,
            ay_clock_hz,
            sample_rate,
            samples: vec![0.0; samples_per_frame],
            samples_written: 0,
        }
    }

    /// Select which register (0-15) subsequent reads/writes address.
    /// On the Spectrum: OUT to port $FFFD.
    pub fn select_register(&mut self, reg: u8) {
        self.selected = reg & 0x0F;
    }

    /// Write a value to the currently selected register.
    /// On the Spectrum: OUT to port $BFFD.
    pub fn write_data(&mut self, val: u8) {
        let reg = self.selected as usize;
        // Mask register values to their valid bit widths
        let masked = match reg {
            1 | 3 | 5 => val & 0x0F,  // Coarse tone: 4 bits
            6 => val & 0x1F,          // Noise period: 5 bits
            7 => val,                 // Mixer: all 8 bits
            8 | 9 | 10 => val & 0x1F, // Volume + envelope mode: 5 bits
            13 => {
                // Writing to envelope shape resets the envelope
                self.env_step = 0;
                self.env_counter = 0;
                self.env_holding = false;
                val & 0x0F
            }
            _ => val,
        };
        self.regs[reg] = masked;
    }

    /// Read the currently selected register's value.
    /// On the Spectrum: IN from port $FFFD.
    pub fn read_data(&self) -> u8 {
        self.regs[self.selected as usize]
    }

    /// Advance one AY clock cycle. Call at ay_clock_hz rate.
    ///
    /// The AY divides its input clock by 8 internally. Tone, noise,
    /// and envelope counters only advance every 8th input clock.
    /// Audio output is sampled every tick for accurate downsampling.
    pub fn tick(&mut self) {
        // Prescaler: divide input clock by 8
        self.prescaler += 1;
        if self.prescaler >= 8 {
            self.prescaler = 0;

            // -- Tone generators --
            for ch in 0..3 {
                if self.tone_counter[ch] == 0 {
                    let period = self.tone_period(ch);
                    self.tone_counter[ch] = period.max(1);
                    self.tone_output[ch] = !self.tone_output[ch];
                }
                self.tone_counter[ch] -= 1;
            }

            // -- Noise generator --
            if self.noise_counter == 0 {
                let period = (self.regs[6] & 0x1F) as u16;
                self.noise_counter = period.max(1);
                // 17-bit LFSR: new bit = bit 0 XOR bit 3
                let bit = (self.noise_lfsr ^ (self.noise_lfsr >> 3)) & 1;
                self.noise_lfsr = (self.noise_lfsr >> 1) | (bit << 16);
                self.noise_output = self.noise_lfsr & 1 != 0;
            }
            self.noise_counter -= 1;

            // -- Envelope generator --
            if !self.env_holding {
                if self.env_counter == 0 {
                    let period = self.envelope_period();
                    self.env_counter = period.max(1);
                    self.advance_envelope();
                }
                self.env_counter -= 1;
            }
        }

        // -- Compute output (sampled at full AY clock rate for accurate downsampling) --
        let output = self.compute_output();

        // -- Bresenham-style audio downsampling --
        // Accumulate output for averaging over each audio sample period.
        self.sample_accum += output;
        self.sample_ticks += 1;

        // Emit a sample when the Bresenham accumulator overflows.
        // This evenly distributes sample_rate samples across ay_clock_hz ticks
        // with zero floating-point drift.
        self.sample_error += self.sample_rate;
        if self.sample_error >= self.ay_clock_hz {
            self.sample_error -= self.ay_clock_hz;
            if self.samples_written < self.samples.len() {
                self.samples[self.samples_written] = self.sample_accum / self.sample_ticks as f32;
                self.samples_written += 1;
            }
            self.sample_accum = 0.0;
            self.sample_ticks = 0;
        }
    }

    /// Finish the frame and write audio samples to the output buffer.
    /// Samples are in the range 0.0 to 1.0.
    pub fn end_frame(&mut self, out: &mut [f32]) {
        // Flush any remaining partial sample (carries the accumulator
        // state across frames — no discontinuity at boundaries).
        if self.sample_ticks > 0 {
            if self.samples_written < self.samples.len() {
                self.samples[self.samples_written] = self.sample_accum / self.sample_ticks as f32;
                self.samples_written += 1;
            }
            // Don't reset sample_accum/sample_ticks — the partial sample
            // continues into the next frame for seamless audio.
            self.sample_accum = 0.0;
            self.sample_ticks = 0;
        }

        let n = out.len().min(self.samples_written);
        out[..n].copy_from_slice(&self.samples[..n]);
        // Zero any remaining output slots
        for s in &mut out[n..] {
            *s = 0.0;
        }
        self.samples_written = 0;
    }

    /// Number of samples generated this frame so far.
    pub fn samples_per_frame(&self) -> usize {
        self.samples.len()
    }

    // -- Internal helpers --

    fn tone_period(&self, ch: usize) -> u16 {
        let fine = self.regs[ch * 2] as u16;
        let coarse = (self.regs[ch * 2 + 1] & 0x0F) as u16;
        (coarse << 8) | fine
    }

    fn envelope_period(&self) -> u32 {
        let fine = self.regs[11] as u32;
        let coarse = self.regs[12] as u32;
        (coarse << 8) | fine
    }

    fn advance_envelope(&mut self) {
        let shape = self.regs[13] & 0x0F;
        let cont = shape & 0x08 != 0;
        let attack = shape & 0x04 != 0;
        let alternate = shape & 0x02 != 0;
        let hold = shape & 0x01 != 0;

        self.env_step += 1;

        if self.env_step >= 16 {
            if cont {
                if hold {
                    // Hold at final value
                    self.env_holding = true;
                    self.env_step = 15;
                    self.env_level = if attack ^ alternate { 0 } else { 15 };
                } else if alternate {
                    // Reverse direction
                    self.env_step = 0;
                    // The level computation handles the direction
                } else {
                    // Repeat
                    self.env_step = 0;
                }
            } else {
                // No continue: hold at 0
                self.env_holding = true;
                self.env_step = 15;
                self.env_level = 0;
                return;
            }
        }

        // Compute current envelope level
        let step = self.env_step & 0x0F;
        let cycle = (self.env_step / 16) & 1;
        let direction_up = if alternate {
            attack ^ (cycle != 0)
        } else {
            attack
        };

        self.env_level = if direction_up { step } else { 15 - step };
    }

    fn compute_output(&self) -> f32 {
        let mixer = self.regs[7];
        let mut total = 0.0f32;

        for ch in 0..3 {
            let tone_enable = mixer & (1 << ch) == 0; // Active low
            let noise_enable = mixer & (8 << ch) == 0; // Active low

            let tone_out = !tone_enable || self.tone_output[ch];
            let noise_out = !noise_enable || self.noise_output;
            let channel_on = tone_out && noise_out;

            let vol_reg = self.regs[8 + ch];
            let level = if vol_reg & 0x10 != 0 {
                // Envelope mode
                self.env_level
            } else {
                vol_reg & 0x0F
            };

            let amplitude = if channel_on {
                VOLUME[level as usize]
            } else {
                0.0
            };

            total += amplitude;
        }

        // Normalize: max is 3.0 (3 channels at full volume)
        total / 3.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_read_write() {
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        ay.select_register(0);
        ay.write_data(0xAB);
        assert_eq!(ay.read_data(), 0xAB);

        // Coarse register is 4-bit
        ay.select_register(1);
        ay.write_data(0xFF);
        assert_eq!(ay.read_data(), 0x0F);
    }

    #[test]
    fn noise_period_masked() {
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        ay.select_register(6);
        ay.write_data(0xFF);
        assert_eq!(ay.read_data(), 0x1F);
    }

    #[test]
    fn silent_by_default() {
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        // Mixer defaults to 0: all tone and noise enabled (active low = enabled)
        // But volume defaults to 0, so output should be silent
        for _ in 0..1000 {
            ay.tick();
        }
        let mut out = vec![0.0f32; 882];
        ay.end_frame(&mut out);
        let max = out.iter().cloned().fold(0.0f32, f32::max);
        assert!(max < 0.01, "expected silence, got max={}", max);
    }

    #[test]
    fn tone_produces_output() {
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        // Channel A: period = 100, volume = 15
        ay.select_register(0);
        ay.write_data(100); // Fine tune
        ay.select_register(1);
        ay.write_data(0); // Coarse tune
        ay.select_register(7);
        ay.write_data(0x3E); // Enable tone A only (bit 0 = 0)
        ay.select_register(8);
        ay.write_data(15); // Volume A = max

        // Tick for a frame's worth of AY clocks (~35,000)
        for _ in 0..35_000 {
            ay.tick();
        }
        let mut out = vec![0.0f32; 882];
        ay.end_frame(&mut out);
        let max = out.iter().cloned().fold(0.0f32, f32::max);
        assert!(max > 0.1, "expected audible output, got max={}", max);
    }

    #[test]
    fn detection_pattern_works() {
        // Mimics what Signal Part 3 does: write to register, read back
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        ay.select_register(8); // Volume A register
        ay.write_data(0x08); // Write a value
        let val = ay.read_data();
        assert_eq!(
            val & 0x0F,
            0x08,
            "AY detection should read back the written value"
        );
    }
}
