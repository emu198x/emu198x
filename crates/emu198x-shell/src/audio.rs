//! Shared host-side audio conversion helpers.

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
        let max_samples = usize::try_from(
            (u64::from(config.sample_rate) * u64::from(config.channels) * u64::from(max_buffer_ms))
                / 1_000,
        )
        .unwrap_or(usize::MAX)
        .max(1);
        let shared = Arc::new(Mutex::new(AudioBuffer::new(max_samples)));
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
            shared.samples.clear();
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
    max_samples: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl AudioBuffer {
    fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        self.samples.extend(samples.iter().copied());
        while self.samples.len() > self.max_samples {
            let _ = self.samples.pop_front();
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

    for slot in data.iter_mut() {
        let sample = shared.samples.pop_front().unwrap_or(0.0);
        *slot = T::from_sample(sample);
    }
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
    fn audio_buffer_drops_oldest_samples_when_full() {
        let mut buffer = AudioBuffer::new(3);

        buffer.push(&[1.0, 2.0]);
        buffer.push(&[3.0, 4.0]);

        assert_eq!(
            buffer.samples.into_iter().collect::<Vec<_>>(),
            vec![2.0, 3.0, 4.0]
        );
    }
}
