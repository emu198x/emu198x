//! ZX Spectrum 128K / +2 machine.
//!
//! Source references:
//! - `wiki/systems/spectrum/overview.md`
//! - `wiki/systems/spectrum/variants.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/machine-sinclair-zx-spectrum-128k/src/lib.rs`
//!
//! Hardware:
//! - Z80 @ 3.546895 MHz (master / 5)
//! - Sinclair 7K010E ULA (228 T-states/line, 311 lines)
//! - 128 KB RAM in 8 × 16 KB banks (5 fixed at $4000, 2 at $8000,
//!   one switchable at $C000 via port $7FFD)
//! - Two 16 KB ROMs (128K editor + 48K BASIC, selected by port $7FFD bit 4)
//! - General Instrument AY-3-8912 PSG (ports $FFFD select, $BFFD data)
//!
//! The Z80-snapshot loader (`apply_snapshot`) is intentionally absent
//! until the `format-sinclair-zx-spectrum-z80` crate is ported across.

pub mod memory;

use common_sinclair_zx_spectrum::audio::BeeperAudio;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_128K};
use common_sinclair_zx_spectrum::ula::Ula;
use gi_ay_3_8912::Ay3_8912;
use sinclair_ula_7k010e::SinclairUla;
use zilog_z80::Z80;

use crate::memory::Memory128K;

/// Audio output sample rate (44.1 kHz).
const AUDIO_SAMPLE_RATE: u32 = 44_100;

/// Pre-allocated samples-per-frame buffer for the AY downsampler. The
/// 128K's 50 Hz frame produces ~882 samples at 44.1 kHz.
const AUDIO_SAMPLES_PER_FRAME: usize = 882;

/// 128K-family hardware variant.
///
/// Both share the same chip set — the +2 just adds a built-in tape deck
/// and a restyled case running an Amstrad-edited but functionally
/// identical ROM set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Variant {
    /// Original Sinclair 128K ("toastrack").
    Original,
    /// Sinclair-branded +2 (1986, Amstrad-manufactured, grey case, built-in tape).
    Plus2,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Spectrum128K {
    pub z80: Z80,
    pub ula: SinclairUla,
    pub memory: Memory128K,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    pub kempston: u8,
    pub tape: TapePlayer,
    pub ay: Ay3_8912,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,
    pub variant: Variant,

    pub(crate) hc: u32,
    beeper_state: bool,
    last_ear: bool,
}

impl Spectrum128K {
    #[must_use]
    pub fn new() -> Self {
        Self::with_variant(Variant::Original)
    }

    #[must_use]
    pub fn new_plus2() -> Self {
        Self::with_variant(Variant::Plus2)
    }

    fn with_variant(variant: Variant) -> Self {
        let cpu_hz = (TIMING_128K.master_hz / u64::from(TIMING_128K.cpu_divisor)) as u32;
        let ay_hz = cpu_hz / 2;
        Self {
            z80: Z80::new(),
            ula: SinclairUla::new(),
            memory: Memory128K::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: 0,
            tape: TapePlayer::new(),
            ay: Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_128K.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
            variant,
            hc: 0,
            beeper_state: false,
            last_ear: false,
        }
    }

    /// Stable model identifier used by save-state headers and runtime
    /// introspection.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        match self.variant {
            Variant::Original => "sinclair-zx-spectrum-128k",
            Variant::Plus2 => "sinclair-zx-spectrum-plus2",
        }
    }

    pub fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        self.tape.load_blocks(blocks);
    }

    pub fn load_tape_pulses(&mut self, pulses: Vec<u32>) {
        self.tape.load_pulses(pulses);
    }

    pub fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        self.tape.load_stream(stream);
    }

    pub fn tape_play(&mut self) {
        self.tape.play();
    }

    /// Run exactly one PAL frame.
    pub fn run_frame(&mut self) {
        while self.hc < TIMING_128K.halfcycles_per_frame {
            self.tick_halfcycle();
        }
        self.end_frame();
    }

    /// Advance the machine by an exact number of master-clock half-cycles.
    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        for _ in 0..halfcycles {
            self.tick_halfcycle();
            if self.hc >= TIMING_128K.halfcycles_per_frame {
                self.end_frame();
            }
        }
    }

    /// Advance the machine by an exact number of CPU T-states.
    pub fn advance_tstates(&mut self, tstates: u32) {
        self.advance_halfcycles(TIMING_128K.tstates_to_hc(tstates));
    }

    fn tick_halfcycle(&mut self) {
        if self.hc & 1 == 0 {
            self.ula.tick(
                &self.memory,
                self.z80.addr,
                self.z80.mreq,
                self.z80.iorq,
                &mut self.framebuffer,
            );

            if self.ula.cpu_clock_active() {
                self.z80.tick();
                self.handle_bus();
            }

            self.z80.irq = self.ula.interrupt_active();

            if self.hc % 4 == 2 {
                self.tape.advance_tstates(1);
                if self.hc % 8 == 2 {
                    self.ay.tick();
                }
                let ear = self.tape.ear_level();
                if ear != self.last_ear {
                    self.last_ear = ear;
                    let tstate = self.hc / 4;
                    self.audio.set_level(tstate, self.speaker_level());
                }
            }
        }

        self.hc += 1;
    }

    fn end_frame(&mut self) {
        self.ula.end_frame();
        self.audio.end_frame(&mut self.audio_frame);
        self.hc -= TIMING_128K.halfcycles_per_frame;
    }

    fn handle_bus(&mut self) {
        if self.z80.mreq && self.z80.rd {
            self.z80.data_in = self.memory.read(self.z80.addr);
        } else if self.z80.mreq && self.z80.wr {
            self.memory.write(self.z80.addr, self.z80.data);
        } else if self.z80.iorq && self.z80.rd && !self.z80.m1 {
            self.z80.data_in = self.io_read(self.z80.addr);
        } else if self.z80.iorq && self.z80.wr {
            self.io_write(self.z80.addr, self.z80.data);
        } else if self.z80.iorq && self.z80.m1 {
            self.z80.data_in = 0xFF;
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        if port & 0x0001 == 0 {
            // ULA port ($FE). Bit 6 picks up the tape EAR if playing.
            let mut val = self.ula.read_fe(port, &self.keyboard);
            if self.tape.is_playing() {
                val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
            }
            val
        } else if port & 0xC002 == 0xC000 {
            // AY register read ($FFFD).
            self.ay.read_data()
        } else if port & 0x00E0 == 0x0000 && port & 0x0001 != 0 {
            // Kempston joystick (active when A5=0 and A0=1).
            self.kempston
        } else {
            self.ula.floating_bus()
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        if port & 0x0001 == 0 {
            self.ula.write_fe(data);
            let beeper = data & 0x10 != 0;
            if beeper != self.beeper_state {
                self.beeper_state = beeper;
                let tstate = self.hc / 4;
                self.audio.set_level(tstate, self.speaker_level());
            }
        }

        // Memory paging: port $7FFD (active when bit 1 = 0 and bit 15 = 0).
        if port & 0x8002 == 0x0000 {
            self.memory.write_7ffd(data);
        }

        // AY register select ($FFFD) and data write ($BFFD).
        if port & 0xC002 == 0xC000 {
            self.ay.select_register(data);
        } else if port & 0xC002 == 0x8000 {
            self.ay.write_data(data);
        }
    }

    fn speaker_level(&self) -> f32 {
        let beeper = if self.beeper_state { 0.8 } else { 0.0 };
        let ear = if self.last_ear { 0.2 } else { 0.0 };
        beeper + ear
    }

    pub fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }
}

impl Default for Spectrum128K {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let m = Spectrum128K::new();
        assert_eq!(m.variant, Variant::Original);
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-128k");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
        assert_eq!(m.kempston, 0);
    }

    #[test]
    fn plus2_variant_uses_distinct_model_id() {
        let m = Spectrum128K::new_plus2();
        assert_eq!(m.variant, Variant::Plus2);
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus2");
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = Spectrum128K::new();
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn advance_tstates_tracks_halfcycle_position() {
        let mut m = Spectrum128K::new();
        m.advance_tstates(7);
        // 7 T-states × 5 hc/T = 35 half-cycles
        assert_eq!(m.hc, 35);
    }

    #[test]
    fn write_7ffd_via_io_changes_paging() {
        let mut m = Spectrum128K::new();
        // Distinct values in banks 0 and 3.
        m.memory.ram_bank_mut(0)[0] = 0xAA;
        m.memory.ram_bank_mut(3)[0] = 0xBB;
        // Default: bank 0 at $C000.
        assert_eq!(m.memory.read(0xC000), 0xAA);

        // Write 3 to $7FFD via io_write — bit 15 = 0, bit 1 = 0 satisfies the mask.
        m.io_write(0x7FFD, 0x03);
        assert_eq!(m.memory.current_bank(), 3);
        assert_eq!(m.memory.read(0xC000), 0xBB);
    }

    #[test]
    fn ay_register_select_and_read_via_io() {
        let mut m = Spectrum128K::new();
        // Select AY register 8 ($FFFD with bit 14 = 1).
        m.io_write(0xFFFD, 0x08);
        // Write a value to that register ($BFFD with bit 14 = 1, bit 15 = 1).
        m.io_write(0xBFFD, 0x0A);
        // Read it back via $FFFD (port & 0xC002 == 0xC000).
        assert_eq!(m.io_read(0xFFFD), 0x0A);
    }
}
