//! MOS 6581 / 8580 SID (Sound Interface Device).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

mod envelope;
mod filter;
mod voice;

pub use envelope::{Envelope, Phase};
pub use filter::Filter;
pub use voice::Voice;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidModel {
    #[default]
    Mos6581,
    Mos8580,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sid6581 {
    pub model: SidModel,
    pub voices: [Voice; 3],
    pub envelopes: [Envelope; 3],
    pub filter: Filter,
    pub volume: u8,
    pub voice3_off: bool,
    pub potx: u8,
    pub poty: u8,
    accumulator: f32,
    channel_accumulators: [f32; 3],
    sample_count: u32,
    ticks_per_sample: f32,
    cpu_freq: u64,
    output_sample_rate: u32,
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
            ticks_per_sample: cpu_frequency as f32 / output_sample_rate as f32,
            cpu_freq: cpu_frequency,
            output_sample_rate,
            buffer: Vec::with_capacity(output_sample_rate as usize / 50 + 1),
            channel_buffers: std::array::from_fn(|_| {
                Vec::with_capacity(output_sample_rate as usize / 50 + 1)
            }),
        }
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
            0x15 => {
                self.filter.cutoff = (self.filter.cutoff & 0x07F8) | u16::from(value & 0x07);
            }
            0x16 => {
                self.filter.cutoff = (self.filter.cutoff & 0x0007) | (u16::from(value) << 3);
            }
            0x17 => {
                self.filter.resonance = (value >> 4) & 0x0F;
                self.filter.routing = value & 0x07;
                self.filter.ext_in = value & 0x08 != 0;
            }
            0x18 => {
                self.volume = value & 0x0F;
                self.filter.mode = value & 0x70;
                self.voice3_off = value & 0x80 != 0;
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

        let mut filtered_sum = 0.0;
        let mut direct_sum = 0.0;
        let mut voice_normalised = [0.0_f32; 3];

        for index in 0..3 {
            let waveform = self.voices[index].waveform_output(ring_mod_msb[index], self.model);
            let envelope = self.envelopes[index].level;
            let centred = f32::from(waveform as i16 - 2048);
            let amplitude = centred * f32::from(envelope) / 255.0;
            voice_normalised[index] = amplitude / 2048.0;

            if index == 2 && self.voice3_off {
                continue;
            }

            if self.filter.voice_routed(index) {
                filtered_sum += amplitude;
            } else {
                direct_sum += amplitude;
            }
        }

        let filter_output = self.filter.clock(filtered_sum);
        let mixed = (filter_output + direct_sum) * f32::from(self.volume) / 15.0;
        let normalised = mixed / 6144.0;

        self.accumulator += normalised;
        for (index, sample) in voice_normalised.iter().copied().enumerate() {
            self.channel_accumulators[index] += sample;
        }
        self.sample_count += 1;

        if self.sample_count as f32 >= self.ticks_per_sample {
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
    fn silent_when_no_voices_active() {
        let mut sid = Sid6581::new(985_248, 48_000);
        for _ in 0..19_656 {
            sid.tick();
        }
        let buffer = sid.take_buffer();
        assert!(!buffer.is_empty(), "should emit samples even in silence");
        for &sample in &buffer {
            assert!(sample.abs() < 1e-6, "expected silence, got {sample}");
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

            for _ in 0..60_000 {
                sid.tick();
            }

            let buffer = sid.take_buffer();
            let settled = &buffer[200.min(buffer.len())..];
            let sum_sq: f32 = settled.iter().map(|sample| sample * sample).sum();
            (sum_sq / settled.len() as f32).sqrt()
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
