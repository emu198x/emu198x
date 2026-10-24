//! The Famicom Disk System's sound: one 64-step, 6-bit wavetable voice
//! whose pitch a second wavetable, the modulator, bends.
//!
//! Both units run every CPU cycle. The voice adds its 12-bit frequency,
//! plus the modulator's pitch offset, to a 16-bit accumulator and steps
//! through the wavetable each time it overflows. The modulator does the
//! same with its own frequency and steps a 32-entry table of 3-bit
//! deltas (each written twice, making 64), which move a 7-bit signed
//! counter; the counter times the modulator's gain, rounded the way the
//! chip rounds it, scales the voice's frequency into the offset.
//!
//! Each unit has a volume envelope ticked every `8 × (speed + 1) ×
//! master speed` cycles, raising or lowering its gain by one up to 32.
//! The output is the wavetable sample times the volume gain (capped at
//! 32) times the master volume's weight, scaled to 0–63.
//!
//! The real unit low-passes its output near 2 kHz; that filter is not
//! modelled. Facts from the NESdev wiki "FDS audio" page and Mesen2's
//! `FdsAudio.cpp`, `ModChannel.h` and `BaseFdsChannel.h` (GPL-3, read
//! for behaviour only).

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Weights for master volumes 2/2, 2/3, 2/4 and 2/5, over 36.
const MASTER_VOLUME: [u32; 4] = [36, 24, 17, 14];

/// The modulator's counter steps, by table entry; 4 resets it to 0.
const MOD_STEPS: [i32; 8] = [0, 1, 2, 4, 0, -4, -2, -1];

/// Bits 6 and 7 of a sound-register read, which the chip leaves to the
/// bus. After an absolute read of `$40xx` the bus holds `$40`.
const OPEN_BUS: u8 = 0x40;

/// An envelope: the volume unit's or the modulator's.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Envelope {
    speed: u8,
    gain: u8,
    off: bool,
    increase: bool,
    frequency: u16,
    timer: u32,
}

impl Envelope {
    /// `$4080` / `$4084`.
    fn write_control(&mut self, value: u8, master_speed: u8) {
        self.speed = value & 0x3F;
        self.increase = value & 0x40 != 0;
        self.off = value & 0x80 != 0;
        self.reset_timer(master_speed);
        if self.off {
            self.gain = self.speed;
        }
    }

    fn write_frequency_low(&mut self, value: u8) {
        self.frequency = (self.frequency & 0x0F00) | u16::from(value);
    }

    fn write_frequency_high(&mut self, value: u8) {
        self.frequency = (self.frequency & 0x00FF) | (u16::from(value & 0x0F) << 8);
    }

    fn reset_timer(&mut self, master_speed: u8) {
        self.timer = 8 * (u32::from(self.speed) + 1) * u32::from(master_speed);
    }

    /// One cycle; true when the envelope stepped.
    fn tick(&mut self, master_speed: u8) -> bool {
        if self.off || master_speed == 0 {
            return false;
        }
        self.timer = self.timer.saturating_sub(1);
        if self.timer > 0 {
            return false;
        }
        self.reset_timer(master_speed);
        if self.increase && self.gain < 32 {
            self.gain += 1;
        } else if !self.increase && self.gain > 0 {
            self.gain -= 1;
        }
        true
    }
}

/// The FDS sound unit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FdsAudio {
    #[serde(with = "BigArray")]
    wave: [u8; 64],
    wave_write: bool,
    volume: Envelope,
    modulator: Envelope,
    #[serde(with = "BigArray")]
    mod_table: [u8; 64],
    mod_position: u8,
    mod_counter: i8,
    mod_disabled: bool,
    mod_accumulator: u16,
    mod_output: i32,
    envelopes_halted: bool,
    wave_halted: bool,
    master_volume: u8,
    master_speed: u8,
    wave_accumulator: u16,
    wave_position: u8,
    output: u8,
}

impl Default for FdsAudio {
    fn default() -> Self {
        Self {
            wave: [0; 64],
            wave_write: false,
            volume: Envelope::default(),
            modulator: Envelope::default(),
            mod_table: [0; 64],
            mod_position: 0,
            mod_counter: 0,
            mod_disabled: false,
            mod_accumulator: 0,
            mod_output: 0,
            envelopes_halted: false,
            wave_halted: false,
            master_volume: 0,
            // The BIOS sets $408A to $E8; few programs change it.
            master_speed: 0xE8,
            wave_accumulator: 0,
            wave_position: 0,
            output: 0,
        }
    }
}

impl FdsAudio {
    /// The output level, 0 to 63.
    #[must_use]
    pub fn output(&self) -> u8 {
        self.output
    }

    /// A read of `$4040–$4097`, or `None` for an address that drives
    /// nothing.
    #[must_use]
    pub fn read(&self, addr: u16) -> Option<u8> {
        match addr {
            // With writing disabled, the whole window reads the sample
            // under the play position.
            0x4040..=0x407F if self.wave_write => {
                Some(OPEN_BUS | self.wave[usize::from(addr & 0x3F)])
            }
            0x4040..=0x407F => Some(OPEN_BUS | self.wave[usize::from(self.wave_position)]),
            0x4090 => Some(OPEN_BUS | self.volume.gain),
            0x4092 => Some(OPEN_BUS | self.modulator.gain),
            _ => None,
        }
    }

    /// A write to `$4040–$408A`.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x4040..=0x407F => {
                if self.wave_write {
                    self.wave[usize::from(addr & 0x3F)] = value & 0x3F;
                }
            }
            0x4080 => {
                self.volume.write_control(value, self.master_speed);
                self.update_mod_output();
            }
            0x4082 => {
                self.volume.write_frequency_low(value);
                self.update_mod_output();
            }
            0x4083 => {
                self.envelopes_halted = value & 0x40 != 0;
                self.wave_halted = value & 0x80 != 0;
                if self.wave_halted {
                    self.wave_position = 0;
                }
                if self.envelopes_halted {
                    self.volume.reset_timer(self.master_speed);
                    self.modulator.reset_timer(self.master_speed);
                }
                self.volume.write_frequency_high(value);
                self.update_mod_output();
            }
            0x4084 => {
                self.modulator.write_control(value, self.master_speed);
                self.update_mod_output();
            }
            0x4085 => {
                self.set_mod_counter(i32::from(value & 0x7F));
                self.update_mod_output();
            }
            0x4086 => self.modulator.write_frequency_low(value),
            0x4087 => {
                self.modulator.write_frequency_high(value);
                self.mod_disabled = value & 0x80 != 0;
                if self.mod_disabled {
                    self.mod_accumulator = 0;
                }
            }
            0x4088 => {
                // The table only takes writes while the modulator is off;
                // each entry fills two steps.
                if self.mod_disabled {
                    let entry = value & 0x07;
                    self.mod_table[usize::from(self.mod_position & 0x3F)] = entry;
                    self.mod_table[usize::from((self.mod_position + 1) & 0x3F)] = entry;
                    self.mod_position = (self.mod_position + 2) & 0x3F;
                }
            }
            0x4089 => {
                self.master_volume = value & 0x03;
                self.wave_write = value & 0x80 != 0;
            }
            0x408A => self.master_speed = value,
            _ => {}
        }
    }

    /// Wrap a 7-bit counter value into −64..=63.
    fn set_mod_counter(&mut self, value: i32) {
        let wrapped = if value >= 64 {
            value - 128
        } else if value < -64 {
            value + 128
        } else {
            value
        };
        self.mod_counter = i8::try_from(wrapped).unwrap_or(0);
    }

    /// The pitch offset: counter × gain, with the chip's rounding, scaled
    /// by the voice's frequency.
    fn update_mod_output(&mut self) {
        let counter = i32::from(self.mod_counter);
        let mut temp = counter * i32::from(self.modulator.gain);
        let remainder = temp & 0x0F;
        temp >>= 4;
        if remainder > 0 && temp & 0x80 == 0 {
            temp += if counter < 0 { -1 } else { 2 };
        }
        if temp >= 192 {
            temp -= 256;
        } else if temp < -64 {
            temp += 256;
        }
        temp *= i32::from(self.volume.frequency);
        let remainder = temp & 0x3F;
        temp >>= 6;
        if remainder >= 32 {
            temp += 1;
        }
        self.mod_output = temp;
    }

    fn tick_modulator(&mut self) -> bool {
        let frequency = self.modulator.frequency;
        if self.mod_disabled || frequency == 0 {
            return false;
        }
        let (sum, overflow) = self.mod_accumulator.overflowing_add(frequency);
        self.mod_accumulator = sum;
        if !overflow {
            return false;
        }
        let entry = self.mod_table[usize::from(self.mod_position)];
        if entry == 4 {
            self.set_mod_counter(0);
        } else {
            self.set_mod_counter(i32::from(self.mod_counter) + MOD_STEPS[usize::from(entry)]);
        }
        self.mod_position = (self.mod_position + 1) & 0x3F;
        true
    }

    /// One CPU cycle.
    pub fn tick(&mut self) {
        if !self.wave_halted && !self.envelopes_halted {
            self.volume.tick(self.master_speed);
            if self.modulator.tick(self.master_speed) {
                self.update_mod_output();
            }
        }
        if self.tick_modulator() {
            self.update_mod_output();
        }
        if !self.wave_write {
            let level = u32::from(self.volume.gain.min(32))
                * MASTER_VOLUME[usize::from(self.master_volume)];
            let sample = u32::from(self.wave[usize::from(self.wave_position)]) * level / 1152;
            self.output = u8::try_from(sample).unwrap_or(63);
        }
        let step = i32::from(self.volume.frequency) + self.mod_output;
        if !self.wave_halted && step > 0 {
            let step = u16::try_from(step).unwrap_or(u16::MAX);
            let (sum, overflow) = self.wave_accumulator.overflowing_add(step);
            self.wave_accumulator = sum;
            if overflow {
                self.wave_position = (self.wave_position + 1) & 0x3F;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square wave at full volume, envelopes off.
    fn playing(frequency: u16) -> FdsAudio {
        let mut audio = FdsAudio::default();
        audio.write(0x4089, 0x80);
        for i in 0..64 {
            audio.write(0x4040 + i, if i < 32 { 63 } else { 0 });
        }
        audio.write(0x4089, 0x00);
        audio.write(0x4080, 0x80 | 32); // envelope off, gain 32
        audio.write(0x4087, 0x80); // modulator off
        audio.write(0x4082, (frequency & 0xFF) as u8);
        audio.write(0x4083, (frequency >> 8) as u8);
        audio
    }

    fn rising_edges(audio: &mut FdsAudio, cycles: u32) -> u32 {
        let mut edges = 0;
        let mut last = audio.output();
        for _ in 0..cycles {
            audio.tick();
            if audio.output() > last {
                edges += 1;
            }
            last = audio.output();
        }
        edges
    }

    #[test]
    fn the_wave_steps_once_per_accumulator_overflow() {
        // Frequency F steps the wave F / 65536 times a cycle; 64 steps
        // make a period, so F = 1024 gives one period per 4096 cycles.
        let mut audio = playing(1024);
        let edges = rising_edges(&mut audio, 4096 * 10);
        assert!((9..=11).contains(&edges), "{edges}");
        assert_eq!(audio.read(0x4090), Some(OPEN_BUS | 32));
    }

    #[test]
    fn full_volume_reaches_63_and_master_volume_scales_it() {
        let mut audio = playing(1024);
        let mut peak = 0;
        for _ in 0..4096 {
            audio.tick();
            peak = peak.max(audio.output());
        }
        assert_eq!(peak, 63);
        audio.write(0x4089, 0x03); // 2/5
        let mut peak = 0;
        for _ in 0..4096 {
            audio.tick();
            peak = peak.max(audio.output());
        }
        assert_eq!(peak, 24, "63 x 14 / 36");
    }

    #[test]
    fn the_modulator_bends_the_pitch() {
        let mut steady = playing(1024);
        let base = rising_edges(&mut steady, 4096 * 20);
        let mut bent = playing(1024);
        // Fill the table with +4 steps, so the counter climbs.
        for _ in 0..32 {
            bent.write(0x4088, 3);
        }
        bent.write(0x4084, 0x80 | 32); // modulator gain 32
        bent.write(0x4086, 0xFF);
        bent.write(0x4087, 0x0F); // enable, high frequency
        let raised = rising_edges(&mut bent, 4096 * 20);
        assert!(raised > base, "{raised} vs {base}");
    }

    #[test]
    fn mod_output_follows_the_documented_rounding() {
        let mut audio = FdsAudio::default();
        audio.write(0x4082, 0x00);
        audio.write(0x4083, 0x01); // frequency 256
        audio.write(0x4084, 0x80 | 3); // gain 3
        audio.write(0x4085, 5); // counter 5
        // 5 x 3 = 15: remainder 15, 15 >> 4 = 0, +2 = 2; 2 x 256 = 512 >> 6 = 8.
        assert_eq!(audio.mod_output, 8);
        audio.write(0x4085, 0x7F); // counter -1
        // -3: remainder 13, -3 >> 4 = -1, high bit set so no rounding;
        // -1 x 256 = -256 >> 6 = -4.
        assert_eq!(audio.mod_output, -4);
    }

    #[test]
    fn the_envelope_ramps_the_gain() {
        let mut audio = FdsAudio::default();
        audio.write(0x408A, 1); // master speed 1
        audio.write(0x4080, 0x40); // increase, speed 0: every 8 cycles
        for _ in 0..80 {
            audio.tick();
        }
        assert_eq!(audio.volume.gain, 10);
        audio.write(0x4083, 0x40); // halt the envelopes
        for _ in 0..80 {
            audio.tick();
        }
        assert_eq!(audio.volume.gain, 10);
    }
}
