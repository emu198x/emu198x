//! TIA audio — two independent channels of the classic 2600 sound hardware.
//!
//! Ported from Stella's `AudioChannel`/`Audio` (the reference implementation of
//! Chris Brenner's gate-level analysis). Each channel is a pair of polynomial
//! counters: a 5-bit "noise" counter and a 4-bit "pulse" counter whose feedback
//! taps are selected by the 4-bit AUDC control, divided by the 5-bit AUDF
//! frequency, and scaled by the 4-bit AUDV volume.
//!
//! Clocking matches the hardware: the aggregate [`TiaAudio::tick`] runs once per
//! colour clock and samples both channels' volume every clock; the two phase
//! clocks fire at four fixed positions per scanline (9/81 → `phase0`,
//! 37/149 → `phase1`), so two output samples are produced per line.

/// One TIA audio channel (AUDC/AUDF/AUDV + the two polynomial counters).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct AudioChannel {
    audc: u8,
    audf: u8,
    audv: u8,
    clock_enable: bool,
    noise_feedback: bool,
    noise_counter_bit4: bool,
    pulse_counter_hold: bool,
    div_counter: u8,
    pulse_counter: u8,
    noise_counter: u8,
}

impl AudioChannel {
    pub(crate) fn set_audc(&mut self, value: u8) {
        self.audc = value & 0x0F;
    }

    pub(crate) fn set_audf(&mut self, value: u8) {
        self.audf = value & 0x1F;
    }

    pub(crate) fn set_audv(&mut self, value: u8) {
        self.audv = value & 0x0F;
    }

    /// The channel's instantaneous output volume: the pulse counter's low bit
    /// gates the 4-bit volume, so the result is `0` or `audv` (`0..=15`).
    pub(crate) fn actual_volume(&self) -> u8 {
        (self.pulse_counter & 0x01) * self.audv
    }

    /// First phase clock: recompute the hold and feedback for this step, then
    /// advance the frequency divider.
    pub(crate) fn phase0(&mut self) {
        if self.clock_enable {
            self.noise_counter_bit4 = self.noise_counter & 0x01 != 0;

            match self.audc & 0x03 {
                0x00 | 0x01 => self.pulse_counter_hold = false,
                0x02 => self.pulse_counter_hold = (self.noise_counter & 0x1E) != 0x02,
                _ => self.pulse_counter_hold = !self.noise_counter_bit4,
            }

            self.noise_feedback = match self.audc & 0x03 {
                0x00 => {
                    ((self.pulse_counter ^ self.noise_counter) & 0x01) != 0
                        || !(self.noise_counter != 0 || self.pulse_counter != 0x0A)
                        || (self.audc & 0x0C) == 0
                }
                _ => {
                    ((self.noise_counter & 0x04 != 0) ^ (self.noise_counter & 0x01 != 0))
                        || self.noise_counter == 0
                }
            };
        }

        self.clock_enable = self.div_counter == self.audf;

        if self.div_counter == self.audf || self.div_counter == 0x1F {
            self.div_counter = 0;
        } else {
            self.div_counter += 1;
        }
    }

    /// Second phase clock: clock the two polynomial counters from the feedback
    /// computed in [`phase0`](Self::phase0).
    pub(crate) fn phase1(&mut self) {
        if !self.clock_enable {
            return;
        }

        let pulse_feedback = match self.audc >> 2 {
            0x00 => {
                ((self.pulse_counter & 0x02 != 0) ^ (self.pulse_counter & 0x01 != 0))
                    && self.pulse_counter != 0x0A
                    && (self.audc & 0x03) != 0
            }
            0x01 => self.pulse_counter & 0x08 == 0,
            0x02 => !self.noise_counter_bit4,
            _ => !((self.pulse_counter & 0x02 != 0) || (self.pulse_counter & 0x0E == 0)),
        };

        self.noise_counter >>= 1;
        if self.noise_feedback {
            self.noise_counter |= 0x10;
        }

        if !self.pulse_counter_hold {
            self.pulse_counter = !(self.pulse_counter >> 1) & 0x07;
            if pulse_feedback {
                self.pulse_counter |= 0x08;
            }
        }
    }
}

/// Number of summed-volume entries (`0..=0x1e`) in the mixing table.
const MIX_ENTRIES: usize = 0x1F;

/// Both TIA audio channels plus the colour-clock phase scheduler and the
/// host-side mono sample buffer.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct TiaAudio {
    channel0: AudioChannel,
    channel1: AudioChannel,
    /// Colour-clock position within the scanline (`0..=227`).
    counter: u16,
    /// Accumulated channel volumes since the last sample, plus the count.
    sum0: u32,
    sum1: u32,
    sum_ct: u32,
    /// Non-linear mixing curve for the summed volume (`0..=0x1e` → `0.0..=1.0`).
    mixing_table: [f32; MIX_ENTRIES],
    /// Mono samples produced this run, drained by the runtime each frame.
    /// Host-side drain buffer, not machine state — skipped from save-states.
    #[serde(skip)]
    samples: Vec<f32>,
}

impl Default for TiaAudio {
    fn default() -> Self {
        // Stella's resistor-ladder mixing curve: louder steps compress toward
        // the rail. R_MAX = 30, R = 1, full scale 0x7fff, normalised to f32.
        let mut mixing_table = [0.0_f32; MIX_ENTRIES];
        let r_max = 30.0_f64;
        let v_max = 0x1E as f64;
        for (v, slot) in mixing_table.iter_mut().enumerate() {
            let v = v as f64;
            let entry = (0x7FFF as f64 * v / v_max * (r_max + v_max) / (r_max + v)).floor();
            *slot = (entry / 32768.0) as f32;
        }
        Self {
            channel0: AudioChannel::default(),
            channel1: AudioChannel::default(),
            counter: 0,
            sum0: 0,
            sum1: 0,
            sum_ct: 0,
            mixing_table,
            samples: Vec::new(),
        }
    }
}

impl TiaAudio {
    /// Write one of the six audio registers (`$15-$1A`).
    pub(crate) fn write(&mut self, addr: u8, value: u8) {
        match addr {
            0x15 => self.channel0.set_audc(value),
            0x16 => self.channel1.set_audc(value),
            0x17 => self.channel0.set_audf(value),
            0x18 => self.channel1.set_audf(value),
            0x19 => self.channel0.set_audv(value),
            0x1A => self.channel1.set_audv(value),
            _ => {}
        }
    }

    /// Advance one colour clock: sample both channels' volume every clock and
    /// fire the phase clocks at their four fixed scanline positions, emitting
    /// one averaged sample per `phase1`.
    pub(crate) fn tick(&mut self) {
        self.sum0 += u32::from(self.channel0.actual_volume());
        self.sum1 += u32::from(self.channel1.actual_volume());
        self.sum_ct += 1;

        match self.counter {
            9 | 81 => {
                self.channel0.phase0();
                self.channel1.phase0();
            }
            37 | 149 => {
                self.channel0.phase1();
                self.channel1.phase1();
                self.create_sample();
            }
            _ => {}
        }

        self.counter += 1;
        if self.counter == 228 {
            self.counter = 0;
        }
    }

    /// Average the accumulated volumes since the last sample, mix the two
    /// channels through the non-linear table, and append one mono sample.
    fn create_sample(&mut self) {
        if self.sum_ct == 0 {
            return;
        }
        let s0 = (self.sum0 / self.sum_ct) as usize;
        let s1 = (self.sum1 / self.sum_ct) as usize;
        self.sum0 = 0;
        self.sum1 = 0;
        self.sum_ct = 0;

        let sum = (s0 + s1).min(MIX_ENTRIES - 1);
        self.samples.push(self.mixing_table[sum]);
    }

    /// Drain the mono samples produced since the last call.
    pub(crate) fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the audio for `lines` scanlines (228 colour clocks each).
    fn run_lines(audio: &mut TiaAudio, lines: usize) {
        for _ in 0..(lines * 228) {
            audio.tick();
        }
    }

    #[test]
    fn silence_when_volume_is_zero() {
        let mut audio = TiaAudio::default();
        // AUDC tone, AUDF set, but volume 0 → no output energy.
        audio.write(0x15, 0x04);
        audio.write(0x17, 0x05);
        run_lines(&mut audio, 4);
        let samples = audio.take_samples();
        assert!(!samples.is_empty(), "samples are still produced");
        assert!(samples.iter().all(|&s| s == 0.0), "zero volume is silent");
    }

    #[test]
    fn pure_tone_oscillates_between_two_levels() {
        let mut audio = TiaAudio::default();
        audio.write(0x15, 0x04); // AUDC0 = pure tone (÷2 of the divided clock)
        audio.write(0x17, 0x03); // AUDF0 divider
        audio.write(0x19, 0x0F); // AUDV0 full volume
        run_lines(&mut audio, 40);
        let samples = audio.take_samples();

        let max = samples.iter().cloned().fold(0.0_f32, f32::max);
        let min = samples.iter().cloned().fold(1.0_f32, f32::min);
        assert!(max > 0.0, "a sounding tone reaches a non-zero level");
        assert!(
            min < max,
            "the tone oscillates rather than sitting at one level"
        );
    }

    #[test]
    fn two_samples_per_scanline() {
        let mut audio = TiaAudio::default();
        run_lines(&mut audio, 10);
        assert_eq!(
            audio.take_samples().len(),
            20,
            "phase1 fires twice per line"
        );
    }
}
