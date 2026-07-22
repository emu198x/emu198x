//! Timex TC2068 (PAL) / TS2068 (NTSC) machine.
//!
//! Source references:
//! - `knowledge/systems/spectrum/variants.md`
//! - Adapted from `../Emu198x-Older/crates/machine-timex-ts2068/src/lib.rs`
//!
//! Hardware:
//! - Z80 — TC2068: 3.5 MHz / TS2068: 3.528 MHz (master / 4)
//! - Timex SCLD with PAL or NTSC config
//! - 16 KB ROM + 48 KB RAM with DOCK / EXROM paging via port `$F4`
//! - General Instrument AY-3-8912 PSG on ports `$F5` (select / read)
//!   and `$F6` (data write) — NOT the standard 128K `$FFFD` / `$BFFD`
//! - Full I/O decoding (exact low-byte match)
//!
//! `TimexModel::TS2068` selects 14.112 MHz NTSC timing with 262 lines.
//! `TimexModel::TC2068` reuses the standard 48K PAL timing.

pub mod memory;

use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::apply_z80_registers;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::tape_recorder::TapeRecorder;
use common_sinclair_zx_spectrum::timing::{
    FrameTiming, SCREEN_HEIGHT, SCREEN_WIDTH_HIRES, TIMING_48K,
};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine;
use format_sinclair_zx_spectrum_snapshot::Snapshot;
use gi_ay_3_8912::Ay3_8912;
use peripheral_kempston_joystick::KempstonJoystick;
use timex_scld::TimexScld;
use zilog_z80::Z80;

use crate::memory::MemoryTimex;

const AUDIO_SAMPLE_RATE: u32 = 44_100;

/// TC2068 (PAL) or TS2068 (NTSC).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TimexModel {
    /// PAL Timex (sold in Portugal as the TC2068, Poland as the Unipolbrit
    /// Komputer 2086): 14 MHz crystal, 312 lines.
    TC2068,
    /// NTSC Timex (the US TS2068): 14.112 MHz crystal, 262 lines.
    TS2068,
}

/// TS2068 NTSC frame timing. The PAL TC2068 reuses `TIMING_48K`.
pub const TIMING_TS2068: FrameTiming = FrameTiming {
    master_hz: 14_112_000,
    cpu_divisor: 4,
    tstates_per_line: 224,
    halfcycles_per_line: 224 * 4,
    lines_per_frame: 262,
    halfcycles_per_frame: 224 * 4 * 262,
    tstates_per_frame: 224 * 262,
    first_border_line: 8,
    first_screen_line: 35,
    last_screen_line: 227,
    last_border_line: 254,
    first_screen_tstate: 24,
    screen_pixels_per_line: 256,
    left_border_tstate: 0,
    right_border_tstate: 176,
    contention_start_tstate: 14_336,
    contention_pattern: [6, 5, 4, 3, 2, 1, 0, 0],
    contention_phase: 0,
    contention_tstates_per_line: 128,
    interrupt_start_tstate: 0,
    interrupt_length_tstates: 32,
};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct TimexTS2068 {
    pub z80: Z80,
    pub ula: TimexScld,
    pub memory: MemoryTimex,
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
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,
    pub model: TimexModel,

    pub(crate) hc: u32,
    speaker: SpeakerMixer,
}

impl TimexTS2068 {
    #[must_use]
    pub fn new(model: TimexModel) -> Self {
        let (timing, config) = match model {
            TimexModel::TC2068 => (&TIMING_48K, &ula_engine::CONFIG_48K),
            TimexModel::TS2068 => (&TIMING_TS2068, &ula_engine::CONFIG_TS2068),
        };
        let cpu_hz = (timing.master_hz / u64::from(timing.cpu_divisor)) as u32;
        // The Timex divides its AY input by 8 (not 2 like the 128K).
        let ay_hz = cpu_hz / 8;
        let samples_per_frame = (AUDIO_SAMPLE_RATE / 50) as usize;

        Self {
            z80: Z80::new(),
            ula: TimexScld::with_config(config),
            memory: MemoryTimex::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH_HIRES * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: KempstonJoystick::new(),
            tape: TapePlayer::new(),
            recorder: TapeRecorder::new(),
            ay: Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, samples_per_frame),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, timing.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; samples_per_frame],
            model,
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

    /// Returns the static frame-timing descriptor for this variant.
    #[must_use]
    pub fn timing(&self) -> &'static FrameTiming {
        match self.model {
            TimexModel::TC2068 => &TIMING_48K,
            TimexModel::TS2068 => &TIMING_TS2068,
        }
    }

    #[must_use]
    pub fn model_id(&self) -> &'static str {
        match self.model {
            TimexModel::TC2068 => "timex-tc2068",
            TimexModel::TS2068 => "timex-ts2068",
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

    pub fn tape_stop(&mut self) {
        self.tape.stop();
    }

    /// Reset the CPU, timing, and audio state. Keeps ROM and RAM intact.
    pub fn reset(&mut self) {
        self.z80 = Z80::new();
        self.hc = 0;
        self.speaker = SpeakerMixer::default();
    }

    /// Reattach `&'static` references that don't survive serde's
    /// `#[serde(skip)]` round-trip, and rehydrate the Z80 walker
    /// sequence. Call once after restoring a postcard snapshot — the
    /// runtime wires this through `after_restore`. The NTSC TS2068 uses
    /// `CONFIG_TS2068` (262 lines) and the PAL TC2068 the 48K config;
    /// the model picks the config exactly as `new` does, so a restore
    /// doesn't fall back to 48K timing on an NTSC machine.
    pub fn restore_volatile_refs(&mut self) {
        let config = match self.model {
            TimexModel::TC2068 => &ula_engine::CONFIG_48K,
            TimexModel::TS2068 => &ula_engine::CONFIG_TS2068,
        };
        self.z80.rehydrate_walker_sequence();
        self.ula.reattach_config(config);
    }

    /// Apply a parsed `.z80` snapshot. Treats the Timex as a stock 48K
    /// for snapshot purposes — the page-to-base map matches the 48K
    /// convention. AY state is not carried in `.z80` v2/v3 for Timex.
    pub fn apply_snapshot(&mut self, snap: &Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
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
        <Self as SpectrumDriver>::run_frame(self);
    }

    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        <Self as SpectrumDriver>::advance_halfcycles(self, halfcycles);
    }

    pub fn advance_tstates(&mut self, tstates: u32) {
        <Self as SpectrumDriver>::advance_tstates(self, tstates);
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
        // Kempston is a separate add-on board with its own (partial)
        // decoding — the SCLD's full decoding doesn't constrain it.
        if self.kempston.claims_port(port) {
            return self.kempston.read(port);
        }
        // Timex uses full low-byte I/O decoding.
        match port & 0xFF {
            0xFE => {
                let mut val = self.ula.read_fe(port, &self.keyboard);
                if self.tape.is_playing() {
                    val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
                }
                val
            }
            0xF4 => self.memory.read_f4(),
            0xF5 => self.ay.read_data(),
            0xFF => self.ula.read_ff(),
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        match port & 0xFF {
            0xFE => {
                self.ula.write_fe(data);
                let beeper = data & 0x10 != 0;
                if beeper != self.speaker.beeper {
                    self.speaker.beeper = beeper;
                    let tstate = common_sinclair_zx_spectrum::timing::FramePosition::new(
                        self.hc,
                        self.timing(),
                    )
                    .tstate(self.timing());
                    self.audio.set_level(tstate, self.speaker.level());
                }
                // MIC (bit 3) carries the tape SAVE signal.
                self.recorder.set_mic_level(data & 0x08 != 0);
            }
            0xF4 => self.memory.write_f4(data),
            0xF5 => self.ay.select_register(data),
            0xF6 => self.ay.write_data(data),
            0xFF => {
                self.ula.write_ff(data);
                self.memory.set_exrom_enabled(data & 0x80 != 0);
            }
            _ => {}
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

impl SpectrumDriver for TimexTS2068 {
    fn frame_timing(&self) -> &FrameTiming {
        self.timing()
    }
    #[inline(always)]
    fn hc(&self) -> u32 {
        self.hc
    }
    #[inline(always)]
    fn hc_mut(&mut self) -> &mut u32 {
        &mut self.hc
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
    fn on_tstate(&mut self, position: common_sinclair_zx_spectrum::timing::FramePosition) {
        self.tape.advance_tstates(1);
        self.recorder.advance(1);
        if position.halfcycles() % 8 == 2 {
            self.ay.tick();
        }
        let ear = self.tape.ear_level();
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
            let tstate = position.tstate(self.timing());
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
    fn pal_variant_uses_48k_timing() {
        let m = TimexTS2068::new(TimexModel::TC2068);
        assert_eq!(m.timing().master_hz, 14_000_000);
        assert_eq!(m.timing().lines_per_frame, 312);
        assert_eq!(m.model_id(), "timex-tc2068");
    }

    #[test]
    fn ntsc_variant_uses_ts2068_timing() {
        let m = TimexTS2068::new(TimexModel::TS2068);
        assert_eq!(m.timing().master_hz, 14_112_000);
        assert_eq!(m.timing().lines_per_frame, 262);
        assert_eq!(m.model_id(), "timex-ts2068");
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = TimexTS2068::new(TimexModel::TS2068);
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn write_to_port_f5_then_read_via_f5_round_trips_ay_register() {
        let mut m = TimexTS2068::new(TimexModel::TS2068);
        // AY register select on port $F5, data write on $F6.
        m.io_write(0x00F5, 0x08); // select Vol A
        m.io_write(0x00F6, 0x0A); // write
        assert_eq!(m.io_read(0x00F5), 0x0A);
    }

    #[test]
    fn audio_controls_passthrough_round_trips() {
        use common_sinclair_zx_spectrum::audio::SpeakerChannel;
        let mut m = TimexTS2068::new(TimexModel::TS2068);
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
