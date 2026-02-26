//! MOS 6581 SID (Sound Interface Device) emulator.
//!
//! The SID has three voices, each with a 24-bit phase-accumulator oscillator,
//! four waveform generators, an ADSR envelope, and a shared multi-mode
//! state-variable filter. All components tick at the C64 CPU rate (985,248 Hz
//! PAL) and the output is downsampled to 48 kHz.
//!
//! # Register map (29 registers, $D400–$D41C)
//!
//! | Addr | Register          |
//! |------|-------------------|
//! | $00  | Voice 1 freq lo   |
//! | $01  | Voice 1 freq hi   |
//! | $02  | Voice 1 PW lo     |
//! | $03  | Voice 1 PW hi     |
//! | $04  | Voice 1 control   |
//! | $05  | Voice 1 AD        |
//! | $06  | Voice 1 SR        |
//! | $07–$0D | Voice 2 (same layout) |
//! | $0E–$14 | Voice 3 (same layout) |
//! | $15  | Filter cutoff lo  |
//! | $16  | Filter cutoff hi  |
//! | $17  | Filter routing + resonance |
//! | $18  | Volume + filter mode |
//! | $19  | Paddle X (read-only) |
//! | $1A  | Paddle Y (read-only) |
//! | $1B  | OSC3 output (read-only) |
//! | $1C  | ENV3 output (read-only) |

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

mod envelope;
mod filter;
mod voice;

pub use envelope::{Envelope, Phase};
pub use filter::Filter;
pub use voice::Voice;

/// MOS 6581 SID chip.
pub struct Sid6581 {
    /// Three voices.
    pub voices: [Voice; 3],
    /// Three envelope generators (one per voice).
    pub envelopes: [Envelope; 3],
    /// Multi-mode filter.
    pub filter: Filter,
    /// 4-bit master volume (0–15).
    pub volume: u8,
    /// Voice 3 mute (bit 7 of $D418): if set, voice 3 is excluded from
    /// the audio mix but its oscillator and envelope still run (useful as
    /// a modulation source).
    pub voice3_off: bool,

    // Downsampling state
    /// Accumulated mixed output for downsampling.
    accumulator: f32,
    /// Number of CPU ticks accumulated.
    sample_count: u32,
    /// CPU ticks per output sample (e.g., 985,248 / 48,000 ≈ 20.5).
    ticks_per_sample: f32,
    /// Output audio buffer (mono f32, -1.0 to 1.0).
    buffer: Vec<f32>,
}

impl Sid6581 {
    /// Create a new SID chip.
    ///
    /// `cpu_frequency` is the master clock rate in Hz (985,248 for PAL C64).
    /// `output_sample_rate` is the audio output rate in Hz (typically 48,000).
    #[must_use]
    pub fn new(cpu_frequency: u32, output_sample_rate: u32) -> Self {
        Self {
            voices: [Voice::new(), Voice::new(), Voice::new()],
            envelopes: [Envelope::new(), Envelope::new(), Envelope::new()],
            filter: Filter::new(),
            volume: 0,
            voice3_off: false,
            accumulator: 0.0,
            sample_count: 0,
            ticks_per_sample: cpu_frequency as f32 / output_sample_rate as f32,
            buffer: Vec::with_capacity(output_sample_rate as usize / 50 + 1),
        }
    }

    /// Read a SID register (addr 0x00–0x1F).
    ///
    /// Most registers are write-only (return 0). Only $1B (OSC3) and
    /// $1C (ENV3) return meaningful data. $19/$1A (paddles) return 0.
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr & 0x1F {
            // OSC3: top 8 bits of voice 3 waveform output
            0x1B => {
                let ring_src_msb = self.voices[1].msb();
                let wav = self.voices[2].waveform_output(ring_src_msb);
                (wav >> 4) as u8
            }
            // ENV3: voice 3 envelope level
            0x1C => self.envelopes[2].level,
            // All other registers are write-only or paddle ($19/$1A)
            _ => 0,
        }
    }

    /// Write a SID register (addr 0x00–0x1F).
    pub fn write(&mut self, addr: u8, value: u8) {
        let reg = addr & 0x1F;
        match reg {
            // Voice 1 (0x00–0x06)
            0x00 => self.voices[0].frequency = (self.voices[0].frequency & 0xFF00) | u16::from(value),
            0x01 => self.voices[0].frequency = (self.voices[0].frequency & 0x00FF) | (u16::from(value) << 8),
            0x02 => self.voices[0].pulse_width = (self.voices[0].pulse_width & 0x0F00) | u16::from(value),
            0x03 => self.voices[0].pulse_width = (self.voices[0].pulse_width & 0x00FF) | ((u16::from(value) & 0x0F) << 8),
            0x04 => self.voices[0].control = value,
            0x05 => {
                self.envelopes[0].attack = (value >> 4) & 0x0F;
                self.envelopes[0].decay = value & 0x0F;
            }
            0x06 => {
                self.envelopes[0].sustain = (value >> 4) & 0x0F;
                self.envelopes[0].release = value & 0x0F;
            }

            // Voice 2 (0x07–0x0D)
            0x07 => self.voices[1].frequency = (self.voices[1].frequency & 0xFF00) | u16::from(value),
            0x08 => self.voices[1].frequency = (self.voices[1].frequency & 0x00FF) | (u16::from(value) << 8),
            0x09 => self.voices[1].pulse_width = (self.voices[1].pulse_width & 0x0F00) | u16::from(value),
            0x0A => self.voices[1].pulse_width = (self.voices[1].pulse_width & 0x00FF) | ((u16::from(value) & 0x0F) << 8),
            0x0B => self.voices[1].control = value,
            0x0C => {
                self.envelopes[1].attack = (value >> 4) & 0x0F;
                self.envelopes[1].decay = value & 0x0F;
            }
            0x0D => {
                self.envelopes[1].sustain = (value >> 4) & 0x0F;
                self.envelopes[1].release = value & 0x0F;
            }

            // Voice 3 (0x0E–0x14)
            0x0E => self.voices[2].frequency = (self.voices[2].frequency & 0xFF00) | u16::from(value),
            0x0F => self.voices[2].frequency = (self.voices[2].frequency & 0x00FF) | (u16::from(value) << 8),
            0x10 => self.voices[2].pulse_width = (self.voices[2].pulse_width & 0x0F00) | u16::from(value),
            0x11 => self.voices[2].pulse_width = (self.voices[2].pulse_width & 0x00FF) | ((u16::from(value) & 0x0F) << 8),
            0x12 => self.voices[2].control = value,
            0x13 => {
                self.envelopes[2].attack = (value >> 4) & 0x0F;
                self.envelopes[2].decay = value & 0x0F;
            }
            0x14 => {
                self.envelopes[2].sustain = (value >> 4) & 0x0F;
                self.envelopes[2].release = value & 0x0F;
            }

            // Filter cutoff lo (bits 0–2 only)
            0x15 => {
                self.filter.cutoff = (self.filter.cutoff & 0x7F8) | u16::from(value & 0x07);
            }
            // Filter cutoff hi (bits 3–10)
            0x16 => {
                self.filter.cutoff = (self.filter.cutoff & 0x007) | (u16::from(value) << 3);
            }
            // Filter routing ($D417): resonance (hi nibble), routing (lo nibble)
            0x17 => {
                self.filter.resonance = (value >> 4) & 0x0F;
                self.filter.routing = value & 0x07;
                self.filter.ext_in = value & 0x08 != 0;
            }
            // Volume + filter mode ($D418)
            0x18 => {
                self.volume = value & 0x0F;
                self.filter.mode = value & 0x70;
                self.voice3_off = value & 0x80 != 0;
            }

            // $19–$1C: read-only registers, writes are ignored
            _ => {}
        }
    }

    /// Tick the SID one CPU cycle.
    ///
    /// Clocks all three oscillators, applies sync and ring modulation,
    /// clocks all three envelopes, mixes through the filter, applies
    /// master volume, and accumulates for downsampling.
    pub fn tick(&mut self) {
        // 1. Capture previous MSB states for sync detection
        let prev_msb = [
            self.voices[0].msb(),
            self.voices[1].msb(),
            self.voices[2].msb(),
        ];

        // 2. Clock all accumulators
        for voice in &mut self.voices {
            voice.clock_accumulator();
        }

        // 3. Clock noise LFSRs
        for voice in &mut self.voices {
            voice.clock_noise();
        }

        // 4. Apply hard sync (voice 0→1, 1→2, 2→0)
        // Sync source for voice N is voice (N-1+3)%3
        if self.voices[0].control & 0x02 != 0 {
            self.voices[0].apply_sync(prev_msb[2], self.voices[2].msb());
        }
        if self.voices[1].control & 0x02 != 0 {
            self.voices[1].apply_sync(prev_msb[0], self.voices[0].msb());
        }
        if self.voices[2].control & 0x02 != 0 {
            self.voices[2].apply_sync(prev_msb[1], self.voices[1].msb());
        }

        // 5. Clock envelopes
        for i in 0..3 {
            let gate = self.voices[i].control & 0x01 != 0;
            self.envelopes[i].clock(gate);
        }

        // 6. Compute waveform outputs with ring modulation
        // Ring mod source: voice 2→0, voice 0→1, voice 1→2
        let ring_mod_msb = [
            self.voices[2].msb(),
            self.voices[0].msb(),
            self.voices[1].msb(),
        ];

        let mut filtered_sum: f32 = 0.0;
        let mut direct_sum: f32 = 0.0;

        for (i, (voice, (env, &ring_msb))) in self
            .voices
            .iter()
            .zip(self.envelopes.iter().zip(ring_mod_msb.iter()))
            .enumerate()
        {
            let waveform = voice.waveform_output(ring_msb);
            let envelope = env.level;

            // Centre the 12-bit waveform around 0 (-2048..+2047), scale by envelope
            let centred = f32::from(waveform.cast_signed() - 2048);
            let amplitude = centred * f32::from(envelope) / 255.0;

            // Voice 3 mute: exclude from audio mix but keep running
            if i == 2 && self.voice3_off {
                continue;
            }

            // Route through filter or direct
            if self.filter.voice_routed(i) {
                filtered_sum += amplitude;
            } else {
                direct_sum += amplitude;
            }
        }

        // 7. Process filter
        let filter_output = self.filter.clock(filtered_sum);

        // 8. Mix and apply master volume
        let mixed = (filter_output + direct_sum) * f32::from(self.volume) / 15.0;

        // Normalise to -1.0..1.0 range (3 voices × 2048 max amplitude = 6144)
        let normalised = mixed / 6144.0;

        // 9. Downsample: accumulate and emit when threshold reached
        self.accumulator += normalised;
        self.sample_count += 1;

        if self.sample_count as f32 >= self.ticks_per_sample {
            let avg = self.accumulator / self.sample_count as f32;
            self.buffer.push(avg);
            self.accumulator = 0.0;
            self.sample_count = 0;
        }
    }

    /// Take the audio output buffer (drains it).
    pub fn take_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    /// Number of samples in the output buffer.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_when_no_voices_active() {
        let mut sid = Sid6581::new(985_248, 48_000);
        // Run for one frame (~19,656 ticks)
        for _ in 0..19_656 {
            sid.tick();
        }
        let buf = sid.take_buffer();
        assert!(!buf.is_empty(), "Should produce samples even when silent");
        // All samples should be 0 (or very close)
        for &s in &buf {
            assert!(
                s.abs() < 1e-6,
                "Expected silence, got sample value {s}"
            );
        }
    }

    #[test]
    fn sawtooth_produces_periodic_waveform() {
        let mut sid = Sid6581::new(985_248, 48_000);

        // Voice 1: sawtooth, ~440 Hz
        // Frequency = 440 * 16777216 / 985248 ≈ 7479
        let freq: u16 = 7479;
        sid.write(0x00, (freq & 0xFF) as u8);
        sid.write(0x01, (freq >> 8) as u8);
        // Control: sawtooth waveform (bit 5) + gate on (bit 0)
        sid.write(0x04, 0x21);
        // ADSR: instant attack (0), max sustain (F), no decay/release
        sid.write(0x05, 0x00); // AD
        sid.write(0x06, 0xF0); // SR
        // Master volume = 15
        sid.write(0x18, 0x0F);

        // Run for ~2 frames
        for _ in 0..40_000 {
            sid.tick();
        }

        let buf = sid.take_buffer();
        assert!(buf.len() > 100, "Expected many samples, got {}", buf.len());

        // Should have both positive and negative samples (centred waveform)
        let has_positive = buf.iter().any(|&s| s > 0.01);
        let has_negative = buf.iter().any(|&s| s < -0.01);
        assert!(has_positive, "Expected positive samples in sawtooth");
        assert!(has_negative, "Expected negative samples in sawtooth");
    }

    #[test]
    fn adsr_attack_reaches_max() {
        let mut sid = Sid6581::new(985_248, 48_000);

        // Voice 1: fastest attack (0), max sustain
        sid.write(0x05, 0x00); // AD: attack=0, decay=0
        sid.write(0x06, 0xF0); // SR: sustain=F, release=0

        // Gate on
        sid.write(0x04, 0x01);

        // Run enough ticks for attack to complete (fastest attack = ~2ms = ~2000 ticks)
        for _ in 0..3000 {
            sid.tick();
        }

        assert_eq!(
            sid.envelopes[0].level, 0xFF,
            "Envelope should reach 0xFF after attack"
        );
        assert_eq!(
            sid.envelopes[0].phase,
            Phase::Sustain,
            "Should be in sustain phase"
        );
    }

    #[test]
    fn adsr_release_decays_to_zero() {
        let mut sid = Sid6581::new(985_248, 48_000);

        // Fastest ADSR all around
        sid.write(0x05, 0x00); // AD: attack=0, decay=0
        sid.write(0x06, 0xF0); // SR: sustain=F, release=0

        // Gate on, let attack+decay settle
        sid.write(0x04, 0x01);
        for _ in 0..3000 {
            sid.tick();
        }
        assert_eq!(sid.envelopes[0].level, 0xFF);

        // Gate off → release
        sid.write(0x04, 0x00);
        for _ in 0..50_000 {
            sid.tick();
        }

        assert_eq!(
            sid.envelopes[0].level, 0,
            "Envelope should decay to 0 after release"
        );
    }

    #[test]
    fn osc3_read_returns_nonzero() {
        let mut sid = Sid6581::new(985_248, 48_000);

        // Voice 3: sawtooth, high frequency
        sid.write(0x0E, 0xFF);
        sid.write(0x0F, 0xFF);
        sid.write(0x12, 0x20); // Sawtooth, no gate needed for OSC3 read

        // Run a few ticks to advance accumulator
        for _ in 0..100 {
            sid.tick();
        }

        let osc3 = sid.read(0x1B);
        // With max frequency and 100 ticks, accumulator should have advanced
        // significantly from 0
        assert!(osc3 > 0, "OSC3 should return non-zero with running oscillator");
    }

    #[test]
    fn env3_read_tracks_envelope() {
        let mut sid = Sid6581::new(985_248, 48_000);

        // Voice 3: fastest attack, max sustain
        sid.write(0x13, 0x00); // AD
        sid.write(0x14, 0xF0); // SR
        sid.write(0x12, 0x01); // Gate on

        // Run to complete attack
        for _ in 0..3000 {
            sid.tick();
        }

        let env3 = sid.read(0x1C);
        assert_eq!(env3, 0xFF, "ENV3 should read 0xFF at full envelope");
    }

    #[test]
    fn filter_attenuates_routed_voice() {
        // Measure output with and without filter routing at a low cutoff.
        // Use a high-frequency signal (5 kHz) so the LP filter clearly
        // attenuates it at minimum cutoff.
        let run_with_filter = |filtered: bool| -> f32 {
            let mut sid = Sid6581::new(985_248, 48_000);

            // Voice 1: sawtooth ~5 kHz (well above the LP cutoff)
            // freq = 5000 * 16777216 / 985248 ≈ 85,143 — clamp to max u16
            let freq: u16 = 65_535; // Maximum frequency
            sid.write(0x00, (freq & 0xFF) as u8);
            sid.write(0x01, (freq >> 8) as u8);
            sid.write(0x04, 0x21); // Sawtooth + gate
            sid.write(0x05, 0x00);
            sid.write(0x06, 0xF0);

            if filtered {
                // Minimum cutoff, LP mode, route voice 1 through filter
                sid.write(0x15, 0x00);
                sid.write(0x16, 0x00); // Minimum cutoff
                sid.write(0x17, 0x01); // Route voice 1, resonance 0
                sid.write(0x18, 0x1F); // LP mode + vol 15
            } else {
                sid.write(0x18, 0x0F); // No filter, vol 15
            }

            for _ in 0..60_000 {
                sid.tick();
            }

            let buf = sid.take_buffer();
            // RMS amplitude (skip first 200 samples for filter settling)
            let settled = &buf[200.min(buf.len())..];
            let sum_sq: f32 = settled.iter().map(|s| s * s).sum();
            (sum_sq / settled.len() as f32).sqrt()
        };

        let direct_rms = run_with_filter(false);
        let filtered_rms = run_with_filter(true);

        assert!(
            filtered_rms < direct_rms * 0.8,
            "Filtered RMS ({filtered_rms}) should be notably less than direct ({direct_rms})"
        );
    }

    #[test]
    fn take_buffer_drains() {
        let mut sid = Sid6581::new(985_248, 48_000);
        for _ in 0..1000 {
            sid.tick();
        }
        let buf = sid.take_buffer();
        assert!(!buf.is_empty());
        assert_eq!(sid.buffer_len(), 0, "Buffer should be empty after take");
    }
}
