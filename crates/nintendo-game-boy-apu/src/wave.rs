//! Wave channel — CH3. 32-step playback over the 16-byte wave RAM
//! at `$FF30..$FF3F` (4-bit samples, two per byte).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Wave {
    pub enabled: bool,
    pub dac_enabled: bool,
    pub length_timer: u16, // up to 256
    pub length_enable: bool,
    pub volume_code: u8, // 0=mute, 1=100%, 2=50%, 3=25%
    pub frequency: u16,  // 11-bit
    period_timer: u16,
    pub sample_position: u8, // 0..=31 (32 nibbles)
    current_sample: u8,
    /// Set for one Apu tick after the period timer reloads — used by
    /// the wave-RAM-read-during-active quirk.
    pub wave_just_read: bool,
}

impl Wave {
    pub(crate) const fn new() -> Self {
        Self {
            enabled: false,
            dac_enabled: false,
            length_timer: 0,
            length_enable: false,
            volume_code: 0,
            frequency: 0,
            period_timer: 0,
            sample_position: 0,
            current_sample: 0,
            wave_just_read: false,
        }
    }

    pub(crate) fn reset_preserve_length(&mut self) {
        let length = self.length_timer;
        *self = Self::new();
        self.length_timer = length;
    }

    pub(crate) fn tick(&mut self, wave_ram: &[u8; 16]) {
        self.wave_just_read = false;

        if self.period_timer == 0 {
            self.period_timer = 2047 - self.frequency;
            self.sample_position = (self.sample_position + 1) & 0x1F;
            let byte = wave_ram[usize::from(self.sample_position / 2)];
            self.current_sample = if (self.sample_position & 1) == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
            self.wave_just_read = true;
        } else {
            self.period_timer -= 1;
        }
    }

    pub(crate) fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_enabled || self.volume_code == 0 {
            return 0.0;
        }
        let shift: u8 = match self.volume_code {
            1 => 0, // 100%
            2 => 1, // 50%
            _ => 2, // 25%
        };
        let shifted = self.current_sample >> shift;
        f32::from(shifted) / 7.5 - 1.0
    }

    pub(crate) fn step_length(&mut self) {
        if self.length_enable && self.length_timer > 0 {
            self.length_timer -= 1;
            if self.length_timer == 0 {
                self.enabled = false;
            }
        }
    }

    fn trigger(&mut self, wave_ram: &mut [u8; 16]) {
        // DMG wave-RAM corruption: re-triggering while active with the
        // frequency timer about to clock corrupts wave RAM.
        if self.enabled && self.period_timer == 0 {
            let offset = ((self.sample_position.wrapping_add(1)) / 2) & 0x0F;
            if offset < 4 {
                wave_ram[0] = wave_ram[usize::from(offset)];
            } else {
                let src = (offset & 0xFC) as usize;
                wave_ram[0] = wave_ram[src];
                wave_ram[1] = wave_ram[src + 1];
                wave_ram[2] = wave_ram[src + 2];
                wave_ram[3] = wave_ram[src + 3];
            }
        }

        self.enabled = self.dac_enabled;
        if self.length_timer == 0 {
            self.length_timer = 256;
        }
        self.period_timer = 2047 - self.frequency;
        self.sample_position = 0;
    }

    // -- Register access ------------------------------------------------

    pub(crate) fn read_dac_enable(&self) -> u8 {
        0x7F | (if self.dac_enabled { 0x80 } else { 0 })
    }

    pub(crate) fn write_dac_enable(&mut self, value: u8) {
        self.dac_enabled = (value & 0x80) != 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }

    pub(crate) fn write_length(&mut self, value: u8) {
        self.length_timer = 256 - u16::from(value);
    }

    pub(crate) fn read_volume(&self) -> u8 {
        0x9F | ((self.volume_code & 0b11) << 5)
    }

    pub(crate) fn write_volume(&mut self, value: u8) {
        self.volume_code = (value >> 5) & 0b11;
    }

    pub(crate) fn write_freq_lo(&mut self, value: u8) {
        self.frequency = (self.frequency & 0x0700) | u16::from(value);
    }

    pub(crate) fn read_freq_hi(&self) -> u8 {
        0xBF | (if self.length_enable { 0x40 } else { 0 })
    }

    pub(crate) fn write_freq_hi(&mut self, value: u8, first_half: bool, wave_ram: &mut [u8; 16]) {
        let was_enable = self.length_enable;
        let new_enable = (value & 0x40) != 0;

        self.frequency = (self.frequency & 0x00FF) | (u16::from(value & 0x07) << 8);

        if !was_enable && new_enable && first_half && self.length_timer > 0 {
            self.length_timer -= 1;
            if self.length_timer == 0 && (value & 0x80) == 0 {
                self.enabled = false;
            }
        }
        self.length_enable = new_enable;

        if (value & 0x80) != 0 {
            let was_zero = self.length_timer == 0;
            self.trigger(wave_ram);
            if was_zero && self.length_enable && first_half {
                self.length_timer -= 1;
            }
        }
    }
}
