use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::AudioFrame;

const AUDIO_CHANNELS: usize = 2;
const AUDIO_TARGET_BATCHES: usize = 3;
const AUDIO_MAX_BATCHES: usize = 5;
const AUDIO_CALLBACK_TARGET_MULTIPLIER: usize = 2;
const AUDIO_CALLBACK_MAX_MULTIPLIER: usize = 3;

struct AudioQueue {
    samples: VecDeque<f32>,
    target_samples: usize,
    max_samples: usize,
    primed: bool,
}

impl AudioQueue {
    fn new(sample_rate: u32, frame_duration: Duration, callback_frames: Option<u32>) -> Self {
        let batch_samples = samples_per_duration(sample_rate, frame_duration);
        let callback_samples = callback_frames
            .map(|frames| frames as usize * AUDIO_CHANNELS)
            .unwrap_or(0);
        let target_samples = (batch_samples * AUDIO_TARGET_BATCHES)
            .max(callback_samples.saturating_mul(AUDIO_CALLBACK_TARGET_MULTIPLIER))
            .max(AUDIO_CHANNELS);
        let max_samples = (batch_samples * AUDIO_MAX_BATCHES)
            .max(callback_samples.saturating_mul(AUDIO_CALLBACK_MAX_MULTIPLIER))
            .max(target_samples);

        Self {
            samples: VecDeque::with_capacity(max_samples),
            target_samples,
            max_samples,
            primed: false,
        }
    }

    #[cfg(test)]
    fn with_thresholds(target_samples: usize, max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            target_samples,
            max_samples,
            primed: false,
        }
    }

    fn clear(&mut self) {
        self.samples.clear();
        self.primed = false;
    }

    fn push_frames(&mut self, frames: &[AudioFrame]) {
        if frames.is_empty() {
            return;
        }

        for &[left, right] in frames {
            self.samples.push_back(left);
            self.samples.push_back(right);
        }

        if self.samples.len() > self.max_samples {
            let samples_to_drop = self.samples.len().saturating_sub(self.target_samples);
            for _ in 0..samples_to_drop {
                let _ = self.samples.pop_front();
            }
        }
    }

    fn fill_output<T>(&mut self, data: &mut [T], silence: T, mut convert: impl FnMut(f32) -> T)
    where
        T: Copy,
    {
        let needed_samples = data.len();
        let start_threshold = self.target_samples.max(needed_samples);

        if !self.primed {
            if self.samples.len() < start_threshold {
                data.fill(silence);
                return;
            }
            self.primed = true;
        }

        if self.samples.len() < needed_samples {
            self.primed = false;
            data.fill(silence);
            return;
        }

        for sample in data {
            let value = self.samples.pop_front().unwrap_or(0.0);
            *sample = convert(value);
        }
    }
}

pub(crate) struct AudioOutput {
    _stream: cpal::Stream,
    queue: Arc<Mutex<AudioQueue>>,
}

impl AudioOutput {
    pub(crate) fn new(sample_rate: u32, frame_duration: Duration) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| String::from("no default audio output device"))?;

        let supported_configs = device
            .supported_output_configs()
            .map_err(|e| format!("failed to query output configs: {e}"))?;

        let desired = supported_configs
            .filter(|cfg| cfg.channels() == AUDIO_CHANNELS as u16)
            .find(|cfg| {
                let min = cfg.min_sample_rate().0;
                let max = cfg.max_sample_rate().0;
                min <= sample_rate && sample_rate <= max
            })
            .ok_or_else(|| {
                format!(
                    "no {}-channel output config supports {} Hz",
                    AUDIO_CHANNELS, sample_rate
                )
            })?;

        let sample_format = desired.sample_format();
        let config = desired
            .with_sample_rate(cpal::SampleRate(sample_rate))
            .config();
        let callback_frames = match config.buffer_size {
            cpal::BufferSize::Fixed(frames) => Some(frames),
            cpal::BufferSize::Default => None,
        };

        let queue = Arc::new(Mutex::new(AudioQueue::new(
            sample_rate,
            frame_duration,
            callback_frames,
        )));
        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let callback_queue = Arc::clone(&queue);
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [f32], _| write_audio_data_f32(data, &callback_queue),
                        |err| eprintln!("Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("failed to build f32 audio stream: {e}"))?
            }
            cpal::SampleFormat::I16 => {
                let callback_queue = Arc::clone(&queue);
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [i16], _| write_audio_data_i16(data, &callback_queue),
                        |err| eprintln!("Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("failed to build i16 audio stream: {e}"))?
            }
            cpal::SampleFormat::U16 => {
                let callback_queue = Arc::clone(&queue);
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [u16], _| write_audio_data_u16(data, &callback_queue),
                        |err| eprintln!("Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("failed to build u16 audio stream: {e}"))?
            }
            other => {
                return Err(format!("unsupported audio sample format: {other:?}"));
            }
        };

        stream
            .play()
            .map_err(|e| format!("failed to start audio stream: {e}"))?;

        Ok(Self {
            _stream: stream,
            queue,
        })
    }

    pub(crate) fn clear(&self) {
        let Ok(mut queue) = self.queue.lock() else {
            return;
        };
        queue.clear();
    }

    pub(crate) fn push_frames(&self, frames: &[AudioFrame]) {
        let Ok(mut queue) = self.queue.lock() else {
            return;
        };
        queue.push_frames(frames);
    }
}

fn samples_per_duration(sample_rate: u32, duration: Duration) -> usize {
    let frames = (duration.as_secs_f64() * f64::from(sample_rate)).ceil() as usize;
    frames.max(1) * AUDIO_CHANNELS
}

fn write_audio_data_f32(data: &mut [f32], queue: &Arc<Mutex<AudioQueue>>) {
    let Ok(mut guard) = queue.lock() else {
        data.fill(0.0);
        return;
    };
    guard.fill_output(data, 0.0, |sample| sample);
}

fn write_audio_data_i16(data: &mut [i16], queue: &Arc<Mutex<AudioQueue>>) {
    let Ok(mut guard) = queue.lock() else {
        data.fill(0);
        return;
    };
    guard.fill_output(data, 0, |sample| {
        let value = sample.clamp(-1.0, 1.0);
        (value * f32::from(i16::MAX)) as i16
    });
}

fn write_audio_data_u16(data: &mut [u16], queue: &Arc<Mutex<AudioQueue>>) {
    let Ok(mut guard) = queue.lock() else {
        data.fill(u16::MAX / 2);
        return;
    };
    guard.fill_output(data, u16::MAX / 2, |sample| {
        let value = sample.clamp(-1.0, 1.0);
        let scaled = ((value * 0.5) + 0.5) * f32::from(u16::MAX);
        scaled.round() as u16
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_waits_for_target_before_playback() {
        let queue = Arc::new(Mutex::new(AudioQueue::with_thresholds(4, 8)));
        {
            let Ok(mut guard) = queue.lock() else {
                panic!("queue lock poisoned");
            };
            guard.push_frames(&[[0.25, -0.25]]);
        }

        let mut data = [1.0f32; 2];
        write_audio_data_f32(&mut data, &queue);
        assert_eq!(data, [0.0, 0.0]);

        {
            let Ok(mut guard) = queue.lock() else {
                panic!("queue lock poisoned");
            };
            guard.push_frames(&[[0.5, -0.5]]);
        }

        write_audio_data_f32(&mut data, &queue);
        assert_eq!(data, [0.25, -0.25]);
    }

    #[test]
    fn queue_drops_oldest_samples_when_latency_exceeds_limit() {
        let queue = Arc::new(Mutex::new(AudioQueue::with_thresholds(4, 6)));
        {
            let Ok(mut guard) = queue.lock() else {
                panic!("queue lock poisoned");
            };
            guard.push_frames(&[[0.1, 0.2], [0.3, 0.4], [0.5, 0.6], [0.7, 0.8]]);
        }

        let mut data = [0.0f32; 4];
        write_audio_data_f32(&mut data, &queue);
        assert_eq!(data, [0.5, 0.6, 0.7, 0.8]);
    }

    #[test]
    fn queue_reprimes_after_underrun() {
        let queue = Arc::new(Mutex::new(AudioQueue::with_thresholds(4, 8)));
        {
            let Ok(mut guard) = queue.lock() else {
                panic!("queue lock poisoned");
            };
            guard.push_frames(&[[0.1, 0.2], [0.3, 0.4]]);
        }

        let mut first = [0.0f32; 2];
        write_audio_data_f32(&mut first, &queue);
        assert_eq!(first, [0.1, 0.2]);

        let mut underrun = [1.0f32; 4];
        write_audio_data_f32(&mut underrun, &queue);
        assert_eq!(underrun, [0.0, 0.0, 0.0, 0.0]);

        {
            let Ok(mut guard) = queue.lock() else {
                panic!("queue lock poisoned");
            };
            guard.push_frames(&[[0.5, 0.6], [0.7, 0.8]]);
        }

        let mut resumed = [0.0f32; 4];
        write_audio_data_f32(&mut resumed, &queue);
        assert_eq!(resumed, [0.3, 0.4, 0.5, 0.6]);
    }
}
