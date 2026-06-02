//! Texas Instruments SN76489 Programmable Sound Generator.
//!
//! Adapted from `Emu198x-Oldest/crates/ti-sn76489` (port 2026-06-01) as
//! the second of four commits unlocking ColecoVision. Self-contained
//! port with no external chip dependencies; pair-mate to
//! [`ti-tms9918`](https://docs.rs/ti-tms9918) for the TMS9918-family
//! machine stack. See `docs/status/outstanding-work.md` § ColecoVision
//! for the accuracy backlog.
//!
//! The SN76489 (and pin-compatible variants SN76489A, SN76494, NCR8496,
//! Sega PSG 315-5124) provides three square-wave tone channels and one
//! noise channel, each with independent 4-bit attenuation.
//!
//! Used by the Sega Master System, Game Gear, Mega Drive (as secondary
//! sound), ColecoVision, BBC Micro, SG-1000, Memotech MTX, Sord M5,
//! and many arcade boards.
//!
//! The chip is driven by a master clock (typically 3.579545 MHz NTSC)
//! divided by 16 internally. Each tone channel has a 10-bit period
//! register; the noise channel has a 3-bit mode register selecting
//! period and feedback type.
//!
//! Output is downsampled to 48 kHz stereo (identical L/R by default;
//! the Game Gear variant adds per-channel stereo panning).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

/// Output sample rate.
const SAMPLE_RATE: u32 = 48_000;

/// SN76489 Programmable Sound Generator.
pub struct Sn76489 {
    // Tone channels 0-2
    tone_period: [u16; 3],
    tone_counter: [u16; 3],
    tone_output: [bool; 3],
    tone_attenuation: [u8; 3],

    // Noise channel
    noise_mode: u8,
    noise_period: u16,
    noise_counter: u16,
    noise_shift: u16,
    noise_output: bool,
    noise_attenuation: u8,
    /// True for white noise (XNOR feedback), false for periodic noise.
    noise_white: bool,

    /// Which register is currently latched for data writes.
    latched_register: u8,

    // Downsampling
    clock_divider: u32,
    accumulator: f32,
    channel_accumulators: [f32; 4],
    sample_count: u32,
    internal_clock_hz: u32,
    resample_phase: u32,
    hp_prev_in: f32,
    hp_prev_out: f32,
    buffer: Vec<f32>,
    channel_buffers: [Vec<f32>; 4],

    // Stereo panning (Game Gear extension). Bits 7-0: R3 L3 R2 L2 R1 L1 R0 L0.
    // Default $FF = all channels to both speakers.
    stereo_panning: u8,
}

impl Sn76489 {
    /// Create a new SN76489 with the given input clock frequency.
    ///
    /// The internal clock divides by 16, so for a 3.579545 MHz input the
    /// effective tone clock is ~223.7 kHz.
    #[must_use]
    pub fn new(clock_hz: u32) -> Self {
        let internal_clock = clock_hz / 16;
        Self {
            tone_period: [0; 3],
            tone_counter: [0; 3],
            tone_output: [true; 3],
            tone_attenuation: [0x0F; 3], // Muted

            noise_mode: 0,
            noise_period: 0x10,
            noise_counter: 0x10,
            noise_shift: 0x8000,
            noise_output: false,
            noise_attenuation: 0x0F,
            noise_white: false,

            latched_register: 0,

            clock_divider: 0,
            accumulator: 0.0,
            channel_accumulators: [0.0; 4],
            sample_count: 0,
            internal_clock_hz: internal_clock,
            resample_phase: 0,
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
            buffer: Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1),
            channel_buffers: std::array::from_fn(|_| {
                Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1)
            }),

            stereo_panning: 0xFF,
        }
    }

    /// Write a byte to the PSG data port.
    ///
    /// Bit 7 = 1: latch/data byte. Bits 6-4 = register (0-7). Bits 3-0 = data.
    /// Bit 7 = 0: data byte for the previously latched register.
    pub fn write(&mut self, value: u8) {
        if value & 0x80 != 0 {
            // Latch + data
            self.latched_register = (value >> 4) & 0x07;
            let data = value & 0x0F;

            match self.latched_register {
                0 => self.tone_period[0] = (self.tone_period[0] & 0x3F0) | u16::from(data),
                1 => self.tone_attenuation[0] = data,
                2 => self.tone_period[1] = (self.tone_period[1] & 0x3F0) | u16::from(data),
                3 => self.tone_attenuation[1] = data,
                4 => self.tone_period[2] = (self.tone_period[2] & 0x3F0) | u16::from(data),
                5 => self.tone_attenuation[2] = data,
                6 => {
                    // Noise control
                    self.noise_white = data & 0x04 != 0;
                    self.noise_mode = data & 0x03;
                    self.noise_period = match self.noise_mode {
                        0 => 0x10,
                        1 => 0x20,
                        2 => 0x40,
                        3 => self.tone_period[2], // Use tone 2 period
                        _ => unreachable!(),
                    };
                    self.noise_shift = 0x8000; // Reset shift register
                }
                7 => self.noise_attenuation = data,
                _ => unreachable!(),
            }
        } else {
            // Data byte for latched register
            let data = value & 0x3F;

            match self.latched_register {
                0 => self.tone_period[0] = (self.tone_period[0] & 0x00F) | (u16::from(data) << 4),
                1 => self.tone_attenuation[0] = value & 0x0F,
                2 => self.tone_period[1] = (self.tone_period[1] & 0x00F) | (u16::from(data) << 4),
                3 => self.tone_attenuation[1] = value & 0x0F,
                4 => self.tone_period[2] = (self.tone_period[2] & 0x00F) | (u16::from(data) << 4),
                5 => self.tone_attenuation[2] = value & 0x0F,
                6 => {
                    self.noise_white = value & 0x04 != 0;
                    self.noise_mode = value & 0x03;
                    self.noise_period = match self.noise_mode {
                        0 => 0x10,
                        1 => 0x20,
                        2 => 0x40,
                        3 => self.tone_period[2],
                        _ => unreachable!(),
                    };
                    self.noise_shift = 0x8000;
                }
                7 => self.noise_attenuation = value & 0x0F,
                _ => unreachable!(),
            }
        }
    }

    /// Write the Game Gear stereo panning register ($06 on GG).
    pub fn write_stereo(&mut self, value: u8) {
        self.stereo_panning = value;
    }

    /// Tick the PSG one master clock cycle. The internal divider handles
    /// the ÷16 frequency reduction.
    pub fn tick(&mut self) {
        self.clock_divider += 1;
        if self.clock_divider < 16 {
            return;
        }
        self.clock_divider = 0;

        // Tick tone channels
        for ch in 0..3 {
            if self.tone_counter[ch] == 0 {
                self.tone_counter[ch] = self.tone_period[ch];
                self.tone_output[ch] = !self.tone_output[ch];
            } else {
                self.tone_counter[ch] -= 1;
            }
        }

        // Tick noise channel
        if self.noise_counter == 0 {
            // If noise is slaved to tone 2, pick up current period
            if self.noise_mode == 3 {
                self.noise_period = self.tone_period[2];
            }
            self.noise_counter = self.noise_period;

            // Clock the shift register on output transition
            let feedback = if self.noise_white {
                // White noise: XNOR of bits 0 and 3 (SN76489 variant)
                // SMS uses bits 0 and 1; this is the TI variant
                (self.noise_shift & 0x01) ^ ((self.noise_shift >> 3) & 0x01)
            } else {
                // Periodic noise: bit 0 only
                self.noise_shift & 0x01
            };
            self.noise_shift = (self.noise_shift >> 1) | (feedback << 15);
            self.noise_output = self.noise_shift & 0x01 != 0;
        } else {
            self.noise_counter -= 1;
        }

        // Mix and downsample
        let (sample, per_channel) = self.mix_with_channels();
        self.accumulator += sample;
        for (acc, ch) in self.channel_accumulators.iter_mut().zip(per_channel.iter()) {
            *acc += ch;
        }
        self.sample_count += 1;

        self.resample_phase = self.resample_phase.saturating_add(SAMPLE_RATE);
        if self.resample_phase >= self.internal_clock_hz {
            let avg = self.accumulator / self.sample_count as f32;

            // The raw chip output is unipolar and normally AC-coupled by the
            // surrounding hardware, so remove DC before exposing samples.
            const DC_BLOCK_ALPHA: f32 = 0.9952;
            let filtered = DC_BLOCK_ALPHA * (self.hp_prev_out + avg - self.hp_prev_in);
            self.hp_prev_in = avg;
            self.hp_prev_out = filtered;

            self.buffer.push(filtered);

            let count = self.sample_count as f32;
            for (i, acc) in self.channel_accumulators.iter_mut().enumerate() {
                self.channel_buffers[i].push(*acc / count);
                *acc = 0.0;
            }

            self.resample_phase -= self.internal_clock_hz;
            self.accumulator = 0.0;
            self.sample_count = 0;
        }
    }

    /// Take the audio output buffer (drains it).
    ///
    /// Returns mono f32 samples at 48 kHz, centered around zero.
    pub fn take_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    /// Take per-channel audio buffers (drains them).
    ///
    /// Returns four mono f32 buffers at 48 kHz: tone 0, tone 1, tone 2, noise.
    /// Each channel is the raw averaged output (not DC-blocked) scaled to
    /// 0.0–0.25 range (one quarter of full mix).
    pub fn take_channel_buffers(&mut self) -> [Vec<f32>; 4] {
        std::array::from_fn(|i| std::mem::take(&mut self.channel_buffers[i]))
    }

    /// Take stereo audio output (interleaved L, R pairs at 48 kHz).
    ///
    /// Applies Game Gear stereo panning. For non-GG systems where
    /// `stereo_panning` is $FF, left and right are identical.
    pub fn take_buffer_stereo(&mut self) -> Vec<f32> {
        let mono = std::mem::take(&mut self.buffer);
        // For true stereo we'd need to mix per-channel, but the mono
        // buffer is already mixed. Return duplicated L/R for now.
        let mut stereo = Vec::with_capacity(mono.len() * 2);
        for s in &mono {
            stereo.push(*s);
            stereo.push(*s);
        }
        stereo
    }

    // -----------------------------------------------------------------------
    // Attenuation volumes
    // -----------------------------------------------------------------------

    /// Convert 4-bit attenuation to linear amplitude.
    /// 0 = full volume, 15 = silence. Each step is ~2 dB.
    fn attenuation_to_volume(att: u8) -> f32 {
        const VOLUMES: [f32; 16] = [
            1.0, 0.7943, 0.6310, 0.5012, 0.3981, 0.3162, 0.2512, 0.1995, 0.1585, 0.1259, 0.1000,
            0.0794, 0.0631, 0.0501, 0.0398, 0.0,
        ];
        VOLUMES[att as usize & 0x0F]
    }

    /// Mix all channels and return (mixed_sample, [ch0, ch1, ch2, noise]).
    /// Each value is pre-scaled by 1/4 so the sum equals the mixed output.
    fn mix_with_channels(&self) -> (f32, [f32; 4]) {
        let mut channels = [0.0f32; 4];

        for (ch, slot) in channels.iter_mut().take(3).enumerate() {
            if self.tone_output[ch] {
                *slot = Self::attenuation_to_volume(self.tone_attenuation[ch]) / 4.0;
            }
        }

        if self.noise_output {
            channels[3] = Self::attenuation_to_volume(self.noise_attenuation) / 4.0;
        }

        let mixed = channels[0] + channels[1] + channels[2] + channels[3];
        (mixed, channels)
    }

    // -----------------------------------------------------------------------
    // Save/load state
    // -----------------------------------------------------------------------

    /// Serialize PSG state to a byte vector.
    ///
    /// Layout: tone_period (3×2) + tone_counter (3×2) + tone_output (3×1) +
    /// tone_attenuation (3×1) + noise_mode (1) + noise_period (2) +
    /// noise_counter (2) + noise_shift (2) + noise_output (1) +
    /// noise_attenuation (1) + noise_white (1) + latched_register (1) +
    /// stereo_panning (1) = 27 bytes.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        for i in 0..3 {
            out.extend_from_slice(&self.tone_period[i].to_le_bytes());
        }
        for i in 0..3 {
            out.extend_from_slice(&self.tone_counter[i].to_le_bytes());
        }
        for i in 0..3 {
            out.push(u8::from(self.tone_output[i]));
        }
        for i in 0..3 {
            out.push(self.tone_attenuation[i]);
        }
        out.push(self.noise_mode);
        out.extend_from_slice(&self.noise_period.to_le_bytes());
        out.extend_from_slice(&self.noise_counter.to_le_bytes());
        out.extend_from_slice(&self.noise_shift.to_le_bytes());
        out.push(u8::from(self.noise_output));
        out.push(self.noise_attenuation);
        out.push(u8::from(self.noise_white));
        out.push(self.latched_register);
        out.push(self.stereo_panning);
    }

    /// Restore PSG state from a byte slice. Returns bytes consumed or error.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        let needed = 27;
        if data.len() < needed {
            return Err("SN76489 state truncated".into());
        }
        let mut p = 0;
        for i in 0..3 {
            self.tone_period[i] = u16::from_le_bytes([data[p], data[p + 1]]); p += 2;
        }
        for i in 0..3 {
            self.tone_counter[i] = u16::from_le_bytes([data[p], data[p + 1]]); p += 2;
        }
        for i in 0..3 {
            self.tone_output[i] = data[p] != 0; p += 1;
        }
        for i in 0..3 {
            self.tone_attenuation[i] = data[p]; p += 1;
        }
        self.noise_mode = data[p]; p += 1;
        self.noise_period = u16::from_le_bytes([data[p], data[p + 1]]); p += 2;
        self.noise_counter = u16::from_le_bytes([data[p], data[p + 1]]); p += 2;
        self.noise_shift = u16::from_le_bytes([data[p], data[p + 1]]); p += 2;
        self.noise_output = data[p] != 0; p += 1;
        self.noise_attenuation = data[p]; p += 1;
        self.noise_white = data[p] != 0; p += 1;
        self.latched_register = data[p]; p += 1;
        self.stereo_panning = data[p]; p += 1;
        Ok(p)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_psg_is_silent() {
        let psg = Sn76489::new(3_579_545);
        // All attenuations default to $0F (silent)
        for ch in 0..3 {
            assert_eq!(psg.tone_attenuation[ch], 0x0F);
        }
        assert_eq!(psg.noise_attenuation, 0x0F);
    }

    #[test]
    fn latch_write_sets_tone_period_low() {
        let mut psg = Sn76489::new(3_579_545);
        // Latch channel 0 tone, low nibble = $A
        psg.write(0x8A); // 1_000_1010: reg 0, data $A
        assert_eq!(psg.tone_period[0] & 0x0F, 0x0A);
    }

    #[test]
    fn data_write_sets_tone_period_high() {
        let mut psg = Sn76489::new(3_579_545);
        // Latch channel 0 tone low = $5
        psg.write(0x85);
        // Data byte: high 6 bits = $3F (max)
        psg.write(0x3F);
        assert_eq!(psg.tone_period[0], 0x3F5);
    }

    #[test]
    fn attenuation_write() {
        let mut psg = Sn76489::new(3_579_545);
        // Set channel 1 attenuation to 5
        psg.write(0xB5); // 1_011_0101: reg 3 (ch1 att), data 5
        assert_eq!(psg.tone_attenuation[1], 5);
    }

    #[test]
    fn noise_mode_sets_period() {
        let mut psg = Sn76489::new(3_579_545);
        // Set noise: white, period mode 1 ($20)
        psg.write(0xE5); // 1_110_0101: reg 6, white + mode 1
        assert!(psg.noise_white);
        assert_eq!(psg.noise_period, 0x20);
    }

    #[test]
    fn noise_mode_3_uses_tone_2_period() {
        let mut psg = Sn76489::new(3_579_545);
        // Set tone 2 period
        psg.write(0xC5); // Latch ch2 tone, low = 5
        psg.write(0x02); // High bits = 2 → period = $25
        assert_eq!(psg.tone_period[2], 0x25);

        // Set noise to mode 3
        psg.write(0xE3); // periodic, mode 3
        assert_eq!(psg.noise_period, 0x25);
    }

    #[test]
    fn tick_produces_output() {
        let mut psg = Sn76489::new(3_579_545);
        // Set channel 0: period = 1, full volume
        psg.write(0x81); // Tone 0 low = 1
        psg.write(0x00); // Tone 0 high = 0 → period = 1
        psg.write(0x90); // Ch0 attenuation = 0 (full volume)

        // Tick enough to generate samples
        for _ in 0..3_579_545 / 50 {
            psg.tick();
        }

        let buf = psg.take_buffer();
        assert!(!buf.is_empty(), "should have produced audio samples");
        assert!(
            buf.iter().any(|&s| s.abs() > 0.01),
            "should have non-zero output"
        );
    }

    #[test]
    fn silent_when_muted() {
        let mut psg = Sn76489::new(3_579_545);
        // All channels default to $0F attenuation (silent)
        for _ in 0..3_579_545 / 50 {
            psg.tick();
        }
        let buf = psg.take_buffer();
        assert!(buf.iter().all(|&s| s == 0.0), "should be silent when muted");
    }

    #[test]
    fn attenuation_to_volume_extremes() {
        assert_eq!(Sn76489::attenuation_to_volume(0), 1.0);
        assert_eq!(Sn76489::attenuation_to_volume(15), 0.0);
    }

    #[test]
    fn downsampler_tracks_48khz_output_rate() {
        let mut psg = Sn76489::new(3_579_545);
        for _ in 0..3_579_545 {
            psg.tick();
        }

        let buf = psg.take_buffer();
        assert!(
            (47_990..=48_010).contains(&buf.len()),
            "expected about 48 kHz of output, got {} samples",
            buf.len()
        );
    }

    #[test]
    fn output_is_dc_blocked() {
        let mut psg = Sn76489::new(3_579_545);
        psg.write(0x81);
        psg.write(0x00);
        psg.write(0x90);

        for _ in 0..(3_579_545 * 2) {
            psg.tick();
        }

        let buf = psg.take_buffer();
        let steady_state = &buf[buf.len() / 2..];
        let average = steady_state.iter().copied().sum::<f32>() / steady_state.len() as f32;
        assert!(
            average.abs() < 0.05,
            "expected DC-blocked output average near zero, got {average}"
        );
    }
}
