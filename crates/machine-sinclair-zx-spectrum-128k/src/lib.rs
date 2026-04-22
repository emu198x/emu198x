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
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::snapshot::{
    apply_128k_bank_pages, apply_ay_registers, apply_z80_registers, Z80Snapshot,
};
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

/// 128K-family machine.
///
/// The Sinclair 128K ("toastrack") and the Sinclair-branded Amstrad-built
/// +2 (1986, grey case, built-in tape deck) share the same chip set and
/// ULA, so one implementation covers both. The runtime-level profile ID
/// distinguishes them for snapshot compatibility.
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

    pub(crate) hc: u32,
    beeper_state: bool,
    last_ear: bool,
}

impl Spectrum128K {
    #[must_use]
    pub fn new() -> Self {
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
            hc: 0,
            beeper_state: false,
            last_ear: false,
        }
    }

    /// Stable hardware identifier. Use the runtime-level profile ID to
    /// distinguish the toastrack 128K from the Amstrad-built +2 —
    /// they're the same chip set.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        "sinclair-zx-spectrum-128k"
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

    pub fn tape_stop(&mut self) {
        self.tape.stop();
    }

    /// Reset the machine's CPU and audio/timing state, keeping the
    /// loaded ROMs and RAM contents intact (matching real-hardware
    /// power-cycle semantics).
    pub fn reset(&mut self) {
        self.z80 = Z80::new();
        self.hc = 0;
        self.beeper_state = false;
        self.last_ear = false;
    }

    /// Apply a parsed `.z80` snapshot. The 128K-family snapshot layout
    /// pages banks through `$7FFD`; the AY register file is replayed
    /// from the snapshot-captured values.
    pub fn apply_snapshot(&mut self, snap: &Z80Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
        self.ula.write_fe(snap.border);
        apply_128k_bank_pages(snap, &mut self.memory);
        self.memory.write_7ffd(snap.port_7ffd);
        apply_ay_registers(snap, &mut self.ay);
    }

    /// Run exactly one PAL frame. Delegates to `SpectrumDriver::run_frame`.
    pub fn run_frame(&mut self) {
        <Self as SpectrumDriver>::run_frame(self);
    }

    /// Advance the machine by an exact number of master-clock
    /// half-cycles, handling frame wrap when `hc` crosses the frame
    /// boundary.
    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        let frame_hc = TIMING_128K.halfcycles_per_frame;
        for _ in 0..halfcycles {
            self.tick_one_halfcycle();
            if self.hc >= frame_hc {
                self.end_frame_ula();
                self.on_end_frame();
                self.hc -= frame_hc;
            }
        }
    }

    /// Advance the machine by an exact number of CPU T-states.
    pub fn advance_tstates(&mut self, tstates: u32) {
        self.advance_halfcycles(TIMING_128K.tstates_to_hc(tstates));
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

impl SpectrumDriver for Spectrum128K {
    #[inline(always)]
    fn hc(&self) -> u32 {
        self.hc
    }
    #[inline(always)]
    fn hc_mut(&mut self) -> &mut u32 {
        &mut self.hc
    }
    #[inline(always)]
    fn frame_hc(&self) -> u32 {
        TIMING_128K.halfcycles_per_frame
    }

    #[inline(always)]
    fn tick_ula(&mut self) {
        self.ula.tick(
            &self.memory,
            self.z80.addr,
            self.z80.mreq,
            self.z80.iorq,
            &mut self.framebuffer,
        );
    }

    #[inline(always)]
    fn cpu_clock_active(&self) -> bool {
        self.ula.cpu_clock_active()
    }

    #[inline(always)]
    fn tick_cpu_and_bus(&mut self) {
        self.z80.tick();
        self.handle_bus();
    }

    #[inline(always)]
    fn feed_irq(&mut self) {
        self.z80.irq = self.ula.interrupt_active();
    }

    #[inline(always)]
    fn on_tstate(&mut self, hc: u32) {
        self.tape.advance_tstates(1);
        if hc % 8 == 2 {
            self.ay.tick();
        }
        let ear = self.tape.ear_level();
        if ear != self.last_ear {
            self.last_ear = ear;
            let tstate = hc / 4;
            self.audio.set_level(tstate, self.speaker_level());
        }
    }

    #[inline(always)]
    fn end_frame_ula(&mut self) {
        self.ula.end_frame();
    }

    #[inline(always)]
    fn on_end_frame(&mut self) {
        self.audio.end_frame(&mut self.audio_frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let m = Spectrum128K::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-128k");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
        assert_eq!(m.kempston, 0);
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
