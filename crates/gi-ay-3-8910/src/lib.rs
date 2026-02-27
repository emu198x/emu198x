//! General Instrument AY-3-8910 Programmable Sound Generator emulator.
//!
//! Three square-wave tone generators, a shared noise generator, a shared
//! envelope generator, and a per-channel mixer. Output is downsampled to
//! the configured sample rate (typically 48 kHz).
//!
//! # Register map (16 registers, active 0–13)
//!
//! | Reg | Name      | Bits |
//! |-----|-----------|------|
//! | R0  | A fine    | 7-0  |
//! | R1  | A coarse  | 3-0  |
//! | R2  | B fine    | 7-0  |
//! | R3  | B coarse  | 3-0  |
//! | R4  | C fine    | 7-0  |
//! | R5  | C coarse  | 3-0  |
//! | R6  | Noise     | 4-0  |
//! | R7  | Mixer     | 7-0  |
//! | R8  | A volume  | 4-0  |
//! | R9  | B volume  | 4-0  |
//! | R10 | C volume  | 4-0  |
//! | R11 | Env fine  | 7-0  |
//! | R12 | Env coarse| 7-0  |
//! | R13 | Env shape | 3-0  |
//! | R14 | Port A    | 7-0  |
//! | R15 | Port B    | 7-0  |

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

/// Logarithmic volume table for the AY-3-8910 DAC.
/// 16 levels, normalised to 0.0–1.0.
const VOLUME_TABLE: [f32; 16] = [
    0.0000, 0.0137, 0.0205, 0.0291,
    0.0423, 0.0618, 0.0847, 0.1369,
    0.1691, 0.2647, 0.3527, 0.4499,
    0.5765, 0.7258, 0.8819, 1.0000,
];

/// A single tone generator (square wave with 12-bit period).
struct ToneGenerator {
    /// 12-bit period register (from R0/R1, R2/R3, or R4/R5).
    period: u16,
    /// Down-counter. Toggles output when it reaches 0.
    counter: u16,
    /// Current square wave output (true = high).
    output: bool,
}

impl ToneGenerator {
    fn new() -> Self {
        Self {
            period: 0,
            counter: 0,
            output: false,
        }
    }

    /// Clock one internal tick (called at `chip_clock` / 8).
    fn clock(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        }
        if self.counter == 0 {
            self.counter = self.period;
            self.output = !self.output;
        }
    }
}

/// 17-bit LFSR noise generator with 5-bit period.
struct NoiseGenerator {
    /// 5-bit period register (from R6).
    period: u8,
    /// Down-counter.
    counter: u8,
    /// 17-bit LFSR state.
    lfsr: u32,
    /// Current noise output.
    output: bool,
}

impl NoiseGenerator {
    fn new() -> Self {
        Self {
            period: 0,
            counter: 0,
            lfsr: 1, // Non-zero seed
            output: false,
        }
    }

    /// Clock one internal tick (called at `chip_clock` / 8).
    fn clock(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        }
        if self.counter == 0 {
            self.counter = self.period.max(1);
            // XOR bits 0 and 3, inverted, feed back to bit 16
            let feedback = ((self.lfsr ^ (self.lfsr >> 3)) & 1) ^ 1;
            self.lfsr = (self.lfsr >> 1) | (feedback << 16);
            self.output = self.lfsr & 1 != 0;
        }
    }
}

/// Shared envelope generator with 16-bit period and 16 shapes.
struct EnvelopeGenerator {
    /// 16-bit period (from R11/R12).
    period: u16,
    /// Down-counter.
    counter: u16,
    /// Current step within envelope cycle (0–15, then wraps or holds).
    step: u8,
    /// True if envelope is holding (not counting).
    holding: bool,
    /// Current direction (true = counting up / attack).
    attack: bool,
    /// Shape register value (R13 & 0x0F).
    shape: u8,
}

impl EnvelopeGenerator {
    fn new() -> Self {
        Self {
            period: 0,
            counter: 0,
            step: 0,
            holding: false,
            attack: false,
            shape: 0,
        }
    }

    /// Clock one internal tick (called at `chip_clock` / 16).
    fn clock(&mut self) {
        if self.holding {
            return;
        }

        if self.counter > 0 {
            self.counter -= 1;
        }
        if self.counter == 0 {
            self.counter = self.period.max(1);
            self.step_envelope();
        }
    }

    fn step_envelope(&mut self) {
        self.step += 1;
        if self.step < 16 {
            return;
        }

        // Cycle complete — decide what happens next
        let cont = self.shape & 0x08 != 0;
        let alt = self.shape & 0x02 != 0;
        let hold = self.shape & 0x01 != 0;

        if cont {
            if hold {
                self.holding = true;
                if alt {
                    self.attack = !self.attack;
                }
            } else if alt {
                self.attack = !self.attack;
                self.step = 0;
            } else {
                self.step = 0;
            }
        } else {
            // No continue: hold at 0
            self.holding = true;
            self.step = 0;
            self.attack = false;
        }
    }

    /// Reset the envelope (triggered by writing R13).
    fn reset(&mut self, shape: u8) {
        self.shape = shape & 0x0F;
        self.step = 0;
        self.counter = self.period.max(1);
        self.holding = false;
        self.attack = shape & 0x04 != 0;
    }

    /// Current 4-bit envelope output level (0–15).
    fn output(&self) -> u8 {
        let level = self.step.min(15);
        if self.attack { level } else { 15 - level }
    }
}

/// Stereo panning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StereoMode {
    /// Mono: all channels mixed equally to both outputs.
    Mono,
    /// ACB: A→left, C→right, B→centre. Used by Spectrum 128.
    Acb,
    /// ABC: A→left, B→centre, C→right. Used by some Amstrad CPC setups.
    Abc,
}

/// AY-3-8910 Programmable Sound Generator.
pub struct Ay3_8910 {
    /// Raw register file (16 bytes).
    regs: [u8; 16],
    /// Currently selected register index.
    selected_reg: u8,

    tone: [ToneGenerator; 3],
    noise: NoiseGenerator,
    envelope: EnvelopeGenerator,

    /// Internal clock divider counter.
    clock_counter: u32,

    // Downsampling state
    accumulator: (f32, f32),
    sample_count: u32,
    ticks_per_sample: f32,
    buffer: Vec<[f32; 2]>,

    /// Stereo panning mode.
    stereo_mode: StereoMode,
}

impl Ay3_8910 {
    /// Create a new AY-3-8910.
    ///
    /// `clock_freq` is the chip input clock in Hz (e.g. 1,773,400 for
    /// Spectrum 128). `sample_rate` is the audio output rate (typically
    /// 48,000).
    #[must_use]
    pub fn new(clock_freq: u32, sample_rate: u32) -> Self {
        Self {
            regs: [0; 16],
            selected_reg: 0,
            tone: [ToneGenerator::new(), ToneGenerator::new(), ToneGenerator::new()],
            noise: NoiseGenerator::new(),
            envelope: EnvelopeGenerator::new(),
            clock_counter: 0,
            accumulator: (0.0, 0.0),
            sample_count: 0,
            ticks_per_sample: clock_freq as f32 / sample_rate as f32,
            buffer: Vec::with_capacity(sample_rate as usize / 50 + 1),
            stereo_mode: StereoMode::Mono,
        }
    }

    /// Set the stereo panning mode.
    pub fn set_stereo(&mut self, mode: StereoMode) {
        self.stereo_mode = mode;
    }

    /// Select a register by index (0–15).
    pub fn select_register(&mut self, reg: u8) {
        self.selected_reg = reg & 0x0F;
    }

    /// Write a value to the currently selected register.
    pub fn write_data(&mut self, value: u8) {
        let reg = self.selected_reg as usize;
        self.regs[reg] = value;

        match reg {
            // Tone periods
            0 | 1 => {
                self.tone[0].period =
                    u16::from(self.regs[0]) | (u16::from(self.regs[1] & 0x0F) << 8);
            }
            2 | 3 => {
                self.tone[1].period =
                    u16::from(self.regs[2]) | (u16::from(self.regs[3] & 0x0F) << 8);
            }
            4 | 5 => {
                self.tone[2].period =
                    u16::from(self.regs[4]) | (u16::from(self.regs[5] & 0x0F) << 8);
            }
            // Noise period
            6 => {
                self.noise.period = value & 0x1F;
            }
            // Envelope period
            11 | 12 => {
                self.envelope.period =
                    u16::from(self.regs[11]) | (u16::from(self.regs[12]) << 8);
            }
            // Envelope shape — writing resets the envelope
            13 => {
                self.envelope.period =
                    u16::from(self.regs[11]) | (u16::from(self.regs[12]) << 8);
                self.envelope.reset(value);
            }
            _ => {}
        }
    }

    /// Read the currently selected register.
    #[must_use]
    pub fn read_data(&self) -> u8 {
        self.regs[self.selected_reg as usize]
    }

    /// Advance the chip by one input clock cycle.
    pub fn tick(&mut self) {
        self.clock_counter += 1;

        // Tone and noise generators clock at input / 8
        if self.clock_counter.is_multiple_of(8) {
            for tone in &mut self.tone {
                tone.clock();
            }
            self.noise.clock();
        }

        // Envelope clocks at input / 16
        if self.clock_counter.is_multiple_of(16) {
            self.envelope.clock();
        }

        // Generate sample
        let (left, right) = self.mix();
        self.accumulator.0 += left;
        self.accumulator.1 += right;
        self.sample_count += 1;

        if self.sample_count as f32 >= self.ticks_per_sample {
            let n = self.sample_count as f32;
            self.buffer
                .push([self.accumulator.0 / n, self.accumulator.1 / n]);
            self.accumulator = (0.0, 0.0);
            self.sample_count = 0;
        }
    }

    /// Mix all three channels through the mixer to produce a stereo sample pair.
    fn mix(&self) -> (f32, f32) {
        let mixer = self.regs[7];
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        // Per-channel panning weights: (left_weight, right_weight)
        // Panned channels get 1.0 on their side, centre channels get 0.5 each.
        let pan: [(f32, f32); 3] = match self.stereo_mode {
            StereoMode::Mono => [(0.5, 0.5), (0.5, 0.5), (0.5, 0.5)],
            StereoMode::Acb => [(1.0, 0.0), (0.5, 0.5), (0.0, 1.0)], // A=left, C=right, B=centre
            StereoMode::Abc => [(1.0, 0.0), (0.5, 0.5), (0.0, 1.0)], // A=left, B=centre, C=right
        };

        for ch in 0..3 {
            let tone_disabled = mixer & (1 << ch) != 0;
            let noise_disabled = mixer & (1 << (ch + 3)) != 0;

            let tone_out = self.tone[ch].output || tone_disabled;
            let noise_out = self.noise.output || noise_disabled;

            let vol_reg = self.regs[8 + ch];
            let level = if vol_reg & 0x10 != 0 {
                self.envelope.output()
            } else {
                vol_reg & 0x0F
            };
            let amplitude = VOLUME_TABLE[level as usize];

            let channel_sample = if tone_out && noise_out {
                amplitude
            } else {
                0.0
            };

            // Centre each channel around 0.
            let centred = channel_sample - amplitude * 0.5;
            left += centred * pan[ch].0;
            right += centred * pan[ch].1;
        }

        // Normalise. Mono: max excursion = 3 × 0.5 × 0.5 = 0.75.
        // Stereo (hard-panned): max excursion = 1 × 0.5 × 1.0 + 1 × 0.5 × 0.5 = 0.75.
        // Use 0.75 for consistent headroom across all modes.
        (left / 0.75, right / 0.75)
    }

    /// Take the audio output buffer (drains it). Each sample is `[left, right]`.
    pub fn take_buffer(&mut self) -> Vec<[f32; 2]> {
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

    /// Spectrum 128K AY clock: 1.7734 MHz
    const AY_CLOCK: u32 = 1_773_400;
    const SAMPLE_RATE: u32 = 48_000;

    /// Helper: extract the left channel from stereo samples.
    fn left(buf: &[[f32; 2]]) -> Vec<f32> {
        buf.iter().map(|s| s[0]).collect()
    }

    /// Helper: extract the right channel from stereo samples.
    fn right(buf: &[[f32; 2]]) -> Vec<f32> {
        buf.iter().map(|s| s[1]).collect()
    }

    #[test]
    fn silent_when_no_channels_enabled() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);
        for _ in 0..40_000 {
            ay.tick();
        }
        let buf = ay.take_buffer();
        assert!(!buf.is_empty(), "Should produce samples even when silent");
    }

    #[test]
    fn tone_a_produces_waveform() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);

        let period: u16 = 284;
        ay.select_register(0);
        ay.write_data((period & 0xFF) as u8);
        ay.select_register(1);
        ay.write_data(((period >> 8) & 0x0F) as u8);

        ay.select_register(7);
        ay.write_data(0b0011_1000);

        ay.select_register(8);
        ay.write_data(0x0F);

        for _ in 0..70_000 {
            ay.tick();
        }

        let buf = ay.take_buffer();
        assert!(buf.len() > 100, "Expected many samples");

        let l = left(&buf);
        let has_positive = l.iter().any(|&s| s > 0.05);
        let has_negative = l.iter().any(|&s| s < -0.05);
        assert!(has_positive, "Expected positive samples in tone");
        assert!(has_negative, "Expected negative samples in tone");
    }

    #[test]
    fn noise_produces_waveform() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);

        ay.select_register(6);
        ay.write_data(15);

        ay.select_register(7);
        ay.write_data(0b0011_0111);

        ay.select_register(8);
        ay.write_data(0x0F);

        for _ in 0..70_000 {
            ay.tick();
        }

        let buf = ay.take_buffer();
        let l = left(&buf);
        let has_positive = l.iter().any(|&s| s > 0.05);
        let has_negative = l.iter().any(|&s| s < -0.05);
        assert!(has_positive, "Expected positive samples in noise");
        assert!(has_negative, "Expected negative samples in noise");
    }

    #[test]
    fn envelope_sawtooth() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);

        ay.select_register(8);
        ay.write_data(0x10);

        ay.select_register(7);
        ay.write_data(0b0011_1000);

        ay.select_register(0);
        ay.write_data(1);

        ay.select_register(11);
        ay.write_data(100);
        ay.select_register(12);
        ay.write_data(0);
        ay.select_register(13);
        ay.write_data(8);

        for _ in 0..70_000 {
            ay.tick();
        }

        let buf = ay.take_buffer();
        let l = left(&buf);
        let max = l.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min = l.iter().cloned().fold(f32::INFINITY, f32::min);
        let range = max - min;
        assert!(
            range > 0.1,
            "Envelope should create dynamic range, got {range}"
        );
    }

    #[test]
    fn envelope_triangle() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);

        ay.select_register(8);
        ay.write_data(0x10);

        ay.select_register(7);
        ay.write_data(0b0011_1000);

        ay.select_register(0);
        ay.write_data((500u16 & 0xFF) as u8);
        ay.select_register(1);
        ay.write_data(((500u16 >> 8) & 0x0F) as u8);

        ay.select_register(11);
        ay.write_data((500u16 & 0xFF) as u8);
        ay.select_register(12);
        ay.write_data((500u16 >> 8) as u8);
        ay.select_register(13);
        ay.write_data(10);

        for _ in 0..70_000 {
            ay.tick();
        }

        let buf = ay.take_buffer();
        let l = left(&buf);
        let max = l.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min = l.iter().cloned().fold(f32::INFINITY, f32::min);
        let range = max - min;
        assert!(
            range > 0.1,
            "Triangle envelope should create dynamic range, got {range}"
        );
    }

    #[test]
    fn register_read_back() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);

        ay.select_register(0);
        ay.write_data(0xAB);
        assert_eq!(ay.read_data(), 0xAB);

        ay.select_register(7);
        ay.write_data(0x3F);
        assert_eq!(ay.read_data(), 0x3F);
    }

    #[test]
    fn mixer_disables_channels() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);

        ay.select_register(7);
        ay.write_data(0xFF);

        ay.select_register(8);
        ay.write_data(0x0F);
        ay.select_register(9);
        ay.write_data(0x0F);
        ay.select_register(10);
        ay.write_data(0x0F);

        ay.select_register(0);
        ay.write_data(100);

        for _ in 0..40_000 {
            ay.tick();
        }

        let buf = ay.take_buffer();
        assert!(!buf.is_empty());
    }

    #[test]
    fn take_buffer_drains() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);
        for _ in 0..1000 {
            ay.tick();
        }
        let buf = ay.take_buffer();
        assert!(!buf.is_empty());
        assert_eq!(ay.buffer_len(), 0, "Buffer should be empty after take");
    }

    #[test]
    fn stereo_acb_pans_a_left_c_right() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);
        ay.set_stereo(StereoMode::Acb);

        // Enable only tone A at full volume.
        ay.select_register(0);
        ay.write_data(100); // Period
        ay.select_register(7);
        ay.write_data(0b0011_1110); // Tone A on (bit 0=0), B+C off
        ay.select_register(8);
        ay.write_data(0x0F); // Vol A = 15

        for _ in 0..70_000 {
            ay.tick();
        }

        let buf = ay.take_buffer();
        let l = left(&buf);
        let r = right(&buf);

        // Channel A should be fully left — right channel should be near-silent.
        let l_energy: f32 = l.iter().map(|s| s * s).sum();
        let r_energy: f32 = r.iter().map(|s| s * s).sum();
        assert!(
            l_energy > 0.1,
            "Left channel should have energy (tone A), got {l_energy}"
        );
        assert!(
            r_energy < 0.001,
            "Right channel should be silent (tone A panned left), got {r_energy}"
        );
    }

    #[test]
    fn stereo_acb_pans_c_right() {
        let mut ay = Ay3_8910::new(AY_CLOCK, SAMPLE_RATE);
        ay.set_stereo(StereoMode::Acb);

        // Enable only tone C at full volume.
        ay.select_register(4);
        ay.write_data(100); // Period C
        ay.select_register(7);
        ay.write_data(0b0011_1011); // Tone C on (bit 2=0), A+B off
        ay.select_register(10);
        ay.write_data(0x0F); // Vol C = 15

        for _ in 0..70_000 {
            ay.tick();
        }

        let buf = ay.take_buffer();
        let l = left(&buf);
        let r = right(&buf);

        let l_energy: f32 = l.iter().map(|s| s * s).sum();
        let r_energy: f32 = r.iter().map(|s| s * s).sum();
        assert!(
            r_energy > 0.1,
            "Right channel should have energy (tone C), got {r_energy}"
        );
        assert!(
            l_energy < 0.001,
            "Left channel should be silent (tone C panned right), got {l_energy}"
        );
    }
}
