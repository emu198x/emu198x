//! Timex TC2048 — 48K Spectrum-compatible with SCLD video modes.
//!
//! Source references:
//! - `knowledge/systems/spectrum/variants.md`
//! - Adapted from `../Emu198x-Older/crates/machine-timex-tc2048/src/lib.rs`
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

use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::apply_z80_registers;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH_HIRES, TIMING_48K};
use common_sinclair_zx_spectrum::ula::Ula;
use format_sinclair_zx_spectrum_snapshot::Snapshot;
use peripheral_kempston_joystick::KempstonJoystick;
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
    /// Kempston Interface joystick. Defaults to unattached.
    pub kempston: KempstonJoystick,
    pub tape: TapePlayer,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,

    pub(crate) hc: u32,
    speaker: SpeakerMixer,
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
            kempston: KempstonJoystick::new(),
            tape: TapePlayer::new(),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_48K.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; samples_per_frame],
            hc: 0,
            speaker: SpeakerMixer::default(),
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

    pub fn tape_stop(&mut self) {
        self.tape.stop();
    }

    /// Reset the CPU, timing, and audio state. Keeps ROM and RAM intact.
    pub fn reset(&mut self) {
        self.z80 = Z80::new();
        self.hc = 0;
        self.speaker = SpeakerMixer::default();
    }

    /// Apply a parsed `.z80` snapshot. TC2048 shares the 48K's flat
    /// 48K memory layout, so the page-to-base mapping is the standard
    /// 48K convention: 4 → $8000, 5 → $C000, 8 → $4000.
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
        // TC2048 uses full low-byte I/O decoding (exact match).
        match port & 0xFF {
            0xFE => {
                let mut val = self.ula.read_fe(port, &self.keyboard);
                if self.tape.is_playing() {
                    val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
                }
                val
            }
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
                    let tstate = self.hc / 4;
                    self.audio.set_level(tstate, self.speaker.level());
                }
            }
            0xFF => self.ula.write_ff(data),
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

impl Default for TimexTC2048 {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumDriver for TimexTC2048 {
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
        TIMING_48K.halfcycles_per_frame
    }
    #[inline(always)]
    fn halfcycles_per_tstate(&self) -> u32 {
        TIMING_48K.cpu_divisor
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
    fn on_tstate(&mut self, _hc: u32) {
        self.tape.advance_tstates(1);
        let ear = self.tape.ear_level();
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
            let tstate = self.hc / 4;
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

    #[test]
    fn audio_controls_passthrough_round_trips() {
        use common_sinclair_zx_spectrum::audio::SpeakerChannel;
        let mut m = TimexTC2048::new();
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
