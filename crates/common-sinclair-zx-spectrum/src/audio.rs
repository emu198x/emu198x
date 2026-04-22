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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BeeperAudio {
    tstates_per_frame: u32,
    samples_per_frame: usize,
    current_level: f32,
    last_tstate: u32,
    accum: Vec<f32>,
    volume: f32,
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
            *sample = ((fraction * 2.0 - 1.0) * f64::from(self.volume)) as f32;
        }

        self.accum.fill(0.0);
        self.last_tstate = 0;
    }

    /// Sets the output volume in the inclusive range `0.0..=1.0`.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
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
}
