//! Pentagon 128 machine.
//!
//! Source references:
//! - `wiki/systems/spectrum/variants.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/machine-pentagon-128/src/lib.rs`
//!
//! Hardware:
//! - Z80 @ 3.584 MHz (master / 4)
//! - Pentagon ULA — no contention, 14.336 MHz crystal, 320 lines/frame
//! - 128 KB RAM in 8 × 16 KB banks (same `$7FFD` paging as the 128K)
//! - Two 16 KB ROMs (128 editor + 48 BASIC, or Pentagon-specific ROMs)
//! - General Instrument AY-3-8912 PSG
//! - Beta 128 disk interface — TR-DOS ROM trap on `$3DXX` fetches in
//!   ROM space, WD1793 stub on its own port range

pub mod memory;

use beta_disk_interface::BetaDisk;
use common_sinclair_zx_spectrum::audio::BeeperAudio;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_PENTAGON};
use common_sinclair_zx_spectrum::ula::Ula;
use format_sinclair_zx_spectrum_z80::Z80Snapshot;
use gi_ay_3_8912::Ay3_8912;
use pentagon_ula::PentagonUla;
use zilog_z80::Z80;

use crate::memory::MemoryPentagon;

const AUDIO_SAMPLE_RATE: u32 = 44_100;
const AUDIO_SAMPLES_PER_FRAME: usize = 882;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Pentagon128 {
    pub z80: Z80,
    pub ula: PentagonUla,
    pub memory: MemoryPentagon,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    pub kempston: u8,
    pub tape: TapePlayer,
    pub ay: Ay3_8912,
    pub beta: BetaDisk,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,

    pub(crate) hc: u32,
    beeper_state: bool,
    last_ear: bool,
}

impl Pentagon128 {
    #[must_use]
    pub fn new() -> Self {
        let cpu_hz = (TIMING_PENTAGON.master_hz / u64::from(TIMING_PENTAGON.cpu_divisor)) as u32;
        let ay_hz = cpu_hz / 2;
        Self {
            z80: Z80::new(),
            ula: PentagonUla::new(),
            memory: MemoryPentagon::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: 0,
            tape: TapePlayer::new(),
            ay: Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME),
            beta: BetaDisk::new(),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_PENTAGON.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
            hc: 0,
            beeper_state: false,
            last_ear: false,
        }
    }

    #[must_use]
    pub fn model_id(&self) -> &'static str {
        "pentagon-128"
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

    /// Apply a parsed `.z80` snapshot. Pentagon shares the 128K page
    /// layout (8 banked RAM pages, `$7FFD` paging, AY register file) —
    /// it has no `$1FFD`.
    pub fn apply_snapshot(&mut self, snap: &Z80Snapshot) {
        self.z80.regs.af = snap.af;
        self.z80.regs.bc = snap.bc;
        self.z80.regs.de = snap.de;
        self.z80.regs.hl = snap.hl;
        self.z80.regs.af_alt = snap.af_alt;
        self.z80.regs.bc_alt = snap.bc_alt;
        self.z80.regs.de_alt = snap.de_alt;
        self.z80.regs.hl_alt = snap.hl_alt;
        self.z80.regs.ix = snap.ix;
        self.z80.regs.iy = snap.iy;
        self.z80.regs.sp = snap.sp;
        self.z80.regs.pc = snap.pc;
        self.z80.regs.i = snap.i;
        self.z80.regs.r = snap.r;
        self.z80.regs.im = snap.im;
        self.z80.regs.iff1 = snap.iff1;
        self.z80.regs.iff2 = snap.iff2;
        self.ula.write_fe(snap.border);

        for (page, data) in &snap.pages {
            if (3..=10).contains(page) {
                let bank = (*page - 3) as usize;
                let base: u16 = match bank {
                    5 => 0x4000,
                    2 => 0x8000,
                    _ => {
                        self.memory.write_7ffd(bank as u8);
                        0xC000
                    }
                };
                for (i, &byte) in data.iter().enumerate() {
                    self.memory.write(base.wrapping_add(i as u16), byte);
                }
            }
        }
        self.memory.write_7ffd(snap.port_7ffd);

        for (reg, &val) in snap.ay_regs.iter().enumerate() {
            self.ay.select_register(reg as u8);
            self.ay.write_data(val);
        }
        self.ay.select_register(snap.ay_register);
    }

    /// Run exactly one PAL frame.
    pub fn run_frame(&mut self) {
        while self.hc < TIMING_PENTAGON.halfcycles_per_frame {
            self.tick_halfcycle();
        }
        self.end_frame();
    }

    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        for _ in 0..halfcycles {
            self.tick_halfcycle();
            if self.hc >= TIMING_PENTAGON.halfcycles_per_frame {
                self.end_frame();
            }
        }
    }

    pub fn advance_tstates(&mut self, tstates: u32) {
        self.advance_halfcycles(TIMING_PENTAGON.tstates_to_hc(tstates));
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

            // Pentagon has no contention — CPU always ticks.
            self.z80.tick();
            self.handle_bus();

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
        self.hc -= TIMING_PENTAGON.halfcycles_per_frame;
    }

    fn handle_bus(&mut self) {
        // Beta disk watches every M1 fetch for its `$3Dxx` magic-page trap.
        if self.z80.m1 && self.z80.mreq && self.z80.rd {
            self.beta.on_m1(self.z80.addr);
        }

        if self.z80.mreq && self.z80.rd {
            // While TR-DOS is paged in, ROM reads come from the Beta ROM.
            if self.beta.trdos_paged && self.z80.addr < 0x4000 {
                self.z80.data_in = self.memory.read_trdos_rom(self.z80.addr);
            } else {
                self.z80.data_in = self.memory.read(self.z80.addr);
            }
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
        if self.beta.claims_port(port) {
            return self.beta.read(port);
        }
        if port & 0x0001 == 0 {
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
            0xFF
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        if self.beta.claims_port(port) {
            self.beta.write(port, data);
            return;
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
        if port & 0x8002 == 0x0000 {
            self.memory.write_7ffd(data);
        }
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

impl Default for Pentagon128 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let m = Pentagon128::new();
        assert_eq!(m.model_id(), "pentagon-128");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = Pentagon128::new();
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn advance_tstates_tracks_position() {
        let mut m = Pentagon128::new();
        m.advance_tstates(7);
        // 7 T-states × 4 hc/T = 28 half-cycles
        assert_eq!(m.hc, 28);
    }
}
