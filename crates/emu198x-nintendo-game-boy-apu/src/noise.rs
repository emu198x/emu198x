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

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_with_dac() -> Noise {
        let mut n = Noise::new();
        n.dac_enabled = true;
        n.enabled = true;
        n.current_volume = 8;
        n
    }

    // -- Construction -------------------------------------------------

    #[test]
    fn default_matches_new() {
        let a = Noise::default();
        let b = Noise::new();
        assert_eq!(a.enabled, b.enabled);
        assert_eq!(a.dac_enabled, b.dac_enabled);
        assert_eq!(a.length_timer, b.length_timer);
        assert_eq!(a.length_enable, b.length_enable);
        assert_eq!(a.envelope_initial, b.envelope_initial);
        assert_eq!(a.clock_shift, b.clock_shift);
        assert_eq!(a.divisor_code, b.divisor_code);
        assert_eq!(a.lfsr, 0x7FFF);
    }

    // -- LFSR / tick --------------------------------------------------

    #[test]
    fn tick_decrements_period_timer_when_nonzero() {
        let mut n = Noise::new();
        n.period_timer = 5;
        n.tick();
        assert_eq!(n.period_timer, 4);
    }

    #[test]
    fn tick_reloads_and_advances_lfsr_in_15bit_mode() {
        let mut n = Noise::new();
        n.divisor_code = 0; // divisor 4
        n.clock_shift = 0;
        n.width_mode = false;
        n.period_timer = 0;
        n.lfsr = 0x7FFF;

        // bits 0=1, bit 1=1, XOR=0; lfsr >> 1 = 0x3FFF, new bit 14 = 0.
        n.tick();
        assert_eq!(n.period_timer, 4);
        assert_eq!(n.lfsr, 0x3FFF, "shifted right with new bit 14 = 0");
        // 7-bit feedback should NOT have touched bit 6 — still 1 from
        // the right-shift carrying old bit 7.
        // (0x3FFF has bit 6 = 1; 7-bit mode would clear it via
        // !0x40 mask, but width_mode is false here.)
    }

    #[test]
    fn tick_writes_bit_6_in_7bit_mode() {
        let mut n = Noise::new();
        n.divisor_code = 0;
        n.clock_shift = 0;
        n.width_mode = true;
        n.period_timer = 0;
        // Bit 0 = 0, bit 1 = 1 → new_bit = 1, expect bit 6 = 1.
        n.lfsr = 0b10;
        n.tick();
        assert_eq!((n.lfsr >> 6) & 1, 1, "bit 6 set in 7-bit mode");
        assert_eq!((n.lfsr >> 14) & 1, 1, "bit 14 also set");
    }

    #[test]
    fn tick_clears_bit_6_in_7bit_mode_when_new_bit_zero() {
        let mut n = Noise::new();
        n.width_mode = true;
        n.divisor_code = 0;
        n.clock_shift = 0;
        n.period_timer = 0;
        // bits 0=1, 1=1 → new_bit=0; pre-load bit 6 = 1 to verify it's
        // forced low.
        n.lfsr = 0x40 | 0b11;
        n.tick();
        assert_eq!((n.lfsr >> 6) & 1, 0);
    }

    // -- reload_period_timer divisor table ---------------------------

    #[test]
    fn divisor_table_covers_every_code() {
        let cases = [
            (0u8, 4u32),
            (1, 8),
            (2, 16),
            (3, 24),
            (4, 32),
            (5, 40),
            (6, 48),
            (7, 56),
        ];
        for (code, expected) in cases {
            let mut n = Noise::new();
            n.divisor_code = code;
            n.clock_shift = 0;
            n.period_timer = 0;
            n.tick();
            assert_eq!(n.period_timer, expected, "divisor code {code}");
        }
    }

    #[test]
    fn clock_shift_left_shifts_divisor() {
        let mut n = Noise::new();
        n.divisor_code = 1; // base = 8
        n.clock_shift = 3; // 8 << 3 = 64
        n.period_timer = 0;
        n.tick();
        assert_eq!(n.period_timer, 64);
    }

    // -- sample() -----------------------------------------------------

    #[test]
    fn sample_zero_when_disabled() {
        let n = Noise::new();
        assert_eq!(n.sample(), 0.0);
    }

    #[test]
    fn sample_zero_when_dac_off() {
        let mut n = enabled_with_dac();
        n.dac_enabled = false;
        assert_eq!(n.sample(), 0.0);
    }

    #[test]
    fn sample_positive_when_lfsr_bit0_zero() {
        let mut n = enabled_with_dac();
        n.lfsr = 0xFFFE; // bit 0 == 0 → +amp
        n.current_volume = 15;
        let s = n.sample();
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sample_negative_when_lfsr_bit0_one() {
        let mut n = enabled_with_dac();
        n.lfsr = 0x0001; // bit 0 == 1 → -amp
        n.current_volume = 15;
        let s = n.sample();
        assert!((s + 1.0).abs() < f32::EPSILON);
    }

    // -- step_length --------------------------------------------------

    #[test]
    fn step_length_no_op_when_disabled() {
        let mut n = enabled_with_dac();
        n.length_enable = false;
        n.length_timer = 5;
        n.step_length();
        assert_eq!(n.length_timer, 5);
        assert!(n.enabled);
    }

    #[test]
    fn step_length_decrements_and_disables_at_zero() {
        let mut n = enabled_with_dac();
        n.length_enable = true;
        n.length_timer = 1;
        n.step_length();
        assert_eq!(n.length_timer, 0);
        assert!(!n.enabled);
    }

    #[test]
    fn step_length_does_not_underflow_at_zero() {
        let mut n = enabled_with_dac();
        n.length_enable = true;
        n.length_timer = 0;
        n.step_length();
        assert_eq!(n.length_timer, 0);
        assert!(n.enabled, "channel stays enabled when length is already 0");
    }

    // -- step_envelope -----------------------------------------------

    #[test]
    fn step_envelope_period_zero_is_no_op() {
        let mut n = enabled_with_dac();
        n.envelope_period = 0;
        n.envelope_timer = 0;
        n.current_volume = 5;
        n.step_envelope();
        assert_eq!(n.current_volume, 5);
    }

    #[test]
    fn step_envelope_decrements_timer() {
        let mut n = enabled_with_dac();
        n.envelope_period = 4;
        n.envelope_timer = 3;
        n.current_volume = 5;
        n.step_envelope();
        assert_eq!(n.envelope_timer, 2);
        assert_eq!(n.current_volume, 5);
    }

    #[test]
    fn step_envelope_increments_volume_when_add_set() {
        let mut n = enabled_with_dac();
        n.envelope_period = 2;
        n.envelope_timer = 1;
        n.envelope_add = true;
        n.current_volume = 5;
        n.step_envelope();
        assert_eq!(n.envelope_timer, 2);
        assert_eq!(n.current_volume, 6);
    }

    #[test]
    fn step_envelope_clamps_volume_at_15() {
        let mut n = enabled_with_dac();
        n.envelope_period = 1;
        n.envelope_timer = 0; // immediate clock
        n.envelope_add = true;
        n.current_volume = 15;
        n.step_envelope();
        assert_eq!(n.current_volume, 15);
    }

    #[test]
    fn step_envelope_decrements_volume_when_add_clear() {
        let mut n = enabled_with_dac();
        n.envelope_period = 1;
        n.envelope_timer = 0;
        n.envelope_add = false;
        n.current_volume = 5;
        n.step_envelope();
        assert_eq!(n.current_volume, 4);
    }

    #[test]
    fn step_envelope_clamps_volume_at_zero() {
        let mut n = enabled_with_dac();
        n.envelope_period = 1;
        n.envelope_timer = 0;
        n.envelope_add = false;
        n.current_volume = 0;
        n.step_envelope();
        assert_eq!(n.current_volume, 0);
    }

    // -- trigger() and write_length_enable -----------------------------

    #[test]
    fn trigger_sets_length_to_64_when_zero() {
        let mut n = Noise::new();
        n.dac_enabled = true;
        n.envelope_initial = 7;
        n.envelope_period = 3;
        n.length_timer = 0;
        n.write_length_enable(0x80, false); // trigger only
        assert!(n.enabled);
        assert_eq!(n.length_timer, 64);
        assert_eq!(n.current_volume, 7);
        assert_eq!(n.envelope_timer, 3);
        assert_eq!(n.lfsr, 0x7FFF);
    }

    #[test]
    fn trigger_with_dac_off_keeps_channel_disabled() {
        let mut n = Noise::new();
        n.dac_enabled = false;
        n.write_length_enable(0x80, false);
        assert!(!n.enabled);
    }

    #[test]
    fn trigger_preserves_nonzero_length() {
        let mut n = Noise::new();
        n.dac_enabled = true;
        n.length_timer = 30;
        n.write_length_enable(0x80, false);
        assert_eq!(n.length_timer, 30);
    }

    // -- Register I/O -------------------------------------------------

    #[test]
    fn write_length_decodes_64_minus_value() {
        let mut n = Noise::new();
        n.write_length(0x00);
        assert_eq!(n.length_timer, 64);
        n.write_length(0x3F); // mask only low 6 bits
        assert_eq!(n.length_timer, 1);
        n.write_length(0xC0); // top bits ignored
        assert_eq!(n.length_timer, 64);
    }

    #[test]
    fn envelope_round_trip() {
        let mut n = Noise::new();
        n.write_envelope(0xF8 | 0b101); // initial 15, add, period 5
        assert_eq!(n.envelope_initial, 15);
        assert!(n.envelope_add);
        assert_eq!(n.envelope_period, 5);
        assert!(n.dac_enabled);
        assert_eq!(n.read_envelope(), 0xF8 | 0b101);
    }

    #[test]
    fn write_envelope_with_dac_off_disables_channel() {
        let mut n = Noise::new();
        n.dac_enabled = true;
        n.enabled = true;
        n.write_envelope(0x00); // top 5 bits zero → DAC off
        assert!(!n.dac_enabled);
        assert!(!n.enabled);
    }

    #[test]
    fn write_envelope_dac_on_clear_path_keeps_disabled_flag() {
        // initial=0 add=false period=4 → top-5-bits zero → DAC off.
        let mut n = Noise::new();
        n.write_envelope(0b0000_0100);
        assert!(!n.dac_enabled);
    }

    #[test]
    fn poly_round_trip() {
        let mut n = Noise::new();
        n.write_poly(0xA8 | 0b011); // shift 0xA, width=1, divisor 3
        assert_eq!(n.clock_shift, 0xA);
        assert!(n.width_mode);
        assert_eq!(n.divisor_code, 3);
        assert_eq!(n.read_poly(), 0xA8 | 0b011);
    }

    #[test]
    fn poly_width_mode_clear_round_trip() {
        let mut n = Noise::new();
        n.write_poly(0x50); // shift=5, width=0, divisor 0
        assert!(!n.width_mode);
        assert_eq!(n.read_poly(), 0x50);
    }

    #[test]
    fn read_length_enable_reflects_flag() {
        let mut n = Noise::new();
        assert_eq!(n.read_length_enable(), 0xBF);
        n.length_enable = true;
        assert_eq!(n.read_length_enable(), 0xFF);
    }

    // -- write_length_enable quirks -----------------------------------

    #[test]
    fn enable_length_in_first_half_clocks_immediately() {
        let mut n = Noise::new();
        n.length_timer = 4;
        n.length_enable = false;
        n.write_length_enable(0x40, true); // enable, first half, no trigger
        assert_eq!(n.length_timer, 3);
        assert!(n.length_enable);
    }

    #[test]
    fn enable_length_in_first_half_disables_when_reaches_zero() {
        let mut n = Noise::new();
        n.dac_enabled = true;
        n.enabled = true;
        n.length_timer = 1;
        n.length_enable = false;
        n.write_length_enable(0x40, true); // length quirk → 0, no trigger
        assert_eq!(n.length_timer, 0);
        assert!(!n.enabled);
    }

    #[test]
    fn enable_length_in_second_half_does_not_clock() {
        let mut n = Noise::new();
        n.length_timer = 4;
        n.write_length_enable(0x40, false);
        assert_eq!(n.length_timer, 4);
        assert!(n.length_enable);
    }

    #[test]
    fn trigger_with_zero_length_in_first_half_decrements_after_reload() {
        // length=0 + trigger sets length to 64; if length_enable also
        // newly set in first_half, it gets decremented again to 63.
        let mut n = Noise::new();
        n.dac_enabled = true;
        n.length_timer = 0;
        n.write_length_enable(0xC0, true); // enable + trigger, first half
        assert_eq!(n.length_timer, 63);
    }

    #[test]
    fn already_enabled_length_is_not_clocked_on_write() {
        let mut n = Noise::new();
        n.length_timer = 4;
        n.length_enable = true;
        n.write_length_enable(0x40, true); // already enabled — no quirk
        assert_eq!(n.length_timer, 4);
    }
}
