//! Shared host-side audio conversion helpers.

pub mod band_limited;

#[cfg(not(target_arch = "wasm32"))]
use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(not(target_arch = "wasm32"))]
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
#[cfg(not(target_arch = "wasm32"))]
use thiserror::Error;

#[cfg(not(target_arch = "wasm32"))]
use crate::{AudioPacket, AudioSink, MachineError};

/// Host audio output setup or callback queue failure.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Error)]
pub enum NativeAudioError {
    /// The system has no default output device.
    #[error("no default output device is available")]
    NoDefaultOutputDevice,

    /// Querying the default output format failed.
    #[error("failed to query the default output config: {source}")]
    DefaultOutputConfig {
        /// Underlying CPAL error.
        #[source]
        source: cpal::Error,
    },

    /// The host output sample format is not supported by the shared path.
    #[error("unsupported output sample format {format:?}")]
    UnsupportedSampleFormat {
        /// Unsupported CPAL sample format.
        format: SampleFormat,
    },

    /// Building the output stream failed.
    #[error("failed to build the output stream: {source}")]
    BuildStream {
        /// Underlying CPAL error.
        #[source]
        source: cpal::Error,
    },

    /// Starting playback failed.
    #[error("failed to start the audio stream: {source}")]
    PlayStream {
        /// Underlying CPAL error.
        #[source]
        source: cpal::Error,
    },
}

/// CPAL-backed host audio output for native verifier shells.
///
/// This is deliberately host-policy only: it handles device setup, bounded
/// callback buffering, sample-rate conversion, and host channel conversion.
/// Per-chip/per-voice mute and gain belong in each machine's native audio
/// mixer before packets cross the shared shell boundary.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeAudioOutput {
    _stream: Stream,
    shared: Arc<Mutex<AudioBuffer>>,
    sample_rate: u32,
    channels: u16,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeAudioOutput {
    /// Creates a native host output stream with a bounded callback queue.
    ///
    /// Playback waits for 50 ms of audio at startup and after a clear or
    /// underrun, capped by `max_buffer_ms` (at least one output frame). This
    /// reserve absorbs brief delays in packet delivery without inserting gaps.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no default output device, the host output
    /// format cannot be queried or built, the sample format is unsupported, or
    /// the stream cannot be started.
    pub fn new(max_buffer_ms: u32) -> Result<Self, NativeAudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(NativeAudioError::NoDefaultOutputDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|source| NativeAudioError::DefaultOutputConfig { source })?;
        let config = supported.config();
        // cpal 0.17 replaced `SampleRate(u32)` with `type SampleRate = u32`,
        // so the inner `.0` access from the newtype is gone — `sample_rate`
        // is the bare u32 now.
        let shared = Arc::new(Mutex::new(AudioBuffer::new(
            config.sample_rate,
            config.channels,
            max_buffer_ms,
        )));
        let stream = build_output_stream(&device, &config, supported.sample_format(), &shared)?;
        stream
            .play()
            .map_err(|source| NativeAudioError::PlayStream { source })?;

        Ok(Self {
            _stream: stream,
            shared,
            sample_rate: config.sample_rate,
            channels: config.channels,
        })
    }

    /// Clears queued host audio without stopping the output stream.
    pub fn clear(&mut self) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.clear();
        }
    }

    /// Host output sample rate selected by CPAL.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Host output channel count selected by CPAL.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AudioSink for NativeAudioOutput {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        let samples = convert_audio_packet(
            packet.samples,
            packet.sample_rate,
            packet.channels,
            self.sample_rate,
            self.channels,
        );
        let mut shared = self.shared.lock().map_err(|_| MachineError::Host {
            reason: "audio buffer lock poisoned".to_owned(),
        })?;
        shared.push(&samples);
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct AudioBuffer {
    samples: VecDeque<f32>,
    channels: usize,
    max_samples: usize,
    reserve_samples: usize,
    primed: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl AudioBuffer {
    fn new(sample_rate: u32, channels: u16, max_buffer_ms: u32) -> Self {
        let channels = usize::from(channels).max(1);
        let max_frames = usize::try_from(u64::from(sample_rate) * u64::from(max_buffer_ms) / 1_000)
            .unwrap_or(usize::MAX)
            .clamp(1, usize::MAX / channels);
        let reserve_frames = usize::try_from((u64::from(sample_rate) * 50).div_ceil(1_000))
            .unwrap_or(usize::MAX)
            .clamp(1, max_frames);
        let max_samples = max_frames * channels;
        Self {
            samples: VecDeque::with_capacity(max_samples),
            channels,
            max_samples,
            reserve_samples: reserve_frames * channels,
            primed: false,
        }
    }

    fn clear(&mut self) {
        self.samples.clear();
        self.primed = false;
    }

    fn push(&mut self, samples: &[f32]) {
        // Keep complete frames and trim before extending, so even an oversized
        // turbo packet cannot grow the queue beyond its bounded storage.
        let complete = samples.len() / self.channels * self.channels;
        let keep = complete.min(self.max_samples);
        let incoming = &samples[complete - keep..complete];
        let discard = self.samples.len().saturating_sub(self.max_samples - keep);
        self.samples.drain(..discard);
        self.samples.extend(incoming.iter().copied());
    }

    fn write_output<T: SizedSample + FromSample<f32>>(&mut self, data: &mut [T]) {
        data.fill(T::from_sample(0.0));
        let needed = data.len() / self.channels * self.channels;
        if needed == 0 {
            return;
        }
        // Startup/recovery priming also exists in the read-only donor
        // Emu198x-Oldest/crates/emu-core/src/audio.rs (AudioQueue). Here the
        // threshold is format-derived and capped at the caller's queue bound.
        let threshold = self.reserve_samples.max(needed.min(self.max_samples));
        if !self.primed && self.samples.len() < threshold {
            return;
        }
        if self.samples.len() < needed && needed <= self.max_samples {
            self.primed = false;
            return;
        }
        self.primed = true;
        // A host callback larger than the entire queue can still consume its
        // complete frames; its remaining frames are silence and trigger priming.
        for frame in data.chunks_exact_mut(self.channels) {
            if self.samples.len() < self.channels {
                self.primed = false;
                break;
            }
            for slot in frame {
                *slot = T::from_sample(self.samples.pop_front().unwrap_or(0.0));
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_output_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    shared: &Arc<Mutex<AudioBuffer>>,
) -> Result<Stream, NativeAudioError> {
    match sample_format {
        SampleFormat::F32 => build_typed_output_stream::<f32>(device, config, shared),
        SampleFormat::I16 => build_typed_output_stream::<i16>(device, config, shared),
        SampleFormat::U16 => build_typed_output_stream::<u16>(device, config, shared),
        format => Err(NativeAudioError::UnsupportedSampleFormat { format }),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_typed_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: &Arc<Mutex<AudioBuffer>>,
) -> Result<Stream, NativeAudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let shared = Arc::clone(shared);
    device
        .build_output_stream(
            // cpal 0.18 takes the config by value; `StreamConfig` is `Copy`, so
            // dereferencing copies it and leaves the caller's borrow intact (it
            // still needs the sample rate / channel count afterwards).
            *config,
            move |data: &mut [T], _| write_output_data(data, &shared),
            move |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|source| NativeAudioError::BuildStream { source })
}

#[cfg(not(target_arch = "wasm32"))]
fn write_output_data<T>(data: &mut [T], shared: &Arc<Mutex<AudioBuffer>>)
where
    T: SizedSample + FromSample<f32>,
{
    let Ok(mut shared) = shared.lock() else {
        for slot in data.iter_mut() {
            *slot = T::from_sample(0.0);
        }
        return;
    };

    shared.write_output(data);
}

/// Converts interleaved machine audio into interleaved host-output
/// audio, preserving channel positions when the source and output
/// channel counts match.
///
/// The conversion is intentionally simple: linear interpolation for
/// sample-rate conversion, channel copy for matching channel counts,
/// source-mono duplication for multi-channel output, and average
/// downmixing only when the host has fewer channels than the source.
#[must_use]
pub fn convert_audio_packet(
    samples: &[f32],
    source_rate: u32,
    source_channels: u8,
    output_rate: u32,
    output_channels: u16,
) -> Vec<f32> {
    let source_channel_count = usize::from(source_channels);
    let output_channel_count = usize::from(output_channels);
    if samples.is_empty()
        || source_rate == 0
        || output_rate == 0
        || source_channel_count == 0
        || output_channel_count == 0
    {
        return Vec::new();
    }

    let source_frames = samples.len() / source_channel_count;
    if source_frames == 0 {
        return Vec::new();
    }

    let output_frames = if source_rate == output_rate {
        source_frames
    } else {
        usize::try_from(
            (source_frames as u64 * u64::from(output_rate)).div_ceil(u64::from(source_rate)),
        )
        .unwrap_or(usize::MAX)
    };
    let mut converted = Vec::with_capacity(output_frames.saturating_mul(output_channel_count));

    if source_rate == output_rate {
        for frame in 0..source_frames {
            push_converted_frame(
                &mut converted,
                samples,
                frame,
                0.0,
                source_frames,
                source_channel_count,
                output_channel_count,
            );
        }
        return converted;
    }

    let step = f64::from(source_rate) / f64::from(output_rate);
    for frame in 0..output_frames {
        let position = frame as f64 * step;
        let index = position.floor() as usize;
        let frac = (position - index as f64) as f32;
        push_converted_frame(
            &mut converted,
            samples,
            index,
            frac,
            source_frames,
            source_channel_count,
            output_channel_count,
        );
    }

    converted
}

fn push_converted_frame(
    output: &mut Vec<f32>,
    samples: &[f32],
    frame_index: usize,
    frac: f32,
    source_frames: usize,
    source_channels: usize,
    output_channels: usize,
) {
    for output_channel in 0..output_channels {
        output.push(convert_channel(
            samples,
            frame_index,
            frac,
            source_frames,
            source_channels,
            output_channels,
            output_channel,
        ));
    }
}

fn convert_channel(
    samples: &[f32],
    frame_index: usize,
    frac: f32,
    source_frames: usize,
    source_channels: usize,
    output_channels: usize,
    output_channel: usize,
) -> f32 {
    if output_channels < source_channels {
        let mut sum = 0.0f32;
        for source_channel in 0..source_channels {
            sum += interpolate_channel(
                samples,
                frame_index,
                frac,
                source_frames,
                source_channels,
                source_channel,
            );
        }
        return sum / source_channels as f32;
    }

    if output_channels == source_channels
        || source_channels == 1
        || output_channel < source_channels
    {
        let source_channel = output_channel.min(source_channels.saturating_sub(1));
        return interpolate_channel(
            samples,
            frame_index,
            frac,
            source_frames,
            source_channels,
            source_channel,
        );
    }

    0.0
}

fn interpolate_channel(
    samples: &[f32],
    frame_index: usize,
    frac: f32,
    source_frames: usize,
    source_channels: usize,
    source_channel: usize,
) -> f32 {
    let last = source_frames.saturating_sub(1);
    let a = samples[(frame_index.min(last) * source_channels) + source_channel];
    let b = samples[((frame_index + 1).min(last) * source_channels) + source_channel];
    a + (b - a) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_stereo_when_output_is_stereo() {
        let converted = convert_audio_packet(&[0.25, -0.5, 0.75, -1.0], 48_000, 2, 48_000, 2);

        assert_eq!(converted, vec![0.25, -0.5, 0.75, -1.0]);
    }

    #[test]
    fn duplicates_mono_to_stereo() {
        let converted = convert_audio_packet(&[0.25, -0.5], 44_100, 1, 44_100, 2);

        assert_eq!(converted, vec![0.25, 0.25, -0.5, -0.5]);
    }

    #[test]
    fn fills_extra_output_channels_with_silence() {
        let converted = convert_audio_packet(&[0.25, -0.5], 48_000, 2, 48_000, 4);

        assert_eq!(converted, vec![0.25, -0.5, 0.0, 0.0]);
    }

    #[test]
    fn downmixes_stereo_to_mono() {
        let converted = convert_audio_packet(&[1.0, -0.5, 0.5, 0.25], 48_000, 2, 48_000, 1);

        assert_eq!(converted, vec![0.25, 0.375]);
    }

    #[test]
    fn resamples_each_channel_independently() {
        let converted = convert_audio_packet(&[0.0, 1.0, 1.0, 3.0], 2, 2, 4, 2);

        assert_eq!(converted, vec![0.0, 1.0, 0.5, 2.0, 1.0, 3.0, 1.0, 3.0]);
    }

    #[test]
    fn ignores_incomplete_source_frame() {
        let converted = convert_audio_packet(&[1.0, 2.0, 3.0], 48_000, 2, 48_000, 2);

        assert_eq!(converted, vec![1.0, 2.0]);
    }

    #[test]
    fn delayed_delivery_keeps_ordered_stereo_without_gaps() {
        // At 100 Hz, each stereo frame represents 10 ms. The 50 ms reserve
        // must cover two callbacks while the next packet is delayed.
        let shared = Arc::new(Mutex::new(AudioBuffer::new(100, 2, 250)));
        let frames: Vec<f32> = (1..=10).flat_map(|n| [n as f32, -(n as f32)]).collect();
        shared.lock().expect("buffer").push(&frames[..6]);
        let mut output = [99.0; 2];
        write_output_data(&mut output, &shared);
        assert_eq!(output, [0.0; 2], "wait for the reserve");
        shared.lock().expect("buffer").push(&frames[6..10]);
        let mut played = Vec::new();
        for _ in 0..3 {
            write_output_data(&mut output, &shared);
            played.extend_from_slice(&output);
        }
        shared.lock().expect("buffer").push(&frames[10..]);
        for _ in 0..7 {
            write_output_data(&mut output, &shared);
            played.extend_from_slice(&output);
        }
        assert_eq!(
            played, frames,
            "delayed packets must not insert silence or swap channels"
        );
    }

    #[test]
    fn reserve_uses_output_frames_and_never_exceeds_the_bound() {
        for (rate, channels, limit_ms, reserve, maximum) in [
            (48_000, 2, 250, 4_800, 24_000),
            (44_100, 1, 250, 2_205, 11_025),
            (32_000, 6, 20, 3_840, 3_840),
            (1, 2, 0, 2, 2),
        ] {
            let buffer = AudioBuffer::new(rate, channels, limit_ms);
            assert_eq!(buffer.reserve_samples, reserve);
            assert_eq!(buffer.max_samples, maximum);
            assert_eq!(buffer.max_samples % usize::from(channels), 0);
        }
    }

    #[test]
    fn underrun_preserves_tail_and_waits_for_a_fresh_reserve() {
        let mut buffer = AudioBuffer::new(100, 2, 250);
        buffer.push(&[1., -1., 2., -2., 3., -3., 4., -4., 5., -5.]);
        let mut output = [99.; 4];
        buffer.write_output(&mut output);
        assert_eq!(output, [1., -1., 2., -2.]);
        buffer.write_output(&mut output);
        assert_eq!(output, [3., -3., 4., -4.]);
        for _ in 0..3 {
            buffer.write_output(&mut output);
            assert_eq!(output, [0.; 4]);
        }
        buffer.push(&[6., -6., 7., -7., 8., -8.]);
        buffer.write_output(&mut output);
        assert_eq!(output, [0.; 4], "four frames cannot prime a 50 ms reserve");
        buffer.push(&[9., -9.]);
        buffer.write_output(&mut output);
        assert_eq!(
            output,
            [5., -5., 6., -6.],
            "resume with the retained stereo tail"
        );
    }

    #[test]
    fn clear_discards_stale_audio_and_reprimes_with_unsigned_silence() {
        let shared = Arc::new(Mutex::new(AudioBuffer::new(100, 1, 250)));
        shared.lock().expect("buffer").push(&[0.5; 5]);
        let mut output = [0u16; 1];
        write_output_data(&mut output, &shared);
        assert_eq!(output, [49_152]);
        {
            let mut buffer = shared.lock().expect("buffer");
            buffer.clear();
            buffer.push(&[-0.5; 4]);
        }
        write_output_data(&mut output, &shared);
        assert_eq!(output, [32_768], "unsigned PCM silence is its midpoint");
        shared.lock().expect("buffer").push(&[-0.5]);
        write_output_data(&mut output, &shared);
        assert_eq!(output, [16_384], "no samples from before clear may return");
    }

    #[test]
    fn large_callbacks_prime_then_play_and_oversized_callbacks_can_recover() {
        let mut buffer = AudioBuffer::new(100, 2, 100);
        buffer.push(&[0.5; 10]); // 50 ms, smaller than the 80 ms callback.
        let mut output = [99.; 16];
        buffer.write_output(&mut output);
        assert_eq!(output, [0.; 16]);
        buffer.push(&[0.5; 6]);
        buffer.write_output(&mut output);
        assert_eq!(output, [0.5; 16]);
        buffer.push(&[-0.5; 20]);
        let mut oversized = [99.; 24]; // 120 ms, above the caller's 100 ms cap.
        buffer.write_output(&mut oversized);
        assert_eq!(oversized[..20], [-0.5; 20]);
        assert_eq!(oversized[20..], [0.; 4]);
        buffer.push(&[0.25; 10]);
        let mut small = [99.; 2];
        buffer.write_output(&mut small);
        assert_eq!(
            small, [0.25; 2],
            "oversized callbacks must not prevent recovery"
        );
    }

    #[test]
    fn oversized_packets_drop_oldest_whole_frames_without_growing_storage() {
        let mut buffer = AudioBuffer::new(100, 3, 30);
        let capacity = buffer.samples.capacity();
        buffer.push(&[1., 2., 3., 4., 5., 6.]);
        buffer.push(&[7., 8., 9., 10., 11., 12.]);
        assert_eq!(
            buffer.samples.iter().copied().collect::<Vec<_>>(),
            [4., 5., 6., 7., 8., 9., 10., 11., 12.]
        );
        let packet: Vec<f32> = (0..30_001).map(|n| n as f32).collect();
        buffer.push(&packet);
        assert_eq!(buffer.samples.len(), 9);
        assert_eq!(buffer.samples.capacity(), capacity, "trim before extending");
        let mut output = [99.; 10];
        buffer.write_output(&mut output);
        assert_eq!(output[..9], packet[29_991..30_000]);
        assert_eq!(output[9], 0., "incomplete callback frame is silent");
        let mut too_short = [99.; 2];
        buffer.write_output(&mut too_short);
        assert_eq!(too_short, [0.; 2]);
    }
}
