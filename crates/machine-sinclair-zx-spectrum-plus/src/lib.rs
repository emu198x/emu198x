//! ZX Spectrum +2A / +2B / +3 machine.
//!
//! Source references:
//! - `wiki/systems/spectrum/overview.md`
//! - `wiki/systems/spectrum/variants.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/machine-sinclair-zx-spectrum-plus/src/lib.rs`
//!
//! Hardware:
//! - Z80 @ 3.546895 MHz (master / 5)
//! - Amstrad 40077 gate array (different contention to the Sinclair ULAs)
//! - 4 × 16 KB ROMs (selected by `$7FFD` bit 4 + `$1FFD` bit 2)
//! - 128 KB RAM in 8 × 16 KB banks, with extended paging via `$1FFD`
//! - General Instrument AY-3-8912 PSG
//! - +3 only: NEC µPD765A floppy controller on `$2FFD` / `$3FFD`,
//!   reading DSK / EDSK images
//!
//! The Z80 snapshot loader (`apply_snapshot`) is intentionally absent
//! until the `format-sinclair-zx-spectrum-z80` crate is ported across.

pub mod memory;

use amstrad_ula_40077::AmstradGateArray;
use common_sinclair_zx_spectrum::audio::BeeperAudio;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_PLUS2A};
use common_sinclair_zx_spectrum::ula::Ula;
use gi_ay_3_8912::Ay3_8912;
use nec_upd765a::Upd765a;
use zilog_z80::Z80;

use crate::memory::MemoryPlus;

/// Audio output sample rate (44.1 kHz).
const AUDIO_SAMPLE_RATE: u32 = 44_100;

/// Pre-allocated samples-per-frame buffer for the AY downsampler. The
/// +2A/+3's 50 Hz frame produces ~882 samples at 44.1 kHz.
const AUDIO_SAMPLES_PER_FRAME: usize = 882;

/// Which Amstrad-era Spectrum variant this machine emulates. The +2A
/// and +2B share the same chip set with the +3; only the +3 has the
/// floppy drive wired to the FDC, so the FDC starts disabled on the
/// other two variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Model {
    /// Sinclair +2A (1987, grey case, 4 ROMs, no disk).
    Plus2A,
    /// Sinclair +2B (1988, black case, ROM revision, no disk).
    Plus2B,
    /// Sinclair +3 (1987, built-in 3" disk drive).
    Plus3,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SpectrumPlus {
    pub z80: Z80,
    pub ula: AmstradGateArray,
    pub memory: MemoryPlus,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    pub kempston: u8,
    pub tape: TapePlayer,
    pub ay: Ay3_8912,
    pub fdc: Upd765a,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,
    pub model: Model,

    pub(crate) hc: u32,
    beeper_state: bool,
    last_ear: bool,
}

impl SpectrumPlus {
    #[must_use]
    pub fn new(model: Model) -> Self {
        let cpu_hz = (TIMING_PLUS2A.master_hz / u64::from(TIMING_PLUS2A.cpu_divisor)) as u32;
        let ay_hz = cpu_hz / 2;
        // Only the +3 ships the floppy drive. +2A / +2B reuse the same
        // SpectrumPlus struct with an FDC instance whose `enabled` bit
        // is cleared, so its `claims_port` always reports false and the
        // bus dispatch never lands on it.
        let mut fdc = Upd765a::new();
        fdc.enabled = model == Model::Plus3;
        Self {
            z80: Z80::new(),
            ula: AmstradGateArray::new(),
            memory: MemoryPlus::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: 0,
            tape: TapePlayer::new(),
            ay: Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME),
            fdc,
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_PLUS2A.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
            model,
            hc: 0,
            beeper_state: false,
            last_ear: false,
        }
    }

    /// Stable model identifier used by save-state headers and runtime
    /// introspection.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        match self.model {
            Model::Plus2A => "sinclair-zx-spectrum-plus2a",
            Model::Plus2B => "sinclair-zx-spectrum-plus2b",
            Model::Plus3 => "sinclair-zx-spectrum-plus3",
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

    /// Insert a parsed DSK / EDSK image into drive 0. +3 only — has no
    /// effect on +2A / +2B because the FDC's `claims_port` returns
    /// false, but the call still succeeds so callers don't need to
    /// branch on model.
    pub fn insert_disk(&mut self, image: nec_upd765a::DiskImage) {
        self.fdc.insert_disk(0, image);
    }

    /// Eject the disk from drive 0.
    pub fn eject_disk(&mut self) {
        self.fdc.eject_disk(0);
    }

    /// Run exactly one PAL frame.
    pub fn run_frame(&mut self) {
        while self.hc < TIMING_PLUS2A.halfcycles_per_frame {
            self.tick_halfcycle();
        }
        self.end_frame();
    }

    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        for _ in 0..halfcycles {
            self.tick_halfcycle();
            if self.hc >= TIMING_PLUS2A.halfcycles_per_frame {
                self.end_frame();
            }
        }
    }

    pub fn advance_tstates(&mut self, tstates: u32) {
        self.advance_halfcycles(TIMING_PLUS2A.tstates_to_hc(tstates));
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
                self.fdc.tick(self.hc);
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
        self.hc -= TIMING_PLUS2A.halfcycles_per_frame;
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
        // The FDC claims its own ports first — `claims_port` honours
        // the `enabled` flag, so +2A / +2B fall through here.
        if self.fdc.claims_port(port) {
            return self.fdc.read(port);
        }

        if port & 0x0001 == 0 {
            // ULA port ($FE). Bit 6 picks up the tape EAR if playing.
            let mut val = self.ula.read_fe(port, &self.keyboard);
            if self.tape.is_playing() {
                val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
            }
            val
        } else if port & 0xC002 == 0xC000 {
            self.ay.read_data()
        } else if port & 0x00E0 == 0x0000 && port & 0x0001 != 0 {
            self.kempston
        } else {
            // Amstrad gate array does not expose a floating bus.
            0xFF
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        if self.fdc.claims_port(port) {
            self.fdc.write(port, data);
            // Fall through: paging and AY decoding live on orthogonal
            // address bits and may still match the FDC port mask.
        }

        if port & 0x0001 == 0 {
            self.ula.write_fe(data);
            let beeper = data & 0x10 != 0;
            if beeper != self.beeper_state {
                self.beeper_state = beeper;
                let tstate = self.hc / 4;
                self.audio.set_level(tstate, self.speaker_level());
            }
        }

        // Memory paging — the +2A/+3 uses tighter port decoding than
        // the 128K to keep `$1FFD` writes from clobbering `$7FFD`:
        //   $7FFD: A15=0, A14=1, A1=0
        //   $1FFD: A15=0, A14=0, A12=1, A1=0
        if port & 0xC002 == 0x4000 {
            self.memory.write_7ffd(data);
        }
        if port & 0xF002 == 0x1000 {
            self.memory.write_1ffd(data);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let m = SpectrumPlus::new(Model::Plus2A);
        assert_eq!(m.model, Model::Plus2A);
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus2a");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
    }

    #[test]
    fn plus3_enables_fdc_other_models_disable_it() {
        let plus3 = SpectrumPlus::new(Model::Plus3);
        assert!(plus3.fdc.enabled, "+3 ships with FDC enabled");

        for non_disk in [Model::Plus2A, Model::Plus2B] {
            let m = SpectrumPlus::new(non_disk);
            assert!(
                !m.fdc.enabled,
                "{} has no floppy drive — FDC must stay dormant",
                m.model_id()
            );
        }
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = SpectrumPlus::new(Model::Plus3);
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn write_7ffd_via_io_changes_paging() {
        let mut m = SpectrumPlus::new(Model::Plus3);
        m.memory.ram_bank_mut(0)[0] = 0xAA;
        m.memory.ram_bank_mut(3)[0] = 0xBB;
        // Default: bank 0 at $C000.
        assert_eq!(m.memory.read(0xC000), 0xAA);

        // $7FFD = $03: bank 3 selected. Mask: A15=0, A14=1, A1=0.
        m.io_write(0x7FFD, 0x03);
        assert_eq!(m.memory.read(0xC000), 0xBB);
    }

    #[test]
    fn special_paging_via_1ffd_swaps_address_space() {
        let mut m = SpectrumPlus::new(Model::Plus3);
        // Stash recognisable bytes in banks 0..3 and put RAM into
        // special mode 0 (banks 0,1,2,3 across the whole 64K).
        m.memory.ram_bank_mut(0)[0] = 0x10;
        m.memory.ram_bank_mut(1)[0] = 0x11;
        m.memory.ram_bank_mut(2)[0] = 0x12;
        m.memory.ram_bank_mut(3)[0] = 0x13;
        // $1FFD = $01: special mode 0. Mask: A15=0, A14=0, A12=1, A1=0.
        m.io_write(0x1FFD, 0x01);
        assert_eq!(m.memory.read(0x0000), 0x10);
        assert_eq!(m.memory.read(0x4000), 0x11);
        assert_eq!(m.memory.read(0x8000), 0x12);
        assert_eq!(m.memory.read(0xC000), 0x13);
    }
}
