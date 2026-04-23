//! Noise channel — CH4. 15- or 7-bit LFSR clocked from a divisor /
//! shift period. Output is the inverted bit 0 of the LFSR.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Noise {
    pub enabled: bool,
    pub dac_enabled: bool,
    pub length_timer: u8,
    pub length_enable: bool,

    pub envelope_initial: u8,
    pub envelope_add: bool,
    pub envelope_period: u8,
    envelope_timer: u8,
    current_volume: u8,

    pub clock_shift: u8,
    pub width_mode: bool, // false = 15-bit, true = 7-bit
    pub divisor_code: u8,
    lfsr: u16,

    period_timer: u32,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

impl Noise {
    pub(crate) const fn new() -> Self {
        Self {
            enabled: false,
            dac_enabled: false,
            length_timer: 0,
            length_enable: false,
            envelope_initial: 0,
            envelope_add: false,
            envelope_period: 0,
            envelope_timer: 0,
            current_volume: 0,
            clock_shift: 0,
            width_mode: false,
            divisor_code: 0,
            lfsr: 0x7FFF,
            period_timer: 0,
        }
    }

    pub(crate) fn reset_preserve_length(&mut self) {
        let length = self.length_timer;
        *self = Self::new();
        self.length_timer = length;
    }

    pub(crate) fn tick(&mut self) {
        if self.period_timer == 0 {
            self.reload_period_timer();

            // XOR bits 0 and 1, shift right, put result in bit 14.
            let b0 = self.lfsr & 1;
            let b1 = (self.lfsr >> 1) & 1;
            let new_bit = b0 ^ b1;
            self.lfsr >>= 1;
            self.lfsr |= new_bit << 14;
            if self.width_mode {
                self.lfsr &= !0x40;
                self.lfsr |= new_bit << 6;
            }
        } else {
            self.period_timer -= 1;
        }
    }

    fn reload_period_timer(&mut self) {
        let divisor: u32 = match self.divisor_code & 0x07 {
            0 => 4,
            1 => 8,
            2 => 16,
            3 => 24,
            4 => 32,
            5 => 40,
            6 => 48,
            _ => 56,
        };
        self.period_timer = divisor << self.clock_shift;
    }

    pub(crate) fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_enabled {
            return 0.0;
        }
        let amp = f32::from(self.current_volume) / 15.0;
        if (self.lfsr & 1) == 0 { amp } else { -amp }
    }

    pub(crate) fn step_length(&mut self) {
        if self.length_enable && self.length_timer > 0 {
            self.length_timer -= 1;
            if self.length_timer == 0 {
                self.enabled = false;
            }
        }
    }

    pub(crate) fn step_envelope(&mut self) {
        if self.envelope_period == 0 {
            return;
        }
        if self.envelope_timer > 0 {
            self.envelope_timer -= 1;
        }
        if self.envelope_timer == 0 {
            self.envelope_timer = self.envelope_period;
            if self.envelope_add && self.current_volume < 15 {
                self.current_volume += 1;
            } else if !self.envelope_add && self.current_volume > 0 {
                self.current_volume -= 1;
            }
        }
    }

    fn trigger(&mut self) {
        self.enabled = self.dac_enabled;
        if self.length_timer == 0 {
            self.length_timer = 64;
        }
        self.reload_period_timer();
        self.envelope_timer = self.envelope_period;
        self.current_volume = self.envelope_initial;
        self.lfsr = 0x7FFF;
    }

    // -- Register access ------------------------------------------------

    pub(crate) fn write_length(&mut self, value: u8) {
        self.length_timer = 64 - (value & 0x3F);
    }

    pub(crate) fn read_envelope(&self) -> u8 {
        ((self.envelope_initial & 0x0F) << 4)
            | (if self.envelope_add { 0x08 } else { 0 })
            | (self.envelope_period & 0x07)
    }

    pub(crate) fn write_envelope(&mut self, value: u8) {
        self.envelope_initial = (value >> 4) & 0x0F;
        self.envelope_add = (value & 0x08) != 0;
        self.envelope_period = value & 0x07;
        self.dac_enabled = (value & 0xF8) != 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }

    pub(crate) fn read_poly(&self) -> u8 {
        ((self.clock_shift & 0x0F) << 4)
            | (if self.width_mode { 0x08 } else { 0 })
            | (self.divisor_code & 0x07)
    }

    pub(crate) fn write_poly(&mut self, value: u8) {
        self.clock_shift = (value >> 4) & 0x0F;
        self.width_mode = (value & 0x08) != 0;
        self.divisor_code = value & 0x07;
    }

    pub(crate) fn read_length_enable(&self) -> u8 {
        0xBF | (if self.length_enable { 0x40 } else { 0 })
    }

    pub(crate) fn write_length_enable(&mut self, value: u8, first_half: bool) {
        let was_enable = self.length_enable;
        let new_enable = (value & 0x40) != 0;

        if !was_enable && new_enable && first_half && self.length_timer > 0 {
            self.length_timer -= 1;
            if self.length_timer == 0 && (value & 0x80) == 0 {
                self.enabled = false;
            }
        }
        self.length_enable = new_enable;

        if (value & 0x80) != 0 {
            let was_zero = self.length_timer == 0;
            self.trigger();
            if was_zero && self.length_enable && first_half {
                self.length_timer -= 1;
            }
        }
    }
}
