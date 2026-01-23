//! Audio output handling.
//!
//! Provides a generic audio output that works with any Machine implementation.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, Stream, StreamConfig};
use ringbuf::{
    HeapRb,
    traits::{Consumer, Producer, Split},
};

/// Audio output handler that manages the cpal stream and ring buffer.
pub struct AudioOutput {
    _stream: Stream,
    producer: ringbuf::HeapProd<f32>,
}

impl AudioOutput {
    /// Create a new audio output stream.
    ///
    /// Returns None if no audio device is available.
    pub fn new(sample_rate: u32, samples_per_frame: usize) -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;

        let config = StreamConfig {
            channels: 1,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Rest level when audio is silent (used for underrun handling and pre-fill)
        let rest_level = 0.0;

        // Ring buffer sized for ~8 frames of audio (provides buffer against timing jitter)
        let ring = HeapRb::<f32>::new(samples_per_frame * 8);
        let (mut producer, mut consumer) = ring.split();

        // Pre-fill buffer with 4 frames of silence to prevent startup underrun
        for _ in 0..samples_per_frame * 4 {
            let _ = producer.try_push(rest_level);
        }

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for sample in data.iter_mut() {
                        // Pop from ring buffer, use rest level on underrun to avoid clicks
                        *sample = consumer.try_pop().unwrap_or(rest_level);
                    }
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )
            .ok()?;

        stream.play().ok()?;

        Some(Self {
            _stream: stream,
            producer,
        })
    }

    /// Push a frame's worth of audio samples to the ring buffer.
    ///
    /// Blocks if the buffer is full, which creates back-pressure that
    /// naturally paces the emulation to match audio consumption rate.
    pub fn push_samples(&mut self, samples: &[f32]) {
        for &sample in samples {
            // Block if buffer is full - this synchronizes emulation to audio rate
            while self.producer.try_push(sample).is_err() {
                std::thread::yield_now();
            }
        }
    }
}
