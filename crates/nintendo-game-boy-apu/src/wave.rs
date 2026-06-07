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
        // DMG wave-trigger delay: the first sample fetch after a trigger
        // takes three extra channel cycles (6 T-cycles) beyond the
        // normal `2047 - frequency` period. SameBoy: `sample_countdown =
        // (sample_length ^ 0x7FF) + 3` (apu.c, NR34 trigger). Without it
        // the fetches — and so the wave-RAM-read-while-on window — sit
        // 6 T-cycles early, shifting blargg `09`'s output by three sweep
        // iterations.
        self.period_timer = (2047 - self.frequency) + 3;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_with_dac() -> Wave {
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.enabled = true;
        w
    }

    // -- sample() volume table --------------------------------------

    #[test]
    fn sample_zero_when_disabled() {
        let w = Wave::new();
        assert_eq!(w.sample(), 0.0);
    }

    #[test]
    fn sample_zero_when_dac_off() {
        let mut w = enabled_with_dac();
        w.dac_enabled = false;
        w.volume_code = 1;
        w.current_sample = 15;
        assert_eq!(w.sample(), 0.0);
    }

    #[test]
    fn sample_zero_when_volume_code_zero_mute() {
        let mut w = enabled_with_dac();
        w.volume_code = 0;
        w.current_sample = 15;
        assert_eq!(w.sample(), 0.0);
    }

    #[test]
    fn sample_volume_100_percent() {
        let mut w = enabled_with_dac();
        w.volume_code = 1;
        w.current_sample = 15;
        // 15 / 7.5 - 1.0 = 1.0
        let s = w.sample();
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sample_volume_50_percent_shifts_right_one() {
        let mut w = enabled_with_dac();
        w.volume_code = 2;
        w.current_sample = 14; // >>1 = 7 → 0.0 → -0.0667 ish actually 7/7.5-1
        let s = w.sample();
        let expected = 7.0_f32 / 7.5 - 1.0;
        assert!((s - expected).abs() < 1e-6);
    }

    #[test]
    fn sample_volume_25_percent_shifts_right_two() {
        let mut w = enabled_with_dac();
        w.volume_code = 3;
        w.current_sample = 12; // >>2 = 3
        let s = w.sample();
        let expected = 3.0_f32 / 7.5 - 1.0;
        assert!((s - expected).abs() < 1e-6);
    }

    // -- tick / sample-position -------------------------------------

    #[test]
    fn tick_decrements_when_period_nonzero() {
        let mut w = Wave::new();
        w.period_timer = 5;
        let ram = [0u8; 16];
        w.tick(&ram);
        assert_eq!(w.period_timer, 4);
        assert!(!w.wave_just_read);
    }

    #[test]
    fn tick_advances_sample_position_and_reads_high_nibble() {
        let mut w = Wave::new();
        w.frequency = 2046; // period reload = 1
        w.period_timer = 0;
        w.sample_position = 0; // will become 1 after advance
        let mut ram = [0u8; 16];
        ram[0] = 0xA5; // sample_position=1 → low nibble = 0x5
        w.tick(&ram);
        assert_eq!(w.sample_position, 1);
        assert_eq!(w.current_sample, 0x5);
        assert!(w.wave_just_read);
        assert_eq!(w.period_timer, 1);
    }

    #[test]
    fn tick_high_nibble_when_position_even() {
        let mut w = Wave::new();
        w.frequency = 2047;
        w.period_timer = 0;
        w.sample_position = 1; // → 2 (even) after advance
        let mut ram = [0u8; 16];
        ram[1] = 0xC3; // sample_position=2 → byte at idx 1, high = 0xC
        w.tick(&ram);
        assert_eq!(w.sample_position, 2);
        assert_eq!(w.current_sample, 0xC);
    }

    #[test]
    fn tick_wraps_sample_position_to_zero() {
        let mut w = Wave::new();
        w.frequency = 2047;
        w.period_timer = 0;
        w.sample_position = 31; // → 0 after wrap
        let mut ram = [0u8; 16];
        ram[0] = 0x9F;
        w.tick(&ram);
        assert_eq!(w.sample_position, 0);
        assert_eq!(w.current_sample, 0x9);
    }

    // -- step_length -------------------------------------------------

    #[test]
    fn step_length_no_op_when_disabled() {
        let mut w = enabled_with_dac();
        w.length_enable = false;
        w.length_timer = 5;
        w.step_length();
        assert_eq!(w.length_timer, 5);
        assert!(w.enabled);
    }

    #[test]
    fn step_length_decrements_and_disables_at_zero() {
        let mut w = enabled_with_dac();
        w.length_enable = true;
        w.length_timer = 1;
        w.step_length();
        assert_eq!(w.length_timer, 0);
        assert!(!w.enabled);
    }

    #[test]
    fn step_length_does_not_underflow_at_zero() {
        let mut w = enabled_with_dac();
        w.length_enable = true;
        w.length_timer = 0;
        w.step_length();
        assert_eq!(w.length_timer, 0);
        assert!(w.enabled);
    }

    // -- Register I/O ------------------------------------------------

    #[test]
    fn write_length_decodes_256_minus_value() {
        let mut w = Wave::new();
        w.write_length(0x00);
        assert_eq!(w.length_timer, 256);
        w.write_length(0xFF);
        assert_eq!(w.length_timer, 1);
        w.write_length(0x80);
        assert_eq!(w.length_timer, 128);
    }

    #[test]
    fn write_freq_lo_replaces_low_byte_only() {
        let mut w = Wave::new();
        w.frequency = 0x0700; // upper 3 bits set
        w.write_freq_lo(0xAB);
        assert_eq!(w.frequency, 0x07AB);
        w.write_freq_lo(0x00);
        assert_eq!(w.frequency, 0x0700);
    }

    #[test]
    fn read_freq_hi_reflects_length_enable() {
        let mut w = Wave::new();
        assert_eq!(w.read_freq_hi(), 0xBF);
        w.length_enable = true;
        assert_eq!(w.read_freq_hi(), 0xFF);
    }

    #[test]
    fn write_freq_hi_replaces_upper_3_bits() {
        let mut w = Wave::new();
        w.frequency = 0x00AB;
        let mut ram = [0u8; 16];
        // Top bits = 5; no length-enable, no trigger.
        w.write_freq_hi(0b0000_0101, false, &mut ram);
        assert_eq!(w.frequency, 0x05AB);
    }

    // -- trigger paths -----------------------------------------------

    #[test]
    fn trigger_via_write_freq_hi_sets_state() {
        let mut w = Wave::new();
        w.dac_enabled = true;
        // Pre-load the low byte; the trigger value also carries the
        // high 3 bits of frequency in its low 3 bits.
        w.frequency = 0x00E8; // low byte 0xE8
        w.length_timer = 0;
        let mut ram = [0u8; 16];
        // Trigger + freq high bits = 3 → frequency = 0x3E8 = 1000.
        w.write_freq_hi(0x83, false, &mut ram);
        assert!(w.enabled);
        assert_eq!(w.length_timer, 256);
        assert_eq!(w.frequency, 1000);
        // The DMG wave-trigger delay adds 3 channel cycles to the first
        // period: (2047 - 1000) + 3.
        assert_eq!(w.period_timer, (2047 - 1000) + 3);
        assert_eq!(w.sample_position, 0);
    }

    #[test]
    fn trigger_with_dac_off_does_not_enable_channel() {
        let mut w = Wave::new();
        w.dac_enabled = false;
        let mut ram = [0u8; 16];
        w.write_freq_hi(0x80, false, &mut ram);
        assert!(!w.enabled);
    }

    #[test]
    fn trigger_preserves_nonzero_length() {
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.length_timer = 50;
        let mut ram = [0u8; 16];
        w.write_freq_hi(0x80, false, &mut ram);
        assert_eq!(w.length_timer, 50);
    }

    // -- Wave-RAM corruption quirk on retrigger ----------------------

    #[test]
    fn retrigger_with_offset_lt_4_overwrites_byte_zero() {
        // When already enabled with period_timer about to clock, the
        // re-trigger copies wave_ram[offset] into wave_ram[0] (offset
        // < 4 path). With sample_position=1 → (1+1)/2 = 1 → offset 1.
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.enabled = true;
        w.period_timer = 0;
        w.sample_position = 1;
        w.frequency = 2047;
        let mut ram = [0u8; 16];
        ram[0] = 0x11;
        ram[1] = 0xAB;
        w.write_freq_hi(0x80, false, &mut ram);
        assert_eq!(ram[0], 0xAB);
    }

    #[test]
    fn retrigger_with_offset_ge_4_copies_4byte_block() {
        // sample_position=7 → (7+1)/2 = 4 → offset 4 → copy 4 bytes
        // from src=4 into [0..4].
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.enabled = true;
        w.period_timer = 0;
        w.sample_position = 7;
        w.frequency = 2047;
        let mut ram = [0u8; 16];
        ram[0] = 0x00;
        ram[1] = 0x00;
        ram[2] = 0x00;
        ram[3] = 0x00;
        ram[4] = 0xAA;
        ram[5] = 0xBB;
        ram[6] = 0xCC;
        ram[7] = 0xDD;
        w.write_freq_hi(0x80, false, &mut ram);
        assert_eq!(ram[0], 0xAA);
        assert_eq!(ram[1], 0xBB);
        assert_eq!(ram[2], 0xCC);
        assert_eq!(ram[3], 0xDD);
    }

    #[test]
    fn retrigger_corruption_skipped_when_period_timer_nonzero() {
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.enabled = true;
        w.period_timer = 5; // not about to clock
        w.sample_position = 1;
        let mut ram = [0u8; 16];
        ram[0] = 0x11;
        ram[1] = 0xAB;
        w.write_freq_hi(0x80, false, &mut ram);
        assert_eq!(ram[0], 0x11, "no corruption when period_timer != 0");
    }

    #[test]
    fn retrigger_corruption_skipped_when_channel_disabled() {
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.enabled = false;
        w.period_timer = 0;
        w.sample_position = 1;
        let mut ram = [0u8; 16];
        ram[0] = 0x11;
        ram[1] = 0xAB;
        w.write_freq_hi(0x80, false, &mut ram);
        assert_eq!(ram[0], 0x11, "no corruption when channel disabled");
    }

    // -- write_freq_hi length-enable quirks --------------------------

    #[test]
    fn enable_length_in_first_half_clocks_immediately() {
        let mut w = Wave::new();
        w.length_timer = 4;
        let mut ram = [0u8; 16];
        w.write_freq_hi(0x40, true, &mut ram); // enable, first half, no trigger
        assert_eq!(w.length_timer, 3);
        assert!(w.length_enable);
    }

    #[test]
    fn enable_length_in_first_half_disables_when_reaches_zero() {
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.enabled = true;
        w.length_timer = 1;
        let mut ram = [0u8; 16];
        w.write_freq_hi(0x40, true, &mut ram); // length-quirk → 0, no trigger
        assert_eq!(w.length_timer, 0);
        assert!(!w.enabled);
    }

    #[test]
    fn enable_length_in_second_half_does_not_clock() {
        let mut w = Wave::new();
        w.length_timer = 4;
        let mut ram = [0u8; 16];
        w.write_freq_hi(0x40, false, &mut ram);
        assert_eq!(w.length_timer, 4);
        assert!(w.length_enable);
    }

    #[test]
    fn trigger_with_zero_length_in_first_half_decrements_after_reload() {
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.length_timer = 0;
        let mut ram = [0u8; 16];
        // enable length + trigger, first half → trigger reloads to
        // 256 then quirk decrements to 255.
        w.write_freq_hi(0xC0, true, &mut ram);
        assert_eq!(w.length_timer, 255);
    }

    // -- DAC enable register -----------------------------------------

    #[test]
    fn dac_enable_round_trip() {
        let mut w = Wave::new();
        w.write_dac_enable(0x80);
        assert!(w.dac_enabled);
        assert_eq!(w.read_dac_enable(), 0xFF);
        w.enabled = true;
        w.write_dac_enable(0x00);
        assert!(!w.dac_enabled);
        assert!(!w.enabled, "clearing DAC also disables channel");
        assert_eq!(w.read_dac_enable(), 0x7F);
    }

    // -- Volume register ---------------------------------------------

    #[test]
    fn volume_round_trip_all_codes() {
        for code in 0..4u8 {
            let mut w = Wave::new();
            w.write_volume(code << 5);
            assert_eq!(w.volume_code, code);
            assert_eq!(w.read_volume() & 0x60, code << 5);
            assert_eq!(w.read_volume() | 0x60, 0xFF, "unused bits read high");
        }
    }

    // -- reset_preserve_length ---------------------------------------

    #[test]
    fn reset_preserve_length_preserves_only_length() {
        let mut w = Wave::new();
        w.dac_enabled = true;
        w.enabled = true;
        w.volume_code = 2;
        w.frequency = 1000;
        w.length_timer = 99;
        w.reset_preserve_length();
        assert_eq!(w.length_timer, 99);
        assert!(!w.enabled);
        assert!(!w.dac_enabled);
        assert_eq!(w.volume_code, 0);
        assert_eq!(w.frequency, 0);
    }
}
