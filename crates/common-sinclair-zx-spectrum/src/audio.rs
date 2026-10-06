//! Speaker audio mixing for Spectrum-family machines.
//!
//! Source references:
//! - `knowledge/concepts/audio-mixing.md`
//! - Adapted from `../Emu198x-Older/crates/common-sinclair-zx-spectrum/src/audio.rs`
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

/// Audio routing version. Bumped when the audio path through this crate
/// (beeper mix, EAR mix, AY mix, speaker → audio_frame routing) changes in
/// a way that invalidates previously-captured audio hashes in the
/// catalogue. The catalogue manifest carries the version each hash was
/// captured against; a mismatch fails loud with a re-capture instruction.
///
/// **Version 1** (2026-05-19): audio path with AY mix wired in for 128K-class
/// and Amstrad-class via `mix_ay_into_audio` end-of-frame. Beeper + EAR
/// blend ratios fixed at 0.8 / 0.2 in `SpeakerMixer::level`.
///
/// **Version 2** (2026-06-05): beeper output is now AC-coupled like the real
/// speaker. `BeeperAudio::end_frame` maps the per-sample level unipolar
/// (silence → 0, full → `volume`) and passes it through a first-order
/// DC-blocking high-pass, mirroring the NES APU. Silence sits at 0 instead of
/// the old −full-scale rail, a held level decays to silence, and a tone is a
/// clean AC swing centred on zero. `volume` default raised 0.5 → 1.0 to keep
/// the perceived loudness of a toggling tone unchanged across the remap.
///
/// **Version 3** (2026-07-22): 128K-family beeper, EAR and AY events use
/// the corrected divide-by-five CPU cadence.
///
/// **Version 4** (2026-10-06, current): `BeeperAudio` divides each sample by
/// the T-states its own bin holds instead of the mean bin width, so a held
/// speaker level is flat rather than a ~17.6 kHz ripple (#1634). Every
/// sample with the speaker above zero changes by up to about half a percent.
/// The same change releases the tape EAR input a frame after the tape stops
/// (#1633), which takes the speaker's 0.2 EAR contribution out of the idle
/// windows of tapes that ended high.
///
/// See `knowledge/decisions/spectrum-architecture-review.md` Seam 4 for
/// the re-capture discipline this constant enforces.
pub const AUDIO_ROUTING_VERSION: u32 = 4;

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
    /// DC-blocking high-pass state: previous input and output samples.
    /// Transient DSP state — defaults to 0.0 on deserialize, which is the
    /// correct rest state for the filter.
    #[serde(default)]
    hp_prev_in: f32,
    #[serde(default)]
    hp_prev_out: f32,
}

/// First-order DC-blocking high-pass coefficient: `y[n] = α(y[n-1] + x[n] − x[n-1])`.
/// `α ≈ 0.9952` gives a ~34 Hz cutoff at the Spectrum's 44.1 kHz output — well
/// below the audible band, so it removes the speaker's DC offset without
/// touching the tone. Same form and value as the NES APU's blocker.
const DC_BLOCK_ALPHA: f32 = 0.9952;

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
            volume: 1.0,
            audio_controls: AudioControls::default(),
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
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

        let len = out.len().min(self.samples_per_frame);

        for (index, sample) in out.iter_mut().take(len).enumerate() {
            // Each sample averages the level over exactly the T-states its
            // bin holds. A frame rarely divides evenly (70,908 / 882 on the
            // 128K), so bins are 80 or 81 T-states wide; dividing every one
            // by the mean width turned a held level into a two-value ripple
            // (#1634). The Jupiter Ace's downsampler divides by its own tick
            // count for the same reason.
            let width = self.sample_start(index + 1) - self.sample_start(index);
            let fraction = (f64::from(self.accum[index]) / f64::from(width)).clamp(0.0, 1.0);
            // Unipolar level: silence → 0, full speaker → `volume`. The real
            // speaker is AC-coupled, so a DC-blocking high-pass removes the
            // offset — silence rests at 0, a held level decays away, and a
            // tone becomes a clean swing centred on zero. Advance the filter
            // even when the channel is muted so unmuting doesn't glitch.
            let raw = (fraction * f64::from(self.volume)) as f32;
            let filtered = DC_BLOCK_ALPHA * (self.hp_prev_out + raw - self.hp_prev_in);
            self.hp_prev_in = raw;
            self.hp_prev_out = filtered;
            *sample = self
                .audio_controls
                .channel(SpeakerChannel::Speaker)
                .apply(filtered)
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

    /// First T-state of sample bin `index`; `index == samples_per_frame` is
    /// the end of the frame. Integer arithmetic, so the bins tile the frame
    /// exactly and [`Self::end_frame`] sees the same widths this fills.
    fn sample_start(&self, index: usize) -> u32 {
        let start =
            index as u64 * u64::from(self.tstates_per_frame) / self.samples_per_frame as u64;
        u32::try_from(start).unwrap_or(u32::MAX)
    }

    fn flush_to(&mut self, tstate: u32) {
        let from = self.last_tstate;
        let to = tstate.min(self.tstates_per_frame);
        if to <= from {
            return;
        }

        // The bin holding `from`: the floor estimate can land one bin late
        // where the integer edges round, so start a bin early and let the
        // overlap test skip it.
        let estimate =
            u64::from(from) * self.samples_per_frame as u64 / u64::from(self.tstates_per_frame);
        let first = usize::try_from(estimate)
            .unwrap_or(usize::MAX)
            .saturating_sub(1);

        for sample_index in first..self.samples_per_frame {
            let sample_start_ts = self.sample_start(sample_index);
            if sample_start_ts >= to {
                break;
            }
            let sample_end_ts = self.sample_start(sample_index + 1);
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
    fn silence_maps_to_zero() {
        // AC-coupled output: with no speaker activity, every sample rests at 0,
        // not the old −full-scale rail.
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        let mut out = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut out);

        for &sample in &out {
            assert!(sample.abs() < 0.01, "silence sample {sample} not near zero");
        }
    }

    #[test]
    fn held_level_is_dc_blocked_toward_zero() {
        // A speaker held high is DC: the high-pass passes the onset transient,
        // then decays toward silence over the frame.
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        audio.set_level(0, 1.0);
        let mut out = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut out);

        assert!(
            out[0] > 0.9,
            "onset sample {} should be near full scale",
            out[0]
        );
        let last = *out.last().expect("output is non-empty");
        assert!(
            last < 0.05,
            "held level should decay; last sample was {last}"
        );
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
    fn held_level_decays_to_silence_across_frames() {
        // The filter state carries across frames, so a level held since the
        // first frame has fully decayed away by the second — no DC remains.
        let mut audio = BeeperAudio::new(44_100, 69_888, 3_500_000);
        audio.set_volume(1.0);
        audio.set_level(0, 1.0);
        let mut first = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut first);

        let mut second = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut second);

        for &sample in &second {
            assert!(
                sample.abs() < 0.02,
                "held level should be silent by frame 2, got {sample}"
            );
        }
    }

    /// #1634: a level held for whole frames must come out flat. Before the
    /// fix the 80- and 81-T-state bins were both divided by the mean width,
    /// so a held level alternated between two values the DC blocker could
    /// not remove: about 81 LSB peak to peak at the EAR's 0.2.
    ///
    /// The DC blocker's f32 state settles on a constant a few millionths
    /// above zero rather than at zero itself, so the test asks for a flat
    /// output below one 16-bit step, not for exact zero.
    #[test]
    fn a_held_level_settles_to_a_flat_silence() {
        for (tstates_per_frame, cpu_hz) in [(69_888, 3_500_000), (70_908, 3_546_900)] {
            for level in [0.2_f32, 0.8, 1.0] {
                let mut audio = BeeperAudio::new(44_100, tstates_per_frame, cpu_hz);
                audio.set_volume(1.0);
                audio.set_level(0, level);
                let mut out = vec![0.0; audio.samples_per_frame()];
                for _ in 0..50 {
                    audio.end_frame(&mut out);
                }
                let low = out.iter().copied().fold(f32::INFINITY, f32::min);
                let high = out.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                assert_eq!(
                    low, high,
                    "level {level} held at {tstates_per_frame} T-states a frame \
                     ripples between {low} and {high}"
                );
                assert!(
                    high.abs() < 1.0 / 32_768.0,
                    "and settles below one 16-bit step, not at {high}"
                );
            }
        }
    }

    #[test]
    fn the_sample_bins_tile_the_frame_exactly() {
        for (tstates_per_frame, cpu_hz) in [(69_888, 3_500_000), (70_908, 3_546_900)] {
            let audio = BeeperAudio::new(44_100, tstates_per_frame, cpu_hz);
            let bins = audio.samples_per_frame();
            assert_eq!(audio.sample_start(0), 0);
            assert_eq!(audio.sample_start(bins), tstates_per_frame);
            for index in 0..bins {
                let width = audio.sample_start(index + 1) - audio.sample_start(index);
                assert!(width > 0, "bin {index} is empty");
            }
        }
    }

    /// The width a bin is divided by is the width it was filled over, so a
    /// level held over any whole bins averages to exactly that level.
    #[test]
    fn a_level_change_on_a_bin_edge_fills_whole_bins_exactly() {
        let mut audio = BeeperAudio::new(44_100, 70_908, 3_546_900);
        audio.set_volume(1.0);
        let edge = audio.sample_start(441);
        audio.set_level(edge, 1.0);
        let mut out = vec![0.0; audio.samples_per_frame()];
        audio.end_frame(&mut out);
        assert!(
            out[..441].iter().all(|s| *s == 0.0),
            "before the edge: silence"
        );
        assert!(out[441] > 0.99, "the first full bin carries the whole step");
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
