//! Speaker audio mixing for Spectrum-family machines.
//!
//! Source references:
//! - `wiki/concepts/audio-mixing.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/audio.rs`
//!
//! The real 48K Spectrum speaker is driven by the beeper output and the tape
//! EAR input through a simple resistor network. The machine reports changes in
//! the combined speaker level at precise T-state positions, and this mixer
//! area-averages those transitions into PCM samples for one frame.
//!
//! [`SpeakerMixer`] holds the two boolean lines (beeper and EAR) and produces
//! the blended `f32` level the beeper accepts. Every Spectrum-family machine
//! uses the same blend ratios, so they share this one struct rather than
//! re-spelling the literal in each crate.

/// Combined beeper + tape-EAR speaker line state with the canonical blend
/// ratios (0.8 for the beeper output, 0.2 for the tape EAR input).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpeakerMixer {
    /// Last value written to bit 4 of port `$FE`.
    pub beeper: bool,
    /// Last sampled tape EAR level on bit 6 of port `$FE`.
    pub ear: bool,
}

impl SpeakerMixer {
    /// Returns the blended speaker level fed to the beeper mixer.
    #[must_use]
    pub fn level(self) -> f32 {
        let beeper = if self.beeper { 0.8 } else { 0.0 };
        let ear = if self.ear { 0.2 } else { 0.0 };
        beeper + ear
    }
}

/// Host-side Spectrum speaker channel identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum SpeakerChannel {
    /// Mixed beeper + tape EAR speaker output.
    Speaker,
}

impl SpeakerChannel {
    const fn index(self) -> usize {
        match self {
            Self::Speaker => 0,
        }
    }

    /// Human-readable channel label for frontend status messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Speaker => "speaker",
        }
    }
}

/// Per-channel host mixer control.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Whether this channel contributes to host audio output.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Linear channel gain after sanitisation, clamped to 0.0..=1.0.
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

/// Host-side audio controls for the Spectrum speaker mixer.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AudioControls {
    master_gain: f32,
    channels: [ChannelControl; 1],
}

impl Default for AudioControls {
    fn default() -> Self {
        Self {
            master_gain: 1.0,
            channels: [ChannelControl::default(); 1],
        }
    }
}

impl AudioControls {
    /// Master gain applied to the host speaker output.
    #[must_use]
    pub const fn master_gain(self) -> f32 {
        self.master_gain
    }

    /// Set master gain. Non-finite values become 0.0; finite values clamp to
    /// 0.0..=1.0.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = sanitize_gain(gain);
    }

    /// Return control state for the speaker output.
    #[must_use]
    pub const fn channel(self, channel: SpeakerChannel) -> ChannelControl {
        self.channels[channel.index()]
    }

    /// Enable or disable the speaker output in the host mixer.
    pub fn set_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        self.channels[channel.index()].set_enabled(enabled);
    }

    /// Set speaker gain. Non-finite values become 0.0; finite values clamp to
    /// 0.0..=1.0.
    pub fn set_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
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
        }; 1],
    }
}

fn sanitize_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BeeperAudio {
    tstates_per_frame: u32,
    samples_per_frame: usize,
    current_level: f32,
    last_tstate: u32,
    accum: Vec<f32>,
    volume: f32,
    #[serde(default = "default_audio_controls")]
    audio_controls: AudioControls,
}

impl BeeperAudio {
    /// Creates a beeper mixer for one machine clock domain.
    #[must_use]
    pub fn new(sample_rate: u32, tstates_per_frame: u32, cpu_hz: u32) -> Self {
        let samples_per_frame = (u64::from(sample_rate) * u64::from(tstates_per_frame))
            .div_ceil(u64::from(cpu_hz)) as usize;

        Self {
            tstates_per_frame,
            samples_per_frame,
            current_level: 0.0,
            last_tstate: 0,
            accum: vec![0.0; samples_per_frame],
            volume: 0.5,
            audio_controls: AudioControls::default(),
        }
    }

    /// Returns the number of samples produced per frame.
    #[must_use]
    pub fn samples_per_frame(&self) -> usize {
        self.samples_per_frame
    }

    /// Records a speaker-level change at one T-state within the frame.
    pub fn set_level(&mut self, tstate: u32, level: f32) {
        if (level - self.current_level).abs() < 0.001 {
            return;
        }

        self.flush_to(tstate);
        self.current_level = level;
        self.last_tstate = tstate;
    }

    /// Finishes the current frame and writes PCM samples into `out`.
    pub fn end_frame(&mut self, out: &mut [f32]) {
        self.flush_to(self.tstates_per_frame);

        let tstates_per_sample = f64::from(self.tstates_per_frame) / self.samples_per_frame as f64;
        let len = out.len().min(self.samples_per_frame);

        for (index, sample) in out.iter_mut().take(len).enumerate() {
            let fraction = (f64::from(self.accum[index]) / tstates_per_sample).clamp(0.0, 1.0);
            let base = ((fraction * 2.0 - 1.0) * f64::from(self.volume)) as f32;
            *sample = self
                .audio_controls
                .channel(SpeakerChannel::Speaker)
                .apply(base)
                * self.audio_controls.master_gain();
        }

        self.accum.fill(0.0);
        self.last_tstate = 0;
    }

    /// Sets the output volume in the inclusive range `0.0..=1.0`.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
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

    /// Enable or disable the speaker in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        self.audio_controls.set_channel_enabled(channel, enabled);
    }

    /// Set the speaker host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        self.audio_controls.set_channel_gain(channel, gain);
    }

    fn flush_to(&mut self, tstate: u32) {
        let from = self.last_tstate;
        let to = tstate.min(self.tstates_per_frame);
        if to <= from {
            return;
        }

        let tstates_per_sample = f64::from(self.tstates_per_frame) / self.samples_per_frame as f64;
        let start_sample = (f64::from(from) / tstates_per_sample) as usize;
        let end_sample =
            ((f64::from(to) / tstates_per_sample).ceil() as usize).min(self.samples_per_frame);

        for sample_index in start_sample..end_sample {
            let sample_start_ts = (sample_index as f64 * tstates_per_sample) as u32;
            let sample_end_ts = ((sample_index + 1) as f64 * tstates_per_sample) as u32;
            let overlap_start = from.max(sample_start_ts);
            let overlap_end = to.min(sample_end_ts);

            if overlap_end > overlap_start {
                self.accum[sample_index] +=
                    self.current_level * (overlap_end - overlap_start) as f32;
            }
        }

        self.last_tstate = to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_maps_to_negative_full_scale() {
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        let mut out = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut out);

        for &sample in &out {
            assert!((sample - (-1.0)).abs() < 0.01);
        }
    }

    #[test]
    fn constant_high_maps_to_positive_full_scale() {
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        audio.set_level(0, 1.0);
        let mut out = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut out);

        for &sample in &out {
            assert!((sample - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn half_frame_toggle_averages_near_zero() {
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        audio.set_level(0, 1.0);
        audio.set_level(69_888 / 2, 0.0);
        let mut out = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut out);

        let avg = out.iter().sum::<f32>() / out.len() as f32;
        assert!(avg.abs() < 0.1);
    }

    #[test]
    fn level_carries_across_frames() {
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        audio.set_level(0, 1.0);
        let mut first = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut first);

        let mut second = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut second);

        for &sample in &second {
            assert!((sample - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn host_audio_controls_mute_speaker_output_only() {
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        audio.set_level(0, 1.0);
        audio.set_audio_channel_enabled(SpeakerChannel::Speaker, false);
        let mut out = vec![1.0; audio.samples_per_frame()];
        audio.end_frame(&mut out);

        assert!(out.iter().all(|sample| *sample == 0.0));
        assert!(
            !audio
                .audio_controls()
                .channel(SpeakerChannel::Speaker)
                .enabled()
        );
    }

    #[test]
    fn host_audio_controls_clamp_gain() {
        let mut controls = AudioControls::default();
        controls.set_master_gain(2.0);
        controls.set_channel_gain(SpeakerChannel::Speaker, f32::NAN);

        assert_eq!(controls.master_gain(), 1.0);
        assert_eq!(controls.channel(SpeakerChannel::Speaker).gain(), 0.0);
    }
}
