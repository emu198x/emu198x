//! Game Boy DMG APU.
//!
//! Four channels mixed to stereo `f32`:
//!
//! - CH1: square wave with frequency sweep
//! - CH2: square wave (no sweep)
//! - CH3: 32-step wave-table playback over `$FF30..$FF3F`
//! - CH4: LFSR noise (15-bit or 7-bit)
//!
//! Driven at the master clock rate (one [`Apu::tick`] per T-cycle).
//! The frame sequencer steps on the falling edge of the timer's
//! internal counter bit 12 (≈ 512 Hz). Length counters tick on
//! steps 0/2/4/6 (256 Hz), sweep on steps 2/6 (128 Hz), envelope
//! on step 7 (64 Hz). Sample emission happens at 48 kHz stereo via
//! a fractional accumulator over the master clock.
//!
//! Ported from `~/Projects/Emu198x-Zig/src/apu.zig`. The Zig
//! sample-rate constant was off by 2× (it counted stereo halves
//! against a halved master-clock divisor); this port uses the real
//! 4.194304 MHz master and emits one stereo pair per accumulator
//! wraparound, giving exactly 48 kHz.

mod noise;
mod square;
mod wave;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::noise::Noise;
use crate::square::Square;
use crate::wave::Wave;

/// MMIO address ranges owned by the APU.
pub const REG_NR10: u16 = 0xFF10;
pub const REG_NR52: u16 = 0xFF26;
pub const WAVE_RAM_START: u16 = 0xFF30;
pub const WAVE_RAM_END: u16 = 0xFF3F;

/// Master clock the APU is ticked at.
pub const MASTER_HZ: u32 = 4_194_304;
/// Output sample rate per channel.
pub const SAMPLE_RATE_HZ: u32 = 48_000;

/// Game Boy DMG APU.
#[derive(Clone, Serialize, Deserialize)]
pub struct Apu {
    enabled: bool,

    ch1: Square,
    ch2: Square,
    ch3: Wave,
    ch4: Noise,

    /// NR50: master volume + VIN panning.
    nr50: u8,
    /// NR51: per-channel left/right routing.
    nr51: u8,

    wave_ram: [u8; 16],

    /// Frame sequencer step (0..=7).
    frame_step: u8,
    /// Previous state of timer counter bit 12 — the frame sequencer
    /// clocks on its falling edge.
    prev_div_bit: bool,

    /// Fractional accumulator for sample-rate conversion (master →
    /// 48 kHz). One stereo sample emitted per wraparound.
    sample_counter: u32,

    /// Output ring of stereo pairs (left, right interleaved). Not
    /// part of the emulated machine state — drained per frame.
    #[serde(skip)]
    samples: VecDeque<f32>,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    /// Creates an APU in the documented power-off state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: false,
            ch1: Square::new(true),
            ch2: Square::new(false),
            ch3: Wave::new(),
            ch4: Noise::new(),
            nr50: 0,
            nr51: 0,
            wave_ram: [0; 16],
            frame_step: 0,
            prev_div_bit: false,
            sample_counter: 0,
            samples: VecDeque::with_capacity(8192),
        }
    }

    /// Creates an APU in the DMG boot-ROM exit state used when the
    /// machine starts cartridges directly at `$0100`.
    #[must_use]
    pub fn new_post_bootrom_dmg() -> Self {
        Self {
            enabled: true,
            ch1: Square::new_post_bootrom_dmg_ch1(),
            ch2: Square::new(false),
            ch3: Wave::new(),
            ch4: Noise::new(),
            nr50: 0x77,
            nr51: 0xF3,
            wave_ram: [0; 16],
            frame_step: 0,
            prev_div_bit: false,
            sample_counter: 0,
            samples: VecDeque::with_capacity(8192),
        }
    }

    /// Advance the APU by one T-cycle. `div_counter` is the timer's
    /// internal 16-bit counter at this T-cycle; the frame sequencer
    /// uses bit 12 for its own clocking.
    pub fn tick(&mut self, div_counter: u16) {
        if self.enabled {
            let div_bit = (div_counter & 0x1000) != 0;
            if self.prev_div_bit && !div_bit {
                self.step_frame_sequencer();
            }
            self.prev_div_bit = div_bit;

            self.ch1.tick();
            self.ch2.tick();
            self.ch3.tick(&self.wave_ram);
            self.ch4.tick();
        }

        // Sample-rate conversion. SAMPLE_RATE_HZ counts per master
        // tick; emit one stereo pair per wraparound.
        self.sample_counter += SAMPLE_RATE_HZ;
        if self.sample_counter >= MASTER_HZ {
            self.sample_counter -= MASTER_HZ;
            self.emit_sample();
        }
    }

    /// Convenience: tick four T-cycles given the timer's current
    /// counter value. The machine should advance the timer one
    /// T-cycle at a time and pass the post-tick value here, so call
    /// this with care — `tick` per T-cycle is preferred when the
    /// timer is being driven in step.
    pub fn tick_m(&mut self, div_counter_per_t: [u16; 4]) {
        for &div in &div_counter_per_t {
            self.tick(div);
        }
    }

    /// Drains accumulated stereo-interleaved samples into `dest`,
    /// returning the number of `f32` written. Samples are removed
    /// from the internal ring as they're read.
    pub fn drain_samples(&mut self, dest: &mut [f32]) -> usize {
        let mut written = 0;
        while written < dest.len()
            && let Some(value) = self.samples.pop_front()
        {
            dest[written] = value;
            written += 1;
        }
        written
    }

    /// Returns the number of `f32` samples currently buffered (one
    /// stereo pair = 2 floats).
    #[must_use]
    pub fn samples_buffered(&self) -> usize {
        self.samples.len()
    }

    /// Reads an APU register or wave-RAM byte.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => self.ch1.read_sweep(),
            0xFF11 => self.ch1.read_duty_length(),
            0xFF12 => self.ch1.read_envelope(),
            0xFF14 => self.ch1.read_freq_hi(),
            0xFF16 => self.ch2.read_duty_length(),
            0xFF17 => self.ch2.read_envelope(),
            0xFF19 => self.ch2.read_freq_hi(),
            0xFF1A => self.ch3.read_dac_enable(),
            0xFF1C => self.ch3.read_volume(),
            0xFF1E => self.ch3.read_freq_hi(),
            0xFF21 => self.ch4.read_envelope(),
            0xFF22 => self.ch4.read_poly(),
            0xFF23 => self.ch4.read_length_enable(),
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => {
                // Bits 4-6 always high; bit 7 = APU enable; bits 0-3
                // mirror per-channel enable.
                let mut v: u8 = 0x70;
                if self.enabled {
                    v |= 0x80;
                }
                if self.ch1.enabled {
                    v |= 0x01;
                }
                if self.ch2.enabled {
                    v |= 0x02;
                }
                if self.ch3.enabled {
                    v |= 0x04;
                }
                if self.ch4.enabled {
                    v |= 0x08;
                }
                v
            }
            WAVE_RAM_START..=WAVE_RAM_END => self.read_wave_ram(addr),
            _ => 0xFF,
        }
    }

    fn read_wave_ram(&self, addr: u16) -> u8 {
        if self.ch3.enabled {
            if self.ch3.wave_just_read {
                self.wave_ram[usize::from(self.ch3.sample_position / 2)]
            } else {
                0xFF
            }
        } else {
            self.wave_ram[usize::from(addr - WAVE_RAM_START)]
        }
    }

    /// Writes an APU register or wave-RAM byte.
    pub fn write(&mut self, addr: u16, value: u8) {
        // When the APU is disabled, most $FF10..$FF25 writes are
        // ignored. On DMG, length registers (NR11/21/31/41) are
        // still writable, NR52 is always writable, and wave RAM is
        // always writable.
        if !self.enabled && (0xFF10..=0xFF25).contains(&addr) {
            match addr {
                0xFF11 => self.ch1.write_length_only(value),
                0xFF16 => self.ch2.write_length_only(value),
                0xFF1B => self.ch3.write_length(value),
                0xFF20 => self.ch4.write_length(value),
                _ => {}
            }
            return;
        }

        match addr {
            0xFF10 => self.ch1.write_sweep(value),
            0xFF11 => self.ch1.write_duty_length(value),
            0xFF12 => self.ch1.write_envelope(value),
            0xFF13 => self.ch1.write_freq_lo(value),
            0xFF14 => self.ch1.write_freq_hi(value, self.first_half()),
            0xFF16 => self.ch2.write_duty_length(value),
            0xFF17 => self.ch2.write_envelope(value),
            0xFF18 => self.ch2.write_freq_lo(value),
            0xFF19 => self.ch2.write_freq_hi(value, self.first_half()),
            0xFF1A => self.ch3.write_dac_enable(value),
            0xFF1B => self.ch3.write_length(value),
            0xFF1C => self.ch3.write_volume(value),
            0xFF1D => self.ch3.write_freq_lo(value),
            0xFF1E => {
                let first_half = self.first_half();
                self.ch3
                    .write_freq_hi(value, first_half, &mut self.wave_ram);
            }
            0xFF20 => self.ch4.write_length(value),
            0xFF21 => self.ch4.write_envelope(value),
            0xFF22 => self.ch4.write_poly(value),
            0xFF23 => self.ch4.write_length_enable(value, self.first_half()),
            0xFF24 => self.nr50 = value,
            0xFF25 => self.nr51 = value,
            0xFF26 => self.write_nr52(value),
            WAVE_RAM_START..=WAVE_RAM_END => self.write_wave_ram(addr, value),
            _ => {}
        }
    }

    fn write_nr52(&mut self, value: u8) {
        let was_enabled = self.enabled;
        self.enabled = (value & 0x80) != 0;
        if !was_enabled && self.enabled {
            // Enabling resets the frame sequencer.
            self.frame_step = 0;
        }
        if was_enabled && !self.enabled {
            // Disabling clears all registers EXCEPT length counters
            // (DMG behaviour: lengths persist across power-off).
            self.ch1.reset_preserve_length();
            self.ch2.reset_preserve_length();
            self.ch3.reset_preserve_length();
            self.ch4.reset_preserve_length();
            self.nr50 = 0;
            self.nr51 = 0;
            self.frame_step = 0;
        }
    }

    fn write_wave_ram(&mut self, addr: u16, value: u8) {
        if self.ch3.enabled {
            if self.ch3.wave_just_read {
                self.wave_ram[usize::from(self.ch3.sample_position / 2)] = value;
            }
        } else {
            self.wave_ram[usize::from(addr - WAVE_RAM_START)] = value;
        }
    }

    /// True if the next frame-sequencer step would NOT clock the
    /// length counters — i.e. we're currently in the first half of
    /// a length period. The "length-enable in first half" quirks
    /// (in each channel's `write_freq_hi` / `write_length_enable`)
    /// gate on this.
    fn first_half(&self) -> bool {
        // Length clocks on even steps (0, 2, 4, 6). If frame_step is
        // odd, the next step is non-length → first half.
        (self.frame_step & 1) == 1
    }

    fn step_frame_sequencer(&mut self) {
        match self.frame_step {
            0 | 4 => {
                self.ch1.step_length();
                self.ch2.step_length();
                self.ch3.step_length();
                self.ch4.step_length();
            }
            2 | 6 => {
                self.ch1.step_length();
                self.ch2.step_length();
                self.ch3.step_length();
                self.ch4.step_length();
                self.ch1.step_sweep();
            }
            7 => {
                self.ch1.step_envelope();
                self.ch2.step_envelope();
                self.ch4.step_envelope();
            }
            _ => {}
        }
        self.frame_step = (self.frame_step + 1) & 0x07;
    }

    fn emit_sample(&mut self) {
        let s1 = self.ch1.sample();
        let s2 = self.ch2.sample();
        let s3 = self.ch3.sample();
        let s4 = self.ch4.sample();

        let mut left = 0.0f32;
        let mut right = 0.0f32;
        if (self.nr51 & 0x01) != 0 {
            right += s1;
        }
        if (self.nr51 & 0x02) != 0 {
            right += s2;
        }
        if (self.nr51 & 0x04) != 0 {
            right += s3;
        }
        if (self.nr51 & 0x08) != 0 {
            right += s4;
        }
        if (self.nr51 & 0x10) != 0 {
            left += s1;
        }
        if (self.nr51 & 0x20) != 0 {
            left += s2;
        }
        if (self.nr51 & 0x40) != 0 {
            left += s3;
        }
        if (self.nr51 & 0x80) != 0 {
            left += s4;
        }

        let left_vol = f32::from((self.nr50 >> 4) & 0x07);
        let right_vol = f32::from(self.nr50 & 0x07);

        // Each channel ranges -1..+1; sum of 4 is -4..+4. Scale by
        // master volume (0..7) and divide by 32 to leave headroom.
        left = left * (left_vol + 1.0) / 32.0;
        right = right * (right_vol + 1.0) / 32.0;

        // Cap the buffer so a runaway emitter doesn't grow unbounded
        // — drop the oldest pair when full. Keeps headroom for the
        // machine to drain at frame boundaries.
        if self.samples.len() + 2 > self.samples.capacity().max(8192) {
            self.samples.pop_front();
            self.samples.pop_front();
        }
        self.samples.push_back(left);
        self.samples.push_back(right);
    }
}
