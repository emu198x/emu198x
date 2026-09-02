//! Machine audio into a Web Audio graph.
//!
//! A browser cannot be handed a callback the way a native audio device can,
//! so the machine's samples are buffered here and drained by the page into an
//! `AudioWorklet`. The buffer is bounded: a page that stops draining — a
//! backgrounded tab, a worklet that never started because the viewer has not
//! interacted with the page yet — must not grow it without limit.
//!
//! Rate and channel conversion reuses `convert_audio_packet`, the same
//! function the native output uses. A second resampler would be a second
//! thing to be wrong.

use std::collections::VecDeque;

use emu198x_shell::{AudioPacket, AudioSink, MachineError, convert_audio_packet};

/// Buffers machine audio for a page to drain into a Web Audio worklet.
#[derive(Debug)]
pub struct WebAudioOutput {
    samples: VecDeque<f32>,
    capacity: usize,
    output_rate: u32,
    output_channels: u16,
    enabled: bool,
    dropped: u64,
}

impl WebAudioOutput {
    /// Creates an output for a graph running at `output_rate` with
    /// `output_channels`, buffering at most `capacity` samples.
    ///
    /// `capacity` is in samples, not frames: at 48 kHz stereo, one second is
    /// 96 000.
    #[must_use]
    pub fn new(output_rate: u32, output_channels: u16, capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity.min(1 << 16)),
            capacity,
            output_rate,
            output_channels,
            enabled: true,
            dropped: 0,
        }
    }

    /// Takes everything buffered, leaving the buffer empty.
    #[must_use]
    pub fn drain(&mut self) -> Vec<f32> {
        self.samples.drain(..).collect()
    }

    /// Takes at most `count` samples, for a worklet asking for one block.
    #[must_use]
    pub fn drain_at_most(&mut self, count: usize) -> Vec<f32> {
        let take = count.min(self.samples.len());
        self.samples.drain(..take).collect()
    }

    /// Samples currently buffered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Samples discarded because the page was not draining fast enough.
    ///
    /// Non-zero means the page is underconsuming, which is audible as a gap
    /// rather than as latency. Exposed so a caller can tell the two apart.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Whether machine audio is being buffered.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Starts or stops buffering.
    ///
    /// Disabling clears what is queued: resuming should play what the machine
    /// is doing now, not the seconds of sound it made while muted.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.samples.clear();
        }
    }

    fn push(&mut self, samples: &[f32]) {
        self.samples.extend(samples.iter().copied());
        while self.samples.len() > self.capacity {
            self.samples.pop_front();
            self.dropped += 1;
        }
    }
}

impl AudioSink for WebAudioOutput {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        if !self.enabled {
            return Ok(());
        }
        let converted = convert_audio_packet(
            packet.samples,
            packet.sample_rate,
            packet.channels,
            self.output_rate,
            self.output_channels,
        );
        self.push(&converted);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::MachineTime;

    fn packet(samples: &[f32]) -> AudioPacket<'_> {
        AudioPacket {
            timestamp: MachineTime::new(0),
            sample_rate: 48_000,
            channels: 1,
            samples,
        }
    }

    fn output() -> WebAudioOutput {
        WebAudioOutput::new(48_000, 1, 1024)
    }

    #[test]
    fn samples_drain_in_order_and_only_once() {
        let mut out = output();
        out.push_audio(packet(&[0.25, -0.25])).expect("accepted");

        assert_eq!(out.drain(), vec![0.25, -0.25]);
        assert!(
            out.drain().is_empty(),
            "draining twice would replay the same audio"
        );
    }

    #[test]
    fn the_buffer_drops_oldest_rather_than_growing_without_bound() {
        let mut out = WebAudioOutput::new(48_000, 1, 2);
        out.push_audio(packet(&[1.0, 2.0, 3.0])).expect("accepted");

        assert_eq!(out.len(), 2, "the buffer stayed at its capacity");
        assert_eq!(out.drain(), vec![2.0, 3.0], "the newest audio survived");
        assert_eq!(out.dropped(), 1, "the drop was counted, not hidden");
    }

    #[test]
    fn a_worklet_can_take_one_block_at_a_time() {
        let mut out = output();
        out.push_audio(packet(&[1.0, 2.0, 3.0, 4.0]))
            .expect("accepted");

        assert_eq!(out.drain_at_most(2), vec![1.0, 2.0]);
        assert_eq!(
            out.drain_at_most(10),
            vec![3.0, 4.0],
            "asking for more than is buffered is not an error"
        );
        assert!(out.drain_at_most(2).is_empty());
    }

    #[test]
    fn muting_discards_rather_than_accumulating() {
        let mut out = output();
        out.set_enabled(false);
        out.push_audio(packet(&[1.0, 2.0])).expect("accepted");

        assert!(out.is_empty(), "a muted machine buffered audio anyway");

        out.set_enabled(true);
        out.push_audio(packet(&[3.0])).expect("accepted");
        assert_eq!(
            out.drain(),
            vec![3.0],
            "unmuting replayed what was made while muted"
        );
    }

    #[test]
    fn a_silent_packet_is_not_an_error() {
        let mut out = output();
        out.push_audio(packet(&[]))
            .expect("an empty packet is accepted");
        assert!(out.is_empty());
    }
}
