//! Timex TC2048 — 48K Spectrum-compatible with SCLD video modes.
//!
//! Source references:
//! - `wiki/systems/spectrum/variants.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/machine-timex-tc2048/src/lib.rs`
//!
//! Hardware:
//! - Z80 @ 3.5 MHz (master / 4) — same as the 48K Ferranti
//! - Timex SCLD (Semi-Custom Logic Device) — same contention as the
//!   48K Ferranti, plus port `$FF` for video mode + interrupt-disable
//! - 16 KB ROM + 48 KB RAM (flat 48K-style memory map)
//! - **No AY chip** — that's TC2068 / TS2068
//! - **Full I/O decoding** (exact low-byte match), unlike the partial
//!   decoding of the stock 48K
//!
//! Hi-res framebuffer width is 704 (vs the standard Spectrum's 352)
//! because the SCLD can output a 512×192 monochrome mode.

pub mod memory;

use common_sinclair_zx_spectrum::audio::BeeperAudio;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH_HIRES, TIMING_48K};
use common_sinclair_zx_spectrum::ula::Ula;
use format_sinclair_zx_spectrum_z80::Z80Snapshot;
use timex_scld::TimexScld;
use zilog_z80::Z80;

use crate::memory::MemoryTC2048;

const AUDIO_SAMPLE_RATE: u32 = 44_100;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct TimexTC2048 {
    pub z80: Z80,
    pub ula: TimexScld,
    pub memory: MemoryTC2048,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    pub kempston: u8,
    pub tape: TapePlayer,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,

    pub(crate) hc: u32,
    beeper_state: bool,
    last_ear: bool,
}

impl TimexTC2048 {
    #[must_use]
    pub fn new() -> Self {
        let cpu_hz = (TIMING_48K.master_hz / u64::from(TIMING_48K.cpu_divisor)) as u32;
        let samples_per_frame = (AUDIO_SAMPLE_RATE / 50) as usize;
        Self {
            z80: Z80::new(),
            ula: TimexScld::new(),
            memory: MemoryTC2048::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH_HIRES * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: 0,
            tape: TapePlayer::new(),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_48K.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; samples_per_frame],
            hc: 0,
            beeper_state: false,
            last_ear: false,
        }
    }

    #[must_use]
    pub fn model_id(&self) -> &'static str {
        "timex-tc2048"
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

    /// Apply a parsed `.z80` snapshot. TC2048 shares the 48K's flat
    /// 48K memory layout, so the page-to-base mapping is the standard
    /// 48K convention: 4 → $8000, 5 → $C000, 8 → $4000.
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
            let base: u16 = match *page {
                4 => 0x8000,
                5 => 0xC000,
                8 => 0x4000,
                _ => continue,
            };
            for (i, &byte) in data.iter().enumerate() {
                self.memory.write(base.wrapping_add(i as u16), byte);
            }
        }
    }

    pub fn run_frame(&mut self) {
        while self.hc < TIMING_48K.halfcycles_per_frame {
            self.tick_halfcycle();
        }
        self.end_frame();
    }

    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        for _ in 0..halfcycles {
            self.tick_halfcycle();
            if self.hc >= TIMING_48K.halfcycles_per_frame {
                self.end_frame();
            }
        }
    }

    pub fn advance_tstates(&mut self, tstates: u32) {
        self.advance_halfcycles(TIMING_48K.tstates_to_hc(tstates));
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
        self.hc -= TIMING_48K.halfcycles_per_frame;
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
        // TC2048 uses full low-byte I/O decoding (exact match).
        match port & 0xFF {
            0xFE => {
                let mut val = self.ula.read_fe(port, &self.keyboard);
                if self.tape.is_playing() {
                    val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
                }
                val
            }
            0x1F => self.kempston,
            0xFF => self.ula.read_ff(),
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        match port & 0xFF {
            0xFE => {
                self.ula.write_fe(data);
                let beeper = data & 0x10 != 0;
                if beeper != self.beeper_state {
                    self.beeper_state = beeper;
                    let tstate = self.hc / 4;
                    self.audio.set_level(tstate, self.speaker_level());
                }
            }
            0xFF => self.ula.write_ff(data),
            _ => {}
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

impl Default for TimexTC2048 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let m = TimexTC2048::new();
        assert_eq!(m.model_id(), "timex-tc2048");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH_HIRES * SCREEN_HEIGHT);
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = TimexTC2048::new();
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn write_to_port_ff_updates_video_mode() {
        let mut m = TimexTC2048::new();
        m.io_write(0x00FF, 0x02); // hi-colour
        assert_eq!(m.ula.video_mode(), 2);
    }
}
