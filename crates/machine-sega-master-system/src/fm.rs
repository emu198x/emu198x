//! The YM2413 FM sound hardware: the Japanese Master System's built-in chip
//! and the Mark III's FM Sound Unit.
//!
//! Both put a YM2413 at ports `$F0` (register address) and `$F1` (data),
//! clocked from the master crystal divided by three, the same 3.58 MHz as
//! the Z80. They differ in the audio control register at `$F2`, which is
//! also how software finds them.
//!
//! - **Built in** (Japanese Master System). `$F2` bits 0-1 select the mix:
//!   0 PSG only (the power-on state), 1 FM only, 2 neither, 3 both. Reading
//!   `$F2` returns those two bits; the top three bits carry a counter of the
//!   VDP's composite sync pulses, which this model does not have and reads
//!   as zero. Software only tests the low bits.
//! - **FM Sound Unit** (the Mark III's FM-70 add-on). `$F2` bit 0 alone:
//!   0 PSG, 1 FM. The unit has no mixer, so FM on mutes the PSG. Every port
//!   it decodes, `$F0`-`$F3`, reads back that bit.
//!
//! Detection is the same on both: write 1 to `$F2` and read it back. A
//! machine without the hardware returns whatever the controller port
//! returns there instead.
//!
//! The port semantics follow MAME's `smsj` driver and its `sega_fm_unit`
//! device (BSD-3-Clause), read for facts only; MAME takes the latter from
//! Charles MacDonald's reading of Enri's schematics. MAME also masks the
//! register address to six bits, which this model follows.
//!
//! # Audio
//!
//! The chip produces a sample every 72 clocks, about 49.7 kHz. This module
//! averages it down to 48 kHz on exactly the cadence the SN76489 uses
//! (a phase accumulator stepped every 16 clocks), so the FM and PSG buffers
//! stay the same length and mix sample for sample. One FM channel at full
//! volume is scaled to the level of one PSG tone at full volume, and the
//! rhythm pin counts twice the melody pin, as the YM2413 application manual
//! has it; both are nominal, not measured against a board.

use emu198x_yamaha_ym2413::{Variant, Ym2413};
use serde::{Deserialize, Serialize};

/// The output sample rate, matching the SN76489's.
const SAMPLE_RATE: u32 = 48_000;

/// Which FM hardware, if any, the machine carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FmUnit {
    /// No YM2413: export Master Systems, the Game Gear.
    #[default]
    None,
    /// The Japanese Master System's on-board YM2413, with its two-bit mixer.
    BuiltIn,
    /// The Mark III's FM Sound Unit: FM or PSG, never both.
    SoundUnit,
}

/// The FM hardware's live state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FmSound {
    pub(crate) unit: FmUnit,
    pub(crate) ym: Ym2413,
    /// The audio control register as written, masked to the bits the
    /// hardware keeps.
    control: u8,
    /// Clocks since the last resampler step, which happens every 16.
    divider: u8,
    phase: u32,
    step_hz: u32,
    accumulator: i64,
    count: u32,
    /// The DC-blocking filter's previous input and output. The chip's
    /// output is offset from zero, and the board AC-couples it, as it does
    /// the PSG's.
    hp_prev_in: f32,
    hp_prev_out: f32,
    buffer: Vec<f32>,
}

impl FmSound {
    pub(crate) fn new(unit: FmUnit, clock_hz: u32) -> Self {
        Self {
            unit,
            ym: Ym2413::new(Variant::Ym2413),
            control: 0,
            divider: 0,
            phase: 0,
            step_hz: clock_hz / 16,
            accumulator: 0,
            count: 0,
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
            buffer: Vec::new(),
        }
    }

    /// One Z80 T-state: the chip's input clock.
    pub(crate) fn tick(&mut self) {
        self.ym.tick();
        if self.ym.prescaler == 0 {
            self.accumulator += i64::from(self.ym.mo) + 2 * i64::from(self.ym.ro);
            self.count += 1;
        }
        self.divider += 1;
        if self.divider < 16 {
            return;
        }
        self.divider = 0;
        self.phase += SAMPLE_RATE;
        if self.phase >= self.step_hz {
            self.phase -= self.step_hz;
            // The pins summed over 18 internal clocks are one sample, so an
            // average over internal clocks is a sample divided by 18.
            let average = self.accumulator as f32 / self.count.max(1) as f32;
            let sample = average * 18.0 / 2048.0;
            // The same one-pole high-pass the SN76489 applies.
            const DC_BLOCK_ALPHA: f32 = 0.9952;
            let filtered = DC_BLOCK_ALPHA * (self.hp_prev_out + sample - self.hp_prev_in);
            self.hp_prev_in = sample;
            self.hp_prev_out = filtered;
            self.buffer.push(filtered);
            self.accumulator = 0;
            self.count = 0;
        }
    }

    /// Whether the PSG reaches the speaker under the current control value.
    pub(crate) fn psg_audible(&self) -> bool {
        match self.unit {
            FmUnit::None => true,
            FmUnit::BuiltIn => matches!(self.control, 0 | 3),
            FmUnit::SoundUnit => self.control == 0,
        }
    }

    /// Whether the FM chip reaches the speaker.
    pub(crate) fn fm_audible(&self) -> bool {
        match self.unit {
            FmUnit::None => false,
            FmUnit::BuiltIn => matches!(self.control, 1 | 3),
            FmUnit::SoundUnit => self.control == 1,
        }
    }

    /// Handle a write to `port`, returning whether this hardware decodes it.
    pub(crate) fn write(&mut self, port: u8, value: u8) -> bool {
        match (self.unit, port) {
            (FmUnit::None, _) => false,
            (_, 0xF0) => {
                self.ym.write(0, value & 0x3F);
                true
            }
            (_, 0xF1) => {
                self.ym.write(1, value);
                true
            }
            (FmUnit::BuiltIn, 0xF2) => {
                self.control = value & 0x03;
                true
            }
            (FmUnit::SoundUnit, 0xF2 | 0xF3) => {
                self.control = value & 0x01;
                true
            }
            _ => false,
        }
    }

    /// Handle a read of `port`, returning `None` when this hardware does not
    /// drive the bus there.
    pub(crate) fn read(&self, port: u8) -> Option<u8> {
        match (self.unit, port) {
            (FmUnit::BuiltIn, 0xF2) => Some(self.control),
            (FmUnit::SoundUnit, 0xF0..=0xF3) => Some(self.control),
            _ => None,
        }
    }

    /// Mix the FM output into a buffer of PSG samples of the same cadence,
    /// muting either source as the control register says.
    pub(crate) fn mix(&mut self, psg: Vec<f32>) -> Vec<f32> {
        let fm = std::mem::take(&mut self.buffer);
        let psg_gain = if self.psg_audible() { 1.0 } else { 0.0 };
        let fm_gain = if self.fm_audible() { 1.0 } else { 0.0 };
        psg.iter()
            .enumerate()
            .map(|(i, &p)| p * psg_gain + fm.get(i).copied().unwrap_or(0.0) * fm_gain)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FmUnit;
    use crate::{Sms, SmsVariant};

    /// Where the program stores what `$F2` read back.
    const DETECT: u16 = 0xC000;

    /// A cartridge that detects FM as Japanese software does (write 1 to
    /// `$F2`, read it back), stores the result, keys a note on channel 0
    /// with the datasheet's pauses after each write, and loops.
    fn fm_cart() -> Vec<u8> {
        let mut code = vec![
            0xF3, // DI
            0x31, 0xF0, 0xDF, // LD SP,$DFF0
            0x3E, 0x01, // LD A,1
            0xD3, 0xF2, // OUT ($F2),A
            0xDB, 0xF2, // IN A,($F2)
            0x32, 0x00, 0xC0, // LD ($C000),A
        ];
        for (register, value) in [(0x30u8, 0x10u8), (0x10, 0xAC), (0x20, 0x1C)] {
            code.extend([
                0x3E, register, // LD A,register
                0xD3, 0xF0, // OUT ($F0),A
                0x06, 0x08, // LD B,8
                0x10, 0xFE, // DJNZ $
                0x3E, value, // LD A,value
                0xD3, 0xF1, // OUT ($F1),A
                0x06, 0x08, // LD B,8
                0x10, 0xFE, // DJNZ $
            ]);
        }
        code.extend([0x18, 0xFE]); // JR $
        code.resize(0x8000, 0xFF);
        code
    }

    /// How far the mixed output swings over `frames` frames.
    fn swing(sys: &mut Sms, frames: u32) -> f32 {
        let mut samples = Vec::new();
        for _ in 0..frames {
            sys.run_frame();
            samples.extend(sys.take_audio_buffer());
        }
        // Skip the first frame, while the note is still being keyed.
        let tail = &samples[samples.len() / frames as usize..];
        let max = tail.iter().copied().fold(f32::MIN, f32::max);
        let min = tail.iter().copied().fold(f32::MAX, f32::min);
        max - min
    }

    #[test]
    fn the_japanese_master_system_has_fm_built_in_and_software_finds_it() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsJapanNtsc);
        assert_eq!(sys.fm_unit(), FmUnit::BuiltIn);
        let loud = swing(&mut sys, 10);
        assert_eq!(sys.peek(DETECT), 0x01, "$F2 reads back the control bits");
        assert!(loud > 0.1, "the FM note sounds: swing {loud}");
    }

    #[test]
    fn an_export_master_system_has_no_fm_and_software_finds_none() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsNtsc);
        assert_eq!(sys.fm_unit(), FmUnit::None);
        assert!(sys.ym2413().is_none());
        let quiet = swing(&mut sys, 10);
        assert_ne!(sys.peek(DETECT), 0x01, "$F2 is the controller port here");
        assert!(quiet < 0.01, "nothing sounds: swing {quiet}");
    }

    #[test]
    fn the_fm_sound_unit_answers_on_all_four_ports_and_mutes_the_psg() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsNtsc);
        sys.set_fm_unit(FmUnit::SoundUnit);
        let loud = swing(&mut sys, 10);
        assert_eq!(sys.peek(DETECT), 0x01);
        assert!(loud > 0.1, "the FM note sounds: swing {loud}");
        for port in 0xF0..=0xF3 {
            assert_eq!(sys.io_read(port), 0x01, "port {port:#x}");
        }
        sys.io_write(0xF3, 0x03);
        assert_eq!(sys.io_read(0xF2), 0x01, "the unit keeps only bit 0");
        let fm = sys.fm.as_ref().expect("fitted");
        assert!(fm.fm_audible() && !fm.psg_audible());
    }

    #[test]
    fn the_built_in_mixer_selects_psg_fm_neither_or_both() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsJapanNtsc);
        let cases = [
            (0, true, false),
            (1, false, true),
            (2, false, false),
            (3, true, true),
        ];
        for (control, psg, fm) in cases {
            sys.io_write(0xF2, control | 0xFC);
            assert_eq!(sys.io_read(0xF2), control, "only bits 0-1 are kept");
            let state = sys.fm.as_ref().expect("built in");
            assert_eq!((state.psg_audible(), state.fm_audible()), (psg, fm));
        }
        // Built-in hardware decodes $F2 only; $F0 still reads the controller.
        assert_eq!(sys.io_read(0xF0), sys.read_controller_port(1));
    }

    #[test]
    fn muting_fm_silences_the_note() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsJapanNtsc);
        swing(&mut sys, 4);
        sys.io_write(0xF2, 0x00);
        let muted = swing(&mut sys, 10);
        assert!(
            muted < 0.01,
            "PSG only, and the PSG is silent: swing {muted}"
        );
    }

    #[test]
    fn fm_and_psg_buffers_stay_the_same_length() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsJapanNtsc);
        for _ in 0..30 {
            sys.run_frame();
            let fm_len = sys.fm.as_ref().expect("built in").buffer.len();
            let mixed = sys.take_audio_buffer();
            assert_eq!(mixed.len(), fm_len, "one FM sample per PSG sample");
        }
    }

    #[test]
    fn removing_the_unit_restores_the_controller_port() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsJapanNtsc);
        sys.set_fm_unit(FmUnit::None);
        assert_eq!(sys.fm_unit(), FmUnit::None);
        assert_eq!(sys.io_read(0xF2), sys.read_controller_port(1));
    }

    #[test]
    fn a_snapshot_carries_the_fm_state() {
        let mut sys = Sms::new(fm_cart(), SmsVariant::SmsJapanNtsc);
        swing(&mut sys, 3);
        let bytes = postcard::to_allocvec(&sys).expect("serialises");
        let mut copy: Sms = postcard::from_bytes(&bytes).expect("deserialises");
        sys.run_frame();
        copy.run_frame();
        assert_eq!(sys.take_audio_buffer(), copy.take_audio_buffer());
    }
}
