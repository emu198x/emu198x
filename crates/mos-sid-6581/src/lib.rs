//! MOS 6581 / 8580 SID (Sound Interface Device).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

mod combined_wave_tables;
mod dac;
mod envelope;
mod external_filter;
mod filter;
mod filter_tables;
#[cfg(test)]
mod oracle_tests;
mod voice;

pub use envelope::{Envelope, Phase};
pub use external_filter::ExternalFilter;
pub use filter::Filter;
pub use voice::Voice;

use serde::{Deserialize, Serialize};

/// Audio routing version. Bumped when the audio path through this crate
/// (voice mix, filter routing, envelope shaping, `$D418` master volume,
/// SID → audio_frame routing) changes in a way that invalidates
/// previously-captured audio hashes in the C64 catalogue. The catalogue
/// manifest carries the version each hash was captured against; a
/// mismatch fails loud with a re-capture instruction.
///
/// **Version 1** (2026-05-20): three-voice mixer with per-voice
/// envelope + waveform generation, state-variable filter, master
/// volume nibble from `$D418` bits 0-3. Host-side channel gating
/// applied after the silicon mix.
///
/// **Version 2** (2026-07-03, issue #64): voices pass through the reSID
/// R-2R DAC model — the 6581's nonlinear waveform/envelope DACs and its
/// `wave_zero` DC offset (near-ideal on the 8580) — and the whole mix is
/// AC-coupled by an output high-pass. This replaces the linear
/// `waveform − 2048` / `envelope ÷ 255` and folds the old split volume-DC
/// digi into the one output high-pass. Every sample changes, so v1 hashes
/// are invalid.
///
/// **Version 3** (2026-07-03, issues #19/#20): the reSID op-amp filter model
/// (`filter8580new`) replaces the piecewise-linear state-variable filter for
/// **both** models — measured op-amp transfer curves, Newton-Raphson-solved
/// summer/mixer/resonance/volume ladders, EKV-modelled 6581 cutoff VCRs, and
/// the 8580's parallel-NMOS cutoff DAC. Routing, `voice3off`, and the master
/// volume now live in the filter/mixer stage (`$D417`/`$D418` semantics), and
/// the ad-hoc output high-pass is replaced by reSID's external filter (the
/// C64 board's 16 kHz low-pass + 16 Hz high-pass). Every sample changes, so
/// v2 hashes are invalid.
///
/// **Version 4** (2026-07-06, issue #763): the output decimator now carries
/// its Bresenham remainder, so the stream is emitted at exactly
/// `output_sample_rate` instead of `cpu_freq / ceil(cpu_freq/rate)` (PAL:
/// 46,916 Hz for a 48,000 Hz stream, ~2.3% sharp). The sample count per
/// capture window changes, so v3 hashes are invalid.
///
/// See `knowledge/decisions/c64-architecture-review.md` Seam 4 for
/// the re-capture discipline this constant enforces.
pub const AUDIO_ROUTING_VERSION: u32 = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidModel {
    #[default]
    Mos6581,
    Mos8580,
}

impl SidModel {
    /// Dense index for per-model lookup tables (matches reSID's `model_dac`).
    const fn index(self) -> usize {
        match self {
            Self::Mos6581 => 0,
            Self::Mos8580 => 1,
        }
    }
}

/// Host-side SID voice identifier.
///
/// These controls are outside the emulated SID register surface: muting a
/// voice here does not change gate bits, oscillator state, envelope state, or
/// `$D418`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SidChannel {
    /// SID voice 1.
    Voice1,
    /// SID voice 2.
    Voice2,
    /// SID voice 3.
    Voice3,
}

impl SidChannel {
    const fn index(self) -> usize {
        match self {
            Self::Voice1 => 0,
            Self::Voice2 => 1,
            Self::Voice3 => 2,
        }
    }

    /// Human-readable channel label for frontend status messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Voice1 => "voice 1",
            Self::Voice2 => "voice 2",
            Self::Voice3 => "voice 3",
        }
    }
}

/// Per-voice host mixer control.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelControl {
    enabled: bool,
    gain: f32,
}

impl Default for ChannelControl {
    fn default() -> Self {
        Self {
            enabled: true,
            gain: 1.0,
        }
    }
}

impl ChannelControl {
    /// Whether this voice contributes to host audio output.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Linear voice gain after sanitisation, clamped to 0.0..=1.0.
    #[must_use]
    pub const fn gain(self) -> f32 {
        self.gain
    }

    fn apply(self, sample: f32) -> f32 {
        if self.enabled {
            sample * sanitize_gain(self.gain)
        } else {
            0.0
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn set_gain(&mut self, gain: f32) {
        self.gain = sanitize_gain(gain);
    }
}

/// Host-side audio controls for the SID mixer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioControls {
    master_gain: f32,
    channels: [ChannelControl; 3],
}

impl Default for AudioControls {
    fn default() -> Self {
        Self {
            master_gain: 1.0,
            channels: [ChannelControl::default(); 3],
        }
    }
}

impl AudioControls {
    /// Master gain applied to the SID host output.
    #[must_use]
    pub const fn master_gain(self) -> f32 {
        self.master_gain
    }

    /// Set master gain. Non-finite values become 0.0; finite values clamp to
    /// 0.0..=1.0.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = sanitize_gain(gain);
    }

    /// Return control state for one SID voice.
    #[must_use]
    pub const fn channel(self, channel: SidChannel) -> ChannelControl {
        self.channels[channel.index()]
    }

    /// Enable or disable one SID voice in the host mixer.
    pub fn set_channel_enabled(&mut self, channel: SidChannel, enabled: bool) {
        self.channels[channel.index()].set_enabled(enabled);
    }

    /// Set one SID voice gain. Non-finite values become 0.0; finite values
    /// clamp to 0.0..=1.0.
    pub fn set_channel_gain(&mut self, channel: SidChannel, gain: f32) {
        self.channels[channel.index()].set_gain(gain);
    }

    fn sanitized(mut self) -> Self {
        self.master_gain = sanitize_gain(self.master_gain);
        for channel in &mut self.channels {
            channel.set_gain(channel.gain);
        }
        self
    }
}

const fn default_audio_controls() -> AudioControls {
    AudioControls {
        master_gain: 1.0,
        channels: [ChannelControl {
            enabled: true,
            gain: 1.0,
        }; 3],
    }
}

fn sanitize_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sid6581 {
    pub model: SidModel,
    pub voices: [Voice; 3],
    pub envelopes: [Envelope; 3],
    pub filter: Filter,
    /// Mirror of the `$D418` volume nibble (authoritative copy lives in
    /// [`Filter`], which owns the volume DAC ladder).
    pub volume: u8,
    /// Mirror of the `$D418` voice3off bit (authoritative copy lives in
    /// [`Filter`]; it only silences an *unfiltered* voice 3, per hardware).
    pub voice3_off: bool,
    pub potx: u8,
    pub poty: u8,
    accumulator: f32,
    channel_accumulators: [f32; 3],
    sample_count: u32,
    /// Bresenham decimation accumulator: `+= output_sample_rate` each tick,
    /// emit a sample when it crosses `cpu_freq`, then subtract `cpu_freq` to
    /// carry the remainder. Integer, so the long-run output rate is exactly
    /// `output_sample_rate` with no drift. `#[serde(default)]` — old snapshots
    /// (which stored a float `ticks_per_sample`) restore with a fresh phase.
    #[serde(default)]
    sample_error: u64,
    cpu_freq: u64,
    output_sample_rate: u32,
    #[serde(default = "default_audio_controls")]
    audio_controls: AudioControls,
    /// The C64 board's output coupling (16 kHz low-pass + 16 Hz high-pass).
    #[serde(default)]
    ext_filter: ExternalFilter,
    #[serde(skip)]
    buffer: Vec<f32>,
    #[serde(skip)]
    channel_buffers: [Vec<f32>; 3],
}

impl Sid6581 {
    #[must_use]
    pub fn new(cpu_frequency: u64, output_sample_rate: u32) -> Self {
        Self::new_with_model(cpu_frequency, output_sample_rate, SidModel::Mos6581)
    }

    #[must_use]
    pub fn new_with_model(cpu_frequency: u64, output_sample_rate: u32, model: SidModel) -> Self {
        Self {
            model,
            voices: [Voice::new(), Voice::new(), Voice::new()],
            envelopes: [Envelope::new(), Envelope::new(), Envelope::new()],
            filter: Filter::new(model),
            volume: 0,
            voice3_off: false,
            potx: 0x80,
            poty: 0x80,
            accumulator: 0.0,
            channel_accumulators: [0.0; 3],
            sample_count: 0,
            sample_error: 0,
            cpu_freq: cpu_frequency,
            output_sample_rate,
            audio_controls: AudioControls::default(),
            ext_filter: ExternalFilter::new(),
            buffer: Vec::with_capacity(output_sample_rate as usize / 50 + 1),
            channel_buffers: std::array::from_fn(|_| {
                Vec::with_capacity(output_sample_rate as usize / 50 + 1)
            }),
        }
    }

    /// Current host-side audio controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.audio_controls
    }

    /// Replace all host-side audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.audio_controls = controls.sanitized();
    }

    /// Enable or disable one SID voice in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: SidChannel, enabled: bool) {
        self.audio_controls.set_channel_enabled(channel, enabled);
    }

    /// Set one SID voice's host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: SidChannel, gain: f32) {
        self.audio_controls.set_channel_gain(channel, gain);
    }

    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr & 0x1F {
            0x19 => self.potx,
            0x1A => self.poty,
            0x1B => {
                let ring_src_msb = self.voices[1].msb();
                let waveform = self.voices[2].waveform_output(ring_src_msb, self.model);
                (waveform >> 4) as u8
            }
            0x1C => self.envelopes[2].level,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u8, value: u8) {
        let reg = addr & 0x1F;
        match reg {
            0x00 => {
                self.voices[0].frequency = (self.voices[0].frequency & 0xFF00) | u16::from(value);
            }
            0x01 => {
                self.voices[0].frequency =
                    (self.voices[0].frequency & 0x00FF) | (u16::from(value) << 8);
            }
            0x02 => {
                self.voices[0].pulse_width =
                    (self.voices[0].pulse_width & 0x0F00) | u16::from(value);
            }
            0x03 => {
                self.voices[0].pulse_width =
                    (self.voices[0].pulse_width & 0x00FF) | ((u16::from(value) & 0x0F) << 8);
            }
            0x04 => self.voices[0].control = value,
            0x05 => {
                self.envelopes[0].attack = (value >> 4) & 0x0F;
                self.envelopes[0].decay = value & 0x0F;
            }
            0x06 => {
                self.envelopes[0].sustain = (value >> 4) & 0x0F;
                self.envelopes[0].release = value & 0x0F;
            }
            0x07 => {
                self.voices[1].frequency = (self.voices[1].frequency & 0xFF00) | u16::from(value);
            }
            0x08 => {
                self.voices[1].frequency =
                    (self.voices[1].frequency & 0x00FF) | (u16::from(value) << 8);
            }
            0x09 => {
                self.voices[1].pulse_width =
                    (self.voices[1].pulse_width & 0x0F00) | u16::from(value);
            }
            0x0A => {
                self.voices[1].pulse_width =
                    (self.voices[1].pulse_width & 0x00FF) | ((u16::from(value) & 0x0F) << 8);
            }
            0x0B => self.voices[1].control = value,
            0x0C => {
                self.envelopes[1].attack = (value >> 4) & 0x0F;
                self.envelopes[1].decay = value & 0x0F;
            }
            0x0D => {
                self.envelopes[1].sustain = (value >> 4) & 0x0F;
                self.envelopes[1].release = value & 0x0F;
            }
            0x0E => {
                self.voices[2].frequency = (self.voices[2].frequency & 0xFF00) | u16::from(value);
            }
            0x0F => {
                self.voices[2].frequency =
                    (self.voices[2].frequency & 0x00FF) | (u16::from(value) << 8);
            }
            0x10 => {
                self.voices[2].pulse_width =
                    (self.voices[2].pulse_width & 0x0F00) | u16::from(value);
            }
            0x11 => {
                self.voices[2].pulse_width =
                    (self.voices[2].pulse_width & 0x00FF) | ((u16::from(value) & 0x0F) << 8);
            }
            0x12 => self.voices[2].control = value,
            0x13 => {
                self.envelopes[2].attack = (value >> 4) & 0x0F;
                self.envelopes[2].decay = value & 0x0F;
            }
            0x14 => {
                self.envelopes[2].sustain = (value >> 4) & 0x0F;
                self.envelopes[2].release = value & 0x0F;
            }
            0x15 => self.filter.write_fc_lo(value),
            0x16 => self.filter.write_fc_hi(value),
            0x17 => self.filter.write_res_filt(value),
            0x18 => {
                self.volume = value & 0x0F;
                self.voice3_off = value & 0x80 != 0;
                self.filter.write_mode_vol(value);
            }
            _ => {}
        }
    }

    pub fn tick(&mut self) {
        let prev_msb = [
            self.voices[0].msb(),
            self.voices[1].msb(),
            self.voices[2].msb(),
        ];

        for voice in &mut self.voices {
            voice.clock_accumulator();
        }

        for voice in &mut self.voices {
            voice.clock_noise();
        }

        if self.voices[0].control & 0x02 != 0 {
            self.voices[0].apply_sync(prev_msb[2], self.voices[2].msb());
        }
        if self.voices[1].control & 0x02 != 0 {
            self.voices[1].apply_sync(prev_msb[0], self.voices[0].msb());
        }
        if self.voices[2].control & 0x02 != 0 {
            self.voices[2].apply_sync(prev_msb[1], self.voices[1].msb());
        }

        for index in 0..3 {
            let gate = self.voices[index].control & 0x01 != 0;
            self.envelopes[index].clock(gate);
        }

        let ring_mod_msb = [
            self.voices[2].msb(),
            self.voices[0].msb(),
            self.voices[1].msb(),
        ];

        let mut voice_values = [0_i32; 3];
        let mut voice_normalised = [0.0_f32; 3];

        let wave_dac = dac::wave_dac(self.model);
        let env_dac = dac::env_dac(self.model);
        let wave_zero = dac::wave_zero(self.model);

        for index in 0..3 {
            let waveform = self.voices[index].waveform_output(ring_mod_msb[index], self.model);
            let envelope = self.envelopes[index].level;
            // reSID voice output (20 bits): the 12-bit waveform and 8-bit
            // envelope each pass through their (nonlinear on the 6581) DAC,
            // then the envelope multiplies the waveform measured from its DC
            // "zero". The DC that `wave_zero` leaves in the signal rides into
            // the filter/mixer and is removed by the board's output coupling.
            let wave_level = wave_dac[waveform as usize] - wave_zero;
            let env_level = env_dac[usize::from(envelope)];
            let raw_amplitude = wave_level * env_level;
            let amplitude = self.audio_controls.channels[index].apply(raw_amplitude);
            voice_normalised[index] = amplitude / (2048.0 * 255.0);
            voice_values[index] = amplitude as i32;
        }

        // The op-amp filter owns routing, voice3off, the mixer, and the $D418
        // master-volume ladder; the external filter is the C64 board's output
        // coupling, which strips the operating-point DC and passes volume
        // steps through as the classic 4-bit digi.
        self.filter
            .clock(voice_values[0], voice_values[1], voice_values[2]);
        self.ext_filter.clock(self.filter.output());
        let normalised =
            self.ext_filter.output() as f32 / 32768.0 * self.audio_controls.master_gain();

        self.accumulator += normalised;
        for (index, sample) in voice_normalised.iter().copied().enumerate() {
            self.channel_accumulators[index] += sample;
        }
        self.sample_count += 1;

        // Integer Bresenham decimation: emit output_sample_rate samples per
        // cpu_freq input ticks, carrying the remainder. The window is 20 or 21
        // ticks (PAL: 985248/48000 = 20.526) and averages out to exactly the
        // target rate. The previous code reset the counter to 0 with no carry,
        // so it always used 21 ticks -> 46,916 Hz for a 48,000 Hz stream.
        self.sample_error += u64::from(self.output_sample_rate);
        if self.sample_error >= self.cpu_freq {
            self.sample_error -= self.cpu_freq;
            let count = self.sample_count as f32;
            self.buffer.push(self.accumulator / count);
            for index in 0..3 {
                self.channel_buffers[index].push(self.channel_accumulators[index] / count);
                self.channel_accumulators[index] = 0.0;
            }
            self.accumulator = 0.0;
            self.sample_count = 0;
        }
    }

    pub fn take_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    pub fn take_channel_buffers(&mut self) -> [Vec<f32>; 3] {
        std::array::from_fn(|index| std::mem::take(&mut self.channel_buffers[index]))
    }

    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    #[must_use]
    pub fn cpu_frequency(&self) -> u64 {
        self.cpu_freq
    }

    #[must_use]
    pub fn output_sample_rate(&self) -> u32 {
        self.output_sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimator_emits_the_advertised_sample_rate() {
        // One second of phi2 ticks must yield ~output_sample_rate samples.
        // Regression for #763: without the Bresenham remainder carry the
        // decimator reset its counter to 0 and always used ceil(cpu/rate)
        // ticks per sample, emitting 985248/21 = 46,916 Hz for a 48 kHz
        // stream (every tune ~2.3% sharp).
        for &(cpu, rate) in &[
            (985_248u64, 48_000u32),
            (1_022_727, 48_000),
            (985_248, 44_100),
        ] {
            let mut sid = Sid6581::new(cpu, rate);
            for _ in 0..cpu {
                sid.tick();
            }
            let n = sid.take_buffer().len() as i64;
            let diff = (n - i64::from(rate)).abs();
            assert!(
                diff <= 1,
                "cpu {cpu} rate {rate}: emitted {n} samples/sec, expected ~{rate}"
            );
        }
    }

    #[test]
    fn silent_when_no_voices_active() {
        // The op-amp model has a power-on DC operating point that the board's
        // 16 Hz output coupling drains over ~100 ms (the real SID's power-on
        // pop), so silence is asserted after settling. The residual floor is
        // the voice-input dither (~a few output LSBs), far below audibility.
        let mut sid = Sid6581::new(985_248, 48_000);
        for _ in 0..300_000 {
            sid.tick();
        }
        let buffer = sid.take_buffer();
        assert!(!buffer.is_empty(), "should emit samples even in silence");
        for &sample in &buffer[buffer.len() / 2..] {
            assert!(sample.abs() < 1e-3, "expected silence, got {sample}");
        }
    }

    #[test]
    fn d418_volume_writes_produce_digi_output() {
        // No voices gated: the only signal is a square wave played through the
        // $D418 master-volume nibble — the classic volume-register digi.
        let mut sid = Sid6581::new(985_248, 48_000);
        for i in 0..48_000 {
            sid.write(0x18, if (i / 20) % 2 == 0 { 0x00 } else { 0x0F });
            sid.tick();
        }
        let buffer = sid.take_buffer();
        assert!(!buffer.is_empty());
        let rms = (buffer.iter().map(|s| s * s).sum::<f32>() / buffer.len() as f32).sqrt();
        assert!(rms > 0.01, "expected audible digi output, got rms {rms}");
    }

    #[test]
    fn constant_volume_adds_no_digi_after_settling() {
        // A held volume with silent voices must decay to silence: the volume-DC
        // path is AC-coupled, so it contributes nothing at steady state and the
        // normal voice mix is left untouched (existing captures stay valid).
        let mut sid = Sid6581::new(985_248, 48_000);
        sid.write(0x18, 0x0F);
        for _ in 0..200_000 {
            sid.tick();
        }
        let buffer = sid.take_buffer();
        for &sample in &buffer[buffer.len() / 2..] {
            assert!(
                sample.abs() < 1e-4,
                "held volume should settle to silence, got {sample}"
            );
        }
    }

    #[test]
    fn sawtooth_produces_bipolar_waveform() {
        let mut sid = Sid6581::new(985_248, 48_000);
        let freq: u16 = 7_479;
        sid.write(0x00, (freq & 0xFF) as u8);
        sid.write(0x01, (freq >> 8) as u8);
        sid.write(0x04, 0x21);
        sid.write(0x05, 0x00);
        sid.write(0x06, 0xF0);
        sid.write(0x18, 0x0F);

        for _ in 0..40_000 {
            sid.tick();
        }

        let buffer = sid.take_buffer();
        assert!(
            buffer.len() > 100,
            "want lots of samples, got {}",
            buffer.len()
        );
        assert!(buffer.iter().any(|&sample| sample > 0.01));
        assert!(buffer.iter().any(|&sample| sample < -0.01));
    }

    #[test]
    fn adsr_attack_reaches_max_level() {
        let mut sid = Sid6581::new(985_248, 48_000);
        sid.write(0x05, 0x00);
        sid.write(0x06, 0xF0);
        sid.write(0x04, 0x01);

        for _ in 0..3_000 {
            sid.tick();
        }

        assert_eq!(sid.envelopes[0].level, 0xFF);
        assert_eq!(sid.envelopes[0].phase, Phase::Sustain);
    }

    // Regression: a voice gated on *after* the SID has been clocking for a
    // while (the real case — boot + program startup all clock the SID before
    // a note ever fires) must still attack. The earlier tests all wrote the
    // gate on a fresh SID with `rate_counter == 0`, so they only ever hit the
    // lucky path and never caught the ADSR-delay-bug counter wrapping the full
    // u32 range into multi-thousand-second silence. With the rate counter
    // bounded to 15 bits, any missed match recovers within ~0x8000 cycles.
    #[test]
    fn adsr_attack_reaches_max_level_when_gated_after_warm_up() {
        let mut sid = Sid6581::new(985_248, 48_000);
        // Clock the SID with the voice ungated so the free-running rate
        // counter lands at an arbitrary, non-zero phase — like a real machine
        // by the time a program fires its first note.
        for _ in 0..5_000 {
            sid.tick();
        }

        sid.write(0x05, 0x00);
        sid.write(0x06, 0xF0);
        sid.write(0x04, 0x01);

        // Attack rate 0 reaches peak in ~2 ms; allow a 15-bit wrap of slack.
        for _ in 0..40_000 {
            sid.tick();
        }

        assert_eq!(sid.envelopes[0].level, 0xFF);
        assert_eq!(sid.envelopes[0].phase, Phase::Sustain);
    }

    #[test]
    fn adsr_release_decays_to_zero() {
        let mut sid = Sid6581::new(985_248, 48_000);
        sid.write(0x05, 0x00);
        sid.write(0x06, 0xF0);
        sid.write(0x04, 0x01);

        for _ in 0..3_000 {
            sid.tick();
        }
        assert_eq!(sid.envelopes[0].level, 0xFF);

        sid.write(0x04, 0x00);
        for _ in 0..50_000 {
            sid.tick();
        }
        assert_eq!(sid.envelopes[0].level, 0);
    }

    #[test]
    fn osc3_read_advances_with_oscillator() {
        let mut sid = Sid6581::new(985_248, 48_000);
        sid.write(0x0E, 0xFF);
        sid.write(0x0F, 0xFF);
        sid.write(0x12, 0x20);

        for _ in 0..100 {
            sid.tick();
        }

        assert!(sid.read(0x1B) > 0);
    }

    #[test]
    fn env3_read_reflects_envelope_level() {
        let mut sid = Sid6581::new(985_248, 48_000);
        sid.write(0x13, 0x00);
        sid.write(0x14, 0xF0);
        sid.write(0x12, 0x01);

        for _ in 0..3_000 {
            sid.tick();
        }

        assert_eq!(sid.read(0x1C), 0xFF);
    }

    #[test]
    fn filter_attenuates_routed_voice() {
        let run = |filtered: bool| -> f32 {
            let mut sid = Sid6581::new(985_248, 48_000);
            let freq: u16 = 65_535;
            sid.write(0x00, (freq & 0xFF) as u8);
            sid.write(0x01, (freq >> 8) as u8);
            sid.write(0x04, 0x21);
            sid.write(0x05, 0x00);
            sid.write(0x06, 0xF0);

            if filtered {
                sid.write(0x15, 0x00);
                sid.write(0x16, 0x00);
                sid.write(0x17, 0x01);
                sid.write(0x18, 0x1F);
            } else {
                sid.write(0x18, 0x0F);
            }

            // Let the power-on DC transient drain through the output coupling
            // before measuring, then take the RMS of a settled window.
            for _ in 0..300_000 {
                sid.tick();
            }
            let _ = sid.take_buffer();
            for _ in 0..60_000 {
                sid.tick();
            }

            let buffer = sid.take_buffer();
            let sum_sq: f32 = buffer.iter().map(|sample| sample * sample).sum();
            (sum_sq / buffer.len() as f32).sqrt()
        };

        let direct_rms = run(false);
        let filtered_rms = run(true);
        assert!(filtered_rms < direct_rms * 0.8);
    }

    #[test]
    fn take_buffer_drains_pending_samples() {
        let mut sid = Sid6581::new(985_248, 48_000);
        for _ in 0..1_000 {
            sid.tick();
        }
        let buffer = sid.take_buffer();
        assert!(!buffer.is_empty());
        assert_eq!(sid.buffer_len(), 0);
    }

    #[test]
    fn host_audio_controls_do_not_change_voice_registers() {
        let mut sid = Sid6581::new(985_248, 48_000);
        sid.write(0x04, 0x21);
        sid.write(0x18, 0x0F);

        sid.set_audio_channel_enabled(SidChannel::Voice1, false);

        assert!(!sid.audio_controls().channel(SidChannel::Voice1).enabled());
        assert_eq!(sid.voices[0].control, 0x21);
        assert_eq!(sid.volume, 0x0F);
    }

    #[test]
    fn host_audio_controls_mute_voice_output_only() {
        let render_peak = |mut sid: Sid6581| -> f32 {
            for _ in 0..200_000 {
                sid.tick();
            }
            // Skip the initial settling window: the one-off 0→15 volume write
            // produces a brief (correct) $D418 click that the AC-coupled digi
            // path decays away. This test measures steady-state voice output.
            let buffer = sid.take_buffer();
            buffer[buffer.len() / 2..]
                .iter()
                .copied()
                .map(f32::abs)
                .fold(0.0, f32::max)
        };

        let mut audible = Sid6581::new(985_248, 48_000);
        let freq: u16 = 7_479;
        audible.write(0x00, (freq & 0xFF) as u8);
        audible.write(0x01, (freq >> 8) as u8);
        audible.write(0x04, 0x21);
        audible.write(0x05, 0x00);
        audible.write(0x06, 0xF0);
        audible.write(0x18, 0x0F);

        let mut muted = audible.clone();
        muted.set_audio_channel_enabled(SidChannel::Voice1, false);

        assert!(render_peak(audible) > 0.01);
        assert!(render_peak(muted) < 0.001);
    }

    #[test]
    fn host_audio_controls_clamp_gain() {
        let mut controls = AudioControls::default();
        controls.set_master_gain(2.0);
        controls.set_channel_gain(SidChannel::Voice2, f32::NAN);
        controls.set_channel_gain(SidChannel::Voice3, -1.0);

        assert_eq!(controls.master_gain(), 1.0);
        assert_eq!(controls.channel(SidChannel::Voice2).gain(), 0.0);
        assert_eq!(controls.channel(SidChannel::Voice3).gain(), 0.0);
    }

    #[test]
    fn channel_buffers_match_main_buffer_length() {
        let mut sid = Sid6581::new(985_248, 48_000);
        sid.write(0x00, 0x80);
        sid.write(0x01, 0x10);
        sid.write(0x02, 0x00);
        sid.write(0x03, 0x08);
        sid.write(0x05, 0x00);
        sid.write(0x06, 0xF0);
        sid.write(0x04, 0x41);
        sid.write(0x18, 0x0F);

        for _ in 0..30_000 {
            sid.tick();
        }

        let main_len = sid.buffer_len();
        let channel_buffers = sid.take_channel_buffers();
        let main_buffer = sid.take_buffer();

        assert_eq!(main_buffer.len(), main_len);
        for (index, channel) in channel_buffers.iter().enumerate() {
            assert_eq!(
                channel.len(),
                main_len,
                "channel {index} has {} samples, want {main_len}",
                channel.len()
            );
        }
    }
}
