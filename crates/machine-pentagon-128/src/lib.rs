//! Pentagon 128 machine.
//!
//! Source references:
//! - `knowledge/systems/spectrum/variants.md`
//! - Adapted from `../Emu198x-Older/crates/machine-pentagon-128/src/lib.rs`
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
use common_sinclair_zx_spectrum::SpectrumTapePlayer;
use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::{
    Snapshot, apply_128k_bank_pages, apply_ay_registers, apply_z80_registers,
};
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::tape_recorder::TapeRecorder;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_PENTAGON};
use common_sinclair_zx_spectrum::ula::Ula;
use gi_ay_3_8912::Ay3_8912;
use pentagon_ula::PentagonUla;
use peripheral_kempston_joystick::KempstonJoystick;
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
            kempston: KempstonJoystick::new(),
            tape: TapePlayer::new(),
            recorder: TapeRecorder::new(),
            ay: Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME),
            beta: BetaDisk::new(),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_PENTAGON.tstates_per_frame, cpu_hz),
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
    /// runtime wires this through `after_restore`. Without the ULA
    /// reattach the Pentagon's config falls back to 48K timing.
    pub fn restore_volatile_refs(&mut self) {
        self.z80.rehydrate_walker_sequence();
        self.ula.reattach_config();
    }

    /// Apply a parsed `.z80` snapshot. Pentagon shares the 128K page
    /// layout (8 banked RAM pages, `$7FFD` paging, AY register file) —
    /// it has no `$1FFD`.
    pub fn apply_snapshot(&mut self, snap: &Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
        self.ula.write_fe(snap.border);
        apply_128k_bank_pages(snap, &mut self.memory);
        self.memory.write_7ffd(snap.port_7ffd);
        apply_ay_registers(snap, &mut self.ay);
    }

    /// Run exactly one PAL frame.
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
                let tstate = common_sinclair_zx_spectrum::timing::FramePosition::new(
                    self.hc,
                    &TIMING_PENTAGON,
                )
                .tstate(&TIMING_PENTAGON);
                self.audio.set_level(tstate, self.speaker.level());
            }
            // MIC (bit 3) carries the tape SAVE signal.
            self.recorder.set_mic_level(data & 0x08 != 0);
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

    /// Bus-level port read. Mirrors what an `IN A,(C)` would observe
    /// but without driving the CPU through the synthetic instruction.
    pub fn port_read(&mut self, port: u16) -> u8 {
        self.io_read(port)
    }

    /// Bus-level port write. Equivalent in effect to `OUT (C),A`
    /// without driving the CPU through the synthetic instruction.
    pub fn port_write(&mut self, port: u16, value: u8) {
        self.io_write(port, value);
    }
}

impl Default for Pentagon128 {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumDriver for Pentagon128 {
    fn frame_timing(&self) -> &common_sinclair_zx_spectrum::timing::FrameTiming {
        &TIMING_PENTAGON
    }
    #[inline(always)]
    fn hc(&self) -> u32 {
        self.hc
    }
    #[inline(always)]
    fn hc_mut(&mut self) -> &mut u32 {
        &mut self.hc
    }
    /// Pentagon has no memory contention — CPU ticks every even half-cycle.
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
    fn on_tstate(&mut self, position: common_sinclair_zx_spectrum::timing::FramePosition) {
        self.tape.advance_tstates(1);
        self.recorder.advance(1);
        if position.halfcycles() % 8 == 2 {
            self.ay.tick();
        }
        let ear = self.tape.ear_level();
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
            let tstate = position.tstate(&TIMING_PENTAGON);
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

    #[test]
    fn audio_controls_passthrough_round_trips() {
        use common_sinclair_zx_spectrum::audio::SpeakerChannel;
        let mut m = Pentagon128::new();
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
