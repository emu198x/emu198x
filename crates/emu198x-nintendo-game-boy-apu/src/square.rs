//! Square-wave channel — used for both CH1 and CH2. CH1 also owns
//! the frequency-sweep unit; CH2 doesn't (its `has_sweep` flag is
//! `false` and `step_sweep` short-circuits).

use serde::{Deserialize, Serialize};

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 0, 1, 1, 1], // 50%
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Square {
    pub enabled: bool,
    pub has_sweep: bool,

    pub duty: u8,
    pub length_timer: u8,
    pub length_enable: bool,

    pub envelope_initial: u8,
    pub envelope_add: bool,
    pub envelope_period: u8,
    envelope_timer: u8,

    pub frequency: u16,
    period_timer: u16,
    duty_position: u8,
    current_volume: u8,

    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_timer: u8,
    sweep_enabled: bool,
    shadow_frequency: u16,
    /// Set when a negate calculation has been performed since the
    /// last trigger; clearing the negate bit afterwards disables
    /// the channel (DMG-only quirk).
    sweep_negate_used: bool,

    pub dac_enabled: bool,
}

impl Square {
    pub(crate) const fn new(has_sweep: bool) -> Self {
        Self {
            enabled: false,
            has_sweep,
            duty: 0,
            length_timer: 0,
            length_enable: false,
            envelope_initial: 0,
            envelope_add: false,
            envelope_period: 0,
            envelope_timer: 0,
            frequency: 0,
            period_timer: 0,
            duty_position: 0,
            current_volume: 0,
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            sweep_timer: 0,
            sweep_enabled: false,
            shadow_frequency: 0,
            sweep_negate_used: false,
            dac_enabled: false,
        }
    }

    pub(crate) const fn new_post_bootrom_ch1(sweep: u8, envelope: u8, enabled: bool) -> Self {
        Self {
            enabled,
            has_sweep: true,
            duty: 0b10,
            length_timer: 0,
            length_enable: false,
            envelope_initial: (envelope >> 4) & 0x0F,
            envelope_add: (envelope & 0x08) != 0,
            envelope_period: envelope & 0x07,
            envelope_timer: 0,
            frequency: 0,
            period_timer: 0,
            duty_position: 0,
            current_volume: 0,
            sweep_period: (sweep >> 4) & 0x07,
            sweep_negate: (sweep & 0x08) != 0,
            sweep_shift: sweep & 0x07,
            sweep_timer: 0,
            sweep_enabled: false,
            shadow_frequency: 0,
            sweep_negate_used: false,
            dac_enabled: (envelope & 0xF8) != 0,
        }
    }

    /// Reset to power-on state but preserve the length counter (DMG
    /// behaviour when NR52 disables the APU).
    pub(crate) fn reset_preserve_length(&mut self) {
        let length = self.length_timer;
        let has_sweep = self.has_sweep;
        *self = Self::new(has_sweep);
        self.length_timer = length;
    }

    pub(crate) fn tick(&mut self) {
        if self.period_timer == 0 {
            self.period_timer = (2048 - self.frequency) * 2;
            self.duty_position = (self.duty_position + 1) & 0x07;
        } else {
            self.period_timer -= 1;
        }
    }

    pub(crate) fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_enabled {
            return 0.0;
        }
        let amp = f32::from(self.current_volume) / 15.0;
        if DUTY_TABLE[usize::from(self.duty & 0b11)][usize::from(self.duty_position)] == 1 {
            amp
        } else {
            -amp
        }
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

    pub(crate) fn step_sweep(&mut self) {
        if !self.has_sweep {
            return;
        }
        if self.sweep_timer > 0 {
            self.sweep_timer -= 1;
        }
        if self.sweep_timer == 0 {
            self.sweep_timer = if self.sweep_period == 0 {
                8
            } else {
                self.sweep_period
            };
            if self.sweep_enabled && self.sweep_period > 0 {
                if self.sweep_negate {
                    self.sweep_negate_used = true;
                }
                let new_freq = self.calc_sweep_freq();
                if new_freq > 2047 {
                    self.enabled = false;
                    return;
                }
                if self.sweep_shift != 0 {
                    self.frequency = new_freq;
                    self.shadow_frequency = new_freq;
                    if self.sweep_negate {
                        self.sweep_negate_used = true;
                    }
                    if self.calc_sweep_freq() > 2047 {
                        self.enabled = false;
                    }
                }
            }
        }
    }

    fn calc_sweep_freq(&self) -> u16 {
        let delta = self.shadow_frequency >> self.sweep_shift;
        if self.sweep_negate {
            self.shadow_frequency.wrapping_sub(delta)
        } else {
            self.shadow_frequency + delta
        }
    }

    fn trigger(&mut self) {
        self.enabled = self.dac_enabled;
        if self.length_timer == 0 {
            self.length_timer = 64;
        }
        self.period_timer = (2048 - self.frequency) * 2;
        self.envelope_timer = self.envelope_period;
        self.current_volume = self.envelope_initial;

        if self.has_sweep {
            self.sweep_negate_used = false;
            self.shadow_frequency = self.frequency;
            self.sweep_timer = if self.sweep_period == 0 {
                8
            } else {
                self.sweep_period
            };
            self.sweep_enabled = self.sweep_period != 0 || self.sweep_shift != 0;
            if self.sweep_shift != 0 {
                if self.sweep_negate {
                    self.sweep_negate_used = true;
                }
                if self.calc_sweep_freq() > 2047 {
                    self.enabled = false;
                }
            }
        }
    }

    // -- Register access ------------------------------------------------

    pub(crate) fn read_sweep(&self) -> u8 {
        0x80 | ((self.sweep_period & 0x07) << 4)
            | (if self.sweep_negate { 0x08 } else { 0 })
            | (self.sweep_shift & 0x07)
    }

    pub(crate) fn write_sweep(&mut self, value: u8) {
        self.sweep_period = (value >> 4) & 0x07;
        let new_negate = (value & 0x08) != 0;
        // Clearing negate after a negate calculation disables the channel.
        if self.sweep_negate_used && !new_negate {
            self.enabled = false;
        }
        self.sweep_negate = new_negate;
        self.sweep_shift = value & 0x07;
    }

    pub(crate) fn read_duty_length(&self) -> u8 {
        ((self.duty & 0b11) << 6) | 0x3F
    }

    pub(crate) fn write_duty_length(&mut self, value: u8) {
        self.duty = (value >> 6) & 0b11;
        self.length_timer = 64 - (value & 0x3F);
    }

    pub(crate) fn write_length_only(&mut self, value: u8) {
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

    pub(crate) fn write_freq_lo(&mut self, value: u8) {
        self.frequency = (self.frequency & 0x0700) | u16::from(value);
    }

    pub(crate) fn read_freq_hi(&self) -> u8 {
        0xBF | (if self.length_enable { 0x40 } else { 0 })
    }

    pub(crate) fn write_freq_hi(&mut self, value: u8, first_half: bool) {
        let was_enable = self.length_enable;
        let new_enable = (value & 0x40) != 0;

        self.frequency = (self.frequency & 0x00FF) | (u16::from(value & 0x07) << 8);

        // Length-enable quirk: enabling length in the first half of a
        // length period clocks the length counter immediately.
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
