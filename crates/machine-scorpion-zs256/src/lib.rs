//! Scorpion ZS-256 machine.
//!
//! Source references:
//! - `knowledge/systems/spectrum/variants.md`
//! - Adapted from `../Emu198x-Older/crates/machine-scorpion-zs256/src/lib.rs`
//!
//! Hardware:
//! - Z80 @ 3.5 MHz (master / 4) — same crystal as the 48K
//! - Scorpion ULA — no contention, 48K-style geometry
//! - 256 KB RAM in 16 × 16 KB banks (paged via `$7FFD` + `$1FFD`)
//! - 4 × 16 KB ROMs (Service / TR-DOS / 128 editor / 48 BASIC)
//! - General Instrument AY-3-8912 PSG
//! - Beta 128 disk interface

pub mod memory;

use beta_disk_interface::BetaDisk;
use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::{
    Snapshot, apply_128k_bank_pages, apply_ay_registers, apply_z80_registers,
};
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::tape_recorder::TapeRecorder;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_SCORPION};
use common_sinclair_zx_spectrum::ula::Ula;
use gi_ay_3_8912::Ay3_8912;
use peripheral_kempston_joystick::KempstonJoystick;
use scorpion_ula::ScorpionUla;
use zilog_z80::Z80;

use crate::memory::MemoryScorpion;

const AUDIO_SAMPLE_RATE: u32 = 44_100;
const AUDIO_SAMPLES_PER_FRAME: usize = 882;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ScorpionZS256 {
    pub z80: Z80,
    pub ula: ScorpionUla,
    pub memory: MemoryScorpion,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    /// Kempston Interface joystick. Defaults to unattached.
    pub kempston: KempstonJoystick,
    pub tape: TapePlayer,
    /// Captures the MIC line during a SAVE for tape write-back (mirrors the 48K
    /// class). `#[serde(default)]` keeps pre-SAVE snapshots loadable.
    #[serde(default)]
    recorder: TapeRecorder,
    pub ay: Ay3_8912,
    pub beta: BetaDisk,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,

    pub(crate) hc: u32,
    speaker: SpeakerMixer,
}

impl ScorpionZS256 {
    #[must_use]
    pub fn new() -> Self {
        let cpu_hz = (TIMING_SCORPION.master_hz / u64::from(TIMING_SCORPION.cpu_divisor)) as u32;
        let ay_hz = cpu_hz / 2;
        Self {
            z80: Z80::new(),
            ula: ScorpionUla::new(),
            memory: MemoryScorpion::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: KempstonJoystick::new(),
            tape: TapePlayer::new(),
            recorder: TapeRecorder::new(),
            ay: Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME),
            beta: BetaDisk::new(),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_SCORPION.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
            hc: 0,
            speaker: SpeakerMixer::default(),
        }
    }

    /// Decodes any captured tape `SAVE` signal into standard-speed blocks.
    #[must_use]
    pub fn recorded_tape_blocks(&self) -> Vec<TapeBlock> {
        self.recorder.decode()
    }

    /// Discards captured `SAVE` signal (e.g. after flushing it to a file).
    pub fn clear_tape_recording(&mut self) {
        self.recorder.clear();
    }

    #[must_use]
    pub fn model_id(&self) -> &'static str {
        "scorpion-zs256"
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

    /// Reset the CPU, timing, and audio state. Keeps ROMs and RAM intact.
    pub fn reset(&mut self) {
        self.z80 = Z80::new();
        self.hc = 0;
        self.speaker = SpeakerMixer::default();
    }

    /// Reattach `&'static` references that don't survive serde's
    /// `#[serde(skip)]` round-trip, and rehydrate the Z80 walker
    /// sequence. Call once after restoring a postcard snapshot — the
    /// runtime wires this through `after_restore`. The Scorpion shares
    /// 48K timing, so the reattach is a structural mirror, but it keeps
    /// every variant on the same explicit-reattach contract.
    pub fn restore_volatile_refs(&mut self) {
        self.z80.rehydrate_walker_sequence();
        self.ula.reattach_config();
    }

    /// Apply a parsed `.z80` snapshot. Scorpion uses 128K-style page-to-bank
    /// routing; only the first 8 banks are addressable through a snapshot.
    pub fn apply_snapshot(&mut self, snap: &Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
        self.ula.write_fe(snap.border);
        apply_128k_bank_pages(snap, &mut self.memory);
        self.memory.write_7ffd(snap.port_7ffd);
        apply_ay_registers(snap, &mut self.ay);
    }

    pub fn run_frame(&mut self) {
        <Self as SpectrumDriver>::run_frame(self);
    }

    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        <Self as SpectrumDriver>::advance_halfcycles(self, halfcycles);
    }

    pub fn advance_tstates(&mut self, tstates: u32) {
        <Self as SpectrumDriver>::advance_tstates(self, tstates);
    }

    fn handle_bus(&mut self) {
        if self.z80.m1 && self.z80.mreq && self.z80.rd {
            self.beta.on_m1(self.z80.addr);
        }

        if self.z80.mreq && self.z80.rd {
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
        if self.kempston.claims_port(port) {
            return self.kempston.read(port);
        }
        if port & 0x0001 == 0 {
            let mut val = self.ula.read_fe(port, &self.keyboard);
            if self.tape.is_playing() {
                val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
            }
            val
        } else if port & 0xC002 == 0xC000 {
            self.ay.read_data()
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
            if beeper != self.speaker.beeper {
                self.speaker.beeper = beeper;
                let tstate = self.hc / 4;
                self.audio.set_level(tstate, self.speaker.level());
            }
            // MIC (bit 3) carries the tape SAVE signal.
            self.recorder.set_mic_level(data & 0x08 != 0);
        }
        if port & 0x8002 == 0x0000 {
            self.memory.write_7ffd(data);
        }
        if port & 0xF002 == 0x1000 {
            self.memory.write_1ffd(data);
        }
        if port & 0xC002 == 0xC000 {
            self.ay.select_register(data);
        } else if port & 0xC002 == 0x8000 {
            self.ay.write_data(data);
        }
    }

    pub fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }

    /// Current host-side speaker audio controls.
    #[must_use]
    pub fn audio_controls(&self) -> common_sinclair_zx_spectrum::audio::AudioControls {
        self.audio.audio_controls()
    }

    /// Replaces the host-side speaker audio controls wholesale.
    pub fn set_audio_controls(
        &mut self,
        controls: common_sinclair_zx_spectrum::audio::AudioControls,
    ) {
        self.audio.set_audio_controls(controls);
    }

    /// Enables or disables one host-side audio channel.
    pub fn set_audio_channel_enabled(
        &mut self,
        channel: common_sinclair_zx_spectrum::audio::SpeakerChannel,
        enabled: bool,
    ) {
        self.audio.set_audio_channel_enabled(channel, enabled);
    }

    /// Sets the host-side gain for one audio channel.
    pub fn set_audio_channel_gain(
        &mut self,
        channel: common_sinclair_zx_spectrum::audio::SpeakerChannel,
        gain: f32,
    ) {
        self.audio.set_audio_channel_gain(channel, gain);
    }

    /// Bus-level port read.
    pub fn port_read(&mut self, port: u16) -> u8 {
        self.io_read(port)
    }

    /// Bus-level port write.
    pub fn port_write(&mut self, port: u16, value: u8) {
        self.io_write(port, value);
    }
}

impl Default for ScorpionZS256 {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumDriver for ScorpionZS256 {
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
        TIMING_SCORPION.halfcycles_per_frame
    }
    #[inline(always)]
    fn halfcycles_per_tstate(&self) -> u32 {
        TIMING_SCORPION.cpu_divisor
    }

    /// Scorpion has no memory contention.
    #[inline(always)]
    fn contended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn tick_ula(&mut self) {
        self.ula.tick(
            &self.memory,
            self.z80.addr,
            self.z80.mreq,
            self.z80.iorq,
            self.z80.rfsh,
            &mut self.framebuffer,
        );
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
        self.recorder.advance(1);
        if hc % 8 == 2 {
            self.ay.tick();
        }
        let ear = self.tape.ear_level();
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
            let tstate = hc / 4;
            self.audio.set_level(tstate, self.speaker.level());
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
        let m = ScorpionZS256::new();
        assert_eq!(m.model_id(), "scorpion-zs256");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = ScorpionZS256::new();
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn audio_controls_passthrough_round_trips() {
        use common_sinclair_zx_spectrum::audio::SpeakerChannel;
        let mut m = ScorpionZS256::new();
        let initial = m.audio_controls();
        assert!(initial.channel(SpeakerChannel::Speaker).enabled());

        m.set_audio_channel_enabled(SpeakerChannel::Speaker, false);
        m.set_audio_channel_gain(SpeakerChannel::Speaker, 0.25);
        let after = m.audio_controls();
        assert!(!after.channel(SpeakerChannel::Speaker).enabled());
        assert!((after.channel(SpeakerChannel::Speaker).gain() - 0.25).abs() < f32::EPSILON);

        m.set_audio_controls(initial);
        assert!(
            m.audio_controls()
                .channel(SpeakerChannel::Speaker)
                .enabled()
        );
    }
}
