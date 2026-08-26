//! General Instrument AY-3-8910 Programmable Sound Generator.
//!
//! Source references:
//! - `knowledge/chips/gi-ay-3-8912.md`
//! - Adapted from `../Emu198x-Older/crates/gi-ay-3-8912/src/lib.rs`
//!
//! 3 square-wave tone channels, 1 noise generator, an envelope generator,
//! and **two** 8-bit bidirectional I/O ports (A = register 14, B =
//! register 15). The single-port AY-3-8912 (no port B) is a thin facade
//! over this core; see the `gi-ay-3-8912` crate.
//!
//! The I/O ports carry no sound; machines use them for keyboard scanning,
//! joysticks, printers, and bank/LED control. Port direction is set by
//! mixer register 7 (bit 6 = port A, bit 7 = port B: 1 = output). The
//! host feeds input-pin state via [`Ay3_8910::set_port_a_input_mask`] /
//! [`Ay3_8910::set_port_b_input`] and reads the chip's output drive via
//! [`Ay3_8910::port_a_output`] / [`Ay3_8910::port_b_output`].
//!
//! On the Spectrum 128K (which uses the 8912, port A only):
//! - AY clock = CPU clock / 2 = 1.7734 MHz
//! - Register select: OUT to port $FFFD
//! - Data write: OUT to port $BFFD
//! - Data read: IN from port $FFFD

pub mod watch;
pub use watch::{AyWriteRecord, AyWriteWatch, DEFAULT_AY_WATCH_CAP};

/// Logarithmic volume table for the AY-3-8910 (0 = silent, 15 = maximum),
/// the normalised general-AY DAC approximation from the primary reference
/// (`reference/by-topic/psg-ay-3-8910`). Indices 12-14 previously diverged
/// from that table (0.5704/0.6873/0.8482); reconciled to the reference. (#157)
static VOLUME: [f32; 16] = [
    0.0000, 0.0137, 0.0205, 0.0291, 0.0423, 0.0618, 0.0847, 0.1369, 0.1691, 0.2647, 0.3527, 0.4499,
    0.5765, 0.7258, 0.8819, 1.0000,
];

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Ay3_8910 {
    /// The 16 registers (directly readable/writable).
    regs: [u8; 16],
    /// Currently selected register (0-15).
    selected: u8,

    // Tone generators (3 channels)
    tone_counter: [u16; 3],
    tone_output: [bool; 3],

    // Noise generator
    noise_counter: u16,
    noise_output: bool,
    /// 17-bit LFSR, taps at bits 0 and 3.
    noise_lfsr: u32,
    /// `/2` prescale: the LFSR advances only every *other* noise-counter expiry,
    /// giving `f_noise = f_clock/(16·NP)` (see [`Ay3_8910::tick`]).
    noise_prescale: bool,

    // Envelope generator
    env_counter: u32,
    /// Current ramp direction: `true` = rising (attack), `false` = falling
    /// (decay). Seeded from R13 bit 2 on write and flipped on each wrap for the
    /// alternating (triangle) shapes.
    env_att: bool,
    env_holding: bool,
    /// Current envelope output level (0-15).
    env_level: u8,
    /// `/2` prescale: the envelope steps only every *other* env-counter expiry
    /// (the AY-3-8910's `m_step = 2`; see [`Ay3_8910::tick`]).
    env_prescale: bool,

    /// Internal clock prescaler (0-7). The AY divides its input clock
    /// by 8 before driving tone, noise, and envelope counters.
    prescaler: u8,

    // Audio output accumulation (Bresenham-style integer timing)
    /// Accumulated output level for the current audio sample.
    sample_accum: f32,
    /// Number of AY ticks accumulated in the current sample.
    sample_ticks: u32,
    /// Bresenham error accumulator for sample timing.
    sample_error: u32,
    /// AY clock rate (for Bresenham division).
    ay_clock_hz: u32,
    /// Audio sample rate (for Bresenham division).
    sample_rate: u32,
    /// Output sample buffer for the current frame.
    samples: Vec<f32>,
    /// Number of samples written this frame.
    samples_written: usize,

    /// Host-side wiring of AY I/O port A (register 14). The chip's
    /// physical pins read whatever the host motherboard ties them to
    /// when in input mode; in output mode the chip drives the pins,
    /// but pulls / open-drain tie-downs on the board AND with the
    /// driven value. On the Sinclair 128K family this mask is `0xBF`
    /// (bit 6 = serial CTS, always low). On a chip with no external
    /// wiring the mask is `0xFF` and `read_data` returns the stored
    /// register value unmodified — the default.
    #[serde(default = "default_port_a_input_mask")]
    port_a_input_mask: u8,

    /// Host-side state of I/O port B (register 15) input pins. When port
    /// B is in input mode the chip reads these pins, not the stored
    /// register; the host drives them (e.g. the Tatung Einstein hangs its
    /// keyboard column lines off port B). Defaults to `0xFF` (no wiring).
    #[serde(default = "default_port_b_input")]
    port_b_input: u8,
}

fn default_port_a_input_mask() -> u8 {
    0xFF
}

fn default_port_b_input() -> u8 {
    0xFF
}

impl Ay3_8910 {
    /// Create a new AY chip.
    ///
    /// - `ay_clock_hz`: AY clock frequency (e.g., 1_773_400 for 128K Spectrum)
    /// - `sample_rate`: audio output sample rate (e.g., 44100)
    /// - `samples_per_frame`: pre-allocated buffer size
    pub fn new(ay_clock_hz: u32, sample_rate: u32, samples_per_frame: usize) -> Self {
        Self {
            regs: [0; 16],
            selected: 0,
            tone_counter: [0; 3],
            tone_output: [false; 3],
            noise_counter: 0,
            noise_output: false,
            noise_lfsr: 1, // Must be non-zero
            noise_prescale: false,
            env_counter: 0,
            env_att: false,
            env_holding: false,
            env_level: 0,
            env_prescale: false,
            prescaler: 0,
            sample_accum: 0.0,
            sample_ticks: 0,
            sample_error: 0,
            ay_clock_hz,
            sample_rate,
            samples: vec![0.0; samples_per_frame],
            samples_written: 0,
            port_a_input_mask: default_port_a_input_mask(),
            port_b_input: default_port_b_input(),
        }
    }

    /// Configure the host-side wiring of AY I/O port A (register 14).
    /// Bits that are pulled low on the motherboard read back as 0 even
    /// when the chip drives them high. On the Sinclair 128K family
    /// this mask is `0xBF` (bit 6 = serial CTS, tied low). Defaults to
    /// `0xFF` (no external wiring); set it once at machine
    /// construction time.
    pub fn set_port_a_input_mask(&mut self, mask: u8) {
        self.port_a_input_mask = mask;
    }

    /// Drive the input-pin state of I/O port B (register 15). Used by the
    /// host when port B is wired to an external input — e.g. the Einstein
    /// keyboard columns. Read back through `read_data` when R15 is
    /// selected and port B is in input mode.
    pub fn set_port_b_input(&mut self, value: u8) {
        self.port_b_input = value;
    }

    /// The value the chip drives on I/O port A (register 14) — the stored
    /// register contents. Meaningful when port A is in output mode (mixer
    /// register 7 bit 6 = 1); machines that use port A as an output (the
    /// Einstein drives the keyboard row select here) read it through this.
    #[must_use]
    pub fn port_a_output(&self) -> u8 {
        self.regs[14]
    }

    /// The value the chip drives on I/O port B (register 15) — the stored
    /// register contents. Meaningful when port B is in output mode (mixer
    /// register 7 bit 7 = 1).
    #[must_use]
    pub fn port_b_output(&self) -> u8 {
        self.regs[15]
    }

    /// Select which register (0-15) subsequent reads/writes address.
    /// On the Spectrum: OUT to port $FFFD.
    pub fn select_register(&mut self, reg: u8) {
        self.selected = reg & 0x0F;
    }

    /// Write a value to the currently selected register.
    /// On the Spectrum: OUT to port $BFFD.
    pub fn write_data(&mut self, val: u8) {
        let reg = self.selected as usize;
        // Mask register values to their valid bit widths
        let masked = match reg {
            1 | 3 | 5 => val & 0x0F, // Coarse tone: 4 bits
            6 => val & 0x1F,         // Noise period: 5 bits
            7 => val,                // Mixer: all 8 bits
            8..=10 => val & 0x1F,    // Volume + envelope mode: 5 bits
            13 => {
                // Writing to envelope shape (re)starts the envelope at the start
                // of its first ramp: direction from the Attack bit, level at that
                // ramp's beginning (0 if rising, 15 if falling).
                self.env_att = val & 0x04 != 0;
                self.env_level = if self.env_att { 0 } else { 15 };
                self.env_counter = 0;
                self.env_holding = false;
                self.env_prescale = false;
                val & 0x0F
            }
            _ => val,
        };
        self.regs[reg] = masked;
    }

    /// Read the currently selected register's value.
    /// On the Spectrum: IN from port $FFFD.
    ///
    /// Registers 14 and 15 are I/O ports on the AY-3-8910 (R15 absent
    /// on the -8912). The chip reads the pin state, not the stored
    /// register. In input mode the pin is whatever the host wires
    /// drive to it (`port_a_input_mask` carries that). In output mode
    /// the chip drives the pin but board-side pull-downs AND with the
    /// driven value — same mask applies. This matches what FUSE does
    /// at `peripherals/sound/ay.c:ay_registerport_read`, and is the
    /// difference that lets late-Ocean 128K loaders (Rainbow Islands,
    /// Bubble Bobble, Out Run) detect the Sinclair 128K via reading
    /// back `0xBF` after writing `0xFF` to R14. R15 (port B) reads the
    /// host-driven [`Self::set_port_b_input`] state when in input mode.
    pub fn read_data(&self) -> u8 {
        let reg = self.selected as usize;
        match reg {
            14 => {
                if self.regs[7] & 0x40 != 0 {
                    self.regs[14] & self.port_a_input_mask
                } else {
                    self.port_a_input_mask
                }
            }
            15 => {
                if self.regs[7] & 0x80 != 0 {
                    self.regs[15]
                } else {
                    self.port_b_input
                }
            }
            _ => self.regs[reg],
        }
    }

    /// The currently selected register index (0-15).
    ///
    /// Used by the runtime query layer (`spectrum.ay.selected_register`)
    /// and by snapshot serialisers that need to capture the chip's
    /// register-pointer state alongside the file. Distinct from
    /// `read_data`, which returns the *value* of that register.
    #[must_use]
    pub fn selected_register(&self) -> u8 {
        self.selected
    }

    /// Borrow the full 16-register file in index order.
    ///
    /// Each entry is the post-mask value (e.g. coarse-tone registers
    /// have already been clipped to 4 bits). Used by the runtime
    /// query layer (`spectrum.ay.registers`) so debuggers and tools
    /// can inspect the chip without driving 16 separate
    /// `select` + `read_data` round-trips.
    #[must_use]
    pub fn registers(&self) -> &[u8; 16] {
        &self.regs
    }

    /// Advance one AY clock cycle. Call at ay_clock_hz rate.
    ///
    /// The AY divides its input clock by 8 internally. Tone, noise,
    /// and envelope counters only advance every 8th input clock.
    /// Audio output is sampled every tick for accurate downsampling.
    pub fn tick(&mut self) {
        // Prescaler: divide input clock by 8
        self.prescaler += 1;
        if self.prescaler >= 8 {
            self.prescaler = 0;

            // -- Tone generators --
            for ch in 0..3 {
                if self.tone_counter[ch] == 0 {
                    let period = self.tone_period(ch);
                    self.tone_counter[ch] = period.max(1);
                    self.tone_output[ch] = !self.tone_output[ch];
                }
                self.tone_counter[ch] -= 1;
            }

            // -- Noise generator --
            if self.noise_counter == 0 {
                let period = (self.regs[6] & 0x1F) as u16;
                self.noise_counter = period.max(1);
                // The LFSR advances only every *other* counter expiry — a /2
                // prescale giving f_noise = f_clock/(16·NP), per the GI datasheet
                // and MAME's `m_prescale_noise` (ay8910.cpp). Clocking on every
                // expiry (f_clock/(8·NP)) makes noise an octave too bright. (#153)
                self.noise_prescale = !self.noise_prescale;
                if self.noise_prescale {
                    // 17-bit LFSR: new bit = bit 0 XOR bit 3
                    let bit = (self.noise_lfsr ^ (self.noise_lfsr >> 3)) & 1;
                    self.noise_lfsr = (self.noise_lfsr >> 1) | (bit << 16);
                    self.noise_output = self.noise_lfsr & 1 != 0;
                }
            }
            self.noise_counter -= 1;

            // -- Envelope generator --
            if !self.env_holding {
                if self.env_counter == 0 {
                    let period = self.envelope_period();
                    self.env_counter = period.max(1);
                    // The envelope steps only every *other* counter expiry — the
                    // AY-3-8910's m_step = 2 (MAME ay8910.cpp), i.e. /256 per
                    // 16-step cycle = /16 per step. Stepping on every expiry plays
                    // every volume envelope an octave too fast. (#152)
                    self.env_prescale = !self.env_prescale;
                    if self.env_prescale {
                        self.advance_envelope();
                    }
                }
                self.env_counter -= 1;
            }
        }

        // -- Compute output (sampled at full AY clock rate for accurate downsampling) --
        let output = self.compute_output();

        // -- Bresenham-style audio downsampling --
        // Accumulate output for averaging over each audio sample period.
        self.sample_accum += output;
        self.sample_ticks += 1;

        // Emit a sample when the Bresenham accumulator overflows.
        // This evenly distributes sample_rate samples across ay_clock_hz ticks
        // with zero floating-point drift.
        self.sample_error += self.sample_rate;
        if self.sample_error >= self.ay_clock_hz {
            self.sample_error -= self.ay_clock_hz;
            if self.samples_written < self.samples.len() {
                self.samples[self.samples_written] = self.sample_accum / self.sample_ticks as f32;
                self.samples_written += 1;
            }
            self.sample_accum = 0.0;
            self.sample_ticks = 0;
        }
    }

    /// Finish the frame and write audio samples to the output buffer.
    /// Samples are in the range 0.0 to 1.0.
    pub fn end_frame(&mut self, out: &mut [f32]) {
        // Flush any remaining partial sample (carries the accumulator
        // state across frames — no discontinuity at boundaries).
        if self.sample_ticks > 0 {
            if self.samples_written < self.samples.len() {
                self.samples[self.samples_written] = self.sample_accum / self.sample_ticks as f32;
                self.samples_written += 1;
            }
            // Don't reset sample_accum/sample_ticks — the partial sample
            // continues into the next frame for seamless audio.
            self.sample_accum = 0.0;
            self.sample_ticks = 0;
        }

        let n = out.len().min(self.samples_written);
        out[..n].copy_from_slice(&self.samples[..n]);
        // Zero any remaining output slots
        for s in &mut out[n..] {
            *s = 0.0;
        }
        self.samples_written = 0;
    }

    /// Number of samples generated this frame so far.
    pub fn samples_per_frame(&self) -> usize {
        self.samples.len()
    }

    // -- Internal helpers --

    fn tone_period(&self, ch: usize) -> u16 {
        let fine = self.regs[ch * 2] as u16;
        let coarse = (self.regs[ch * 2 + 1] & 0x0F) as u16;
        (coarse << 8) | fine
    }

    fn envelope_period(&self) -> u32 {
        let fine = self.regs[11] as u32;
        let coarse = self.regs[12] as u32;
        (coarse << 8) | fine
    }

    /// Advance the envelope one step. Faithful to the reference shape table
    /// (`reference/by-topic/psg-ay-3-8910`) and XRoar's `ay891x.c`: the level
    /// ramps in the current direction (`env_att`); on reaching an endpoint the
    /// Continue/Hold/Alternate bits decide repeat, reverse (triangle), or hold.
    /// Only called while not holding (gated in [`Ay3_8910::tick`]).
    fn advance_envelope(&mut self) {
        let shape = self.regs[13] & 0x0F;
        let cont = shape & 0x08 != 0;
        let alternate = shape & 0x02 != 0;
        let hold = shape & 0x01 != 0;

        if self.env_att {
            if self.env_level >= 15 {
                self.env_ramp_complete(cont, alternate, hold);
            } else {
                self.env_level += 1;
            }
        } else if self.env_level == 0 {
            self.env_ramp_complete(cont, alternate, hold);
        } else {
            self.env_level -= 1;
        }
    }

    /// A ramp reached its endpoint. `Continue = 0` (shapes 0-7) always falls to
    /// silence and holds — the reference's "fall and hold at 0", whichever way
    /// the ramp ran. With `Continue = 1`: `Hold` ends the envelope at the
    /// endpoint, flipped by `Alternate` (so shapes 11/13 hold at max); otherwise
    /// `Alternate` reverses direction (triangle shapes 10/14) and neither bit
    /// repeats the same ramp (sawtooth shapes 8/12).
    fn env_ramp_complete(&mut self, cont: bool, alternate: bool, hold: bool) {
        if !cont {
            self.env_level = 0;
            self.env_holding = true;
            return;
        }
        if hold {
            let endpoint = if self.env_att { 15 } else { 0 };
            self.env_level = if alternate { 15 - endpoint } else { endpoint };
            self.env_holding = true;
            return;
        }
        if alternate {
            self.env_att = !self.env_att;
        } else {
            // Repeat the same ramp from its start.
            self.env_level = if self.env_att { 0 } else { 15 };
        }
    }

    fn compute_output(&self) -> f32 {
        let mixer = self.regs[7];
        let mut total = 0.0f32;

        for ch in 0..3 {
            let tone_enable = mixer & (1 << ch) == 0; // Active low
            let noise_enable = mixer & (8 << ch) == 0; // Active low

            let tone_out = !tone_enable || self.tone_output[ch];
            let noise_out = !noise_enable || self.noise_output;
            let channel_on = tone_out && noise_out;

            let vol_reg = self.regs[8 + ch];
            let level = if vol_reg & 0x10 != 0 {
                // Envelope mode
                self.env_level
            } else {
                vol_reg & 0x0F
            };

            let amplitude = if channel_on {
                VOLUME[level as usize]
            } else {
                0.0
            };

            total += amplitude;
        }

        // Normalize: max is 3.0 (3 channels at full volume)
        total / 3.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_read_write() {
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        ay.select_register(0);
        ay.write_data(0xAB);
        assert_eq!(ay.read_data(), 0xAB);

        // Coarse register is 4-bit
        ay.select_register(1);
        ay.write_data(0xFF);
        assert_eq!(ay.read_data(), 0x0F);
    }

    #[test]
    fn noise_period_masked() {
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        ay.select_register(6);
        ay.write_data(0xFF);
        assert_eq!(ay.read_data(), 0x1F);
    }

    #[test]
    fn silent_by_default() {
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        // Mixer defaults to 0: all tone and noise enabled (active low = enabled)
        // But volume defaults to 0, so output should be silent
        for _ in 0..1000 {
            ay.tick();
        }
        let mut out = vec![0.0f32; 882];
        ay.end_frame(&mut out);
        let max = out.iter().cloned().fold(0.0f32, f32::max);
        assert!(max < 0.01, "expected silence, got max={}", max);
    }

    #[test]
    fn tone_produces_output() {
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        // Channel A: period = 100, volume = 15
        ay.select_register(0);
        ay.write_data(100); // Fine tune
        ay.select_register(1);
        ay.write_data(0); // Coarse tune
        ay.select_register(7);
        ay.write_data(0x3E); // Enable tone A only (bit 0 = 0)
        ay.select_register(8);
        ay.write_data(15); // Volume A = max

        // Tick for a frame's worth of AY clocks (~35,000)
        for _ in 0..35_000 {
            ay.tick();
        }
        let mut out = vec![0.0f32; 882];
        ay.end_frame(&mut out);
        let max = out.iter().cloned().fold(0.0f32, f32::max);
        assert!(max > 0.1, "expected audible output, got max={}", max);
    }

    #[test]
    fn selected_register_and_registers_expose_full_state() {
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        // Default state: register 0 selected, all 16 zeroed.
        assert_eq!(ay.selected_register(), 0);
        assert_eq!(ay.registers(), &[0u8; 16]);

        // Write distinct values into a few registers and confirm the
        // borrow returns the masked values (coarse tone clipped to 4 bits).
        ay.select_register(0);
        ay.write_data(0xAB); // fine tone A — full 8 bits preserved
        ay.select_register(1);
        ay.write_data(0xFF); // coarse tone A — clipped to 0x0F
        ay.select_register(7);
        ay.write_data(0x3E); // mixer
        ay.select_register(13);
        ay.write_data(0x09); // envelope shape

        assert_eq!(ay.selected_register(), 13);
        let regs = ay.registers();
        assert_eq!(regs[0], 0xAB);
        assert_eq!(regs[1], 0x0F);
        assert_eq!(regs[7], 0x3E);
        assert_eq!(regs[13], 0x09);
        // Untouched registers remain zero.
        assert_eq!(regs[2], 0x00);
    }

    #[test]
    fn detection_pattern_works() {
        // Mimics what Signal Part 3 does: write to register, read back
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        ay.select_register(8); // Volume A register
        ay.write_data(0x08); // Write a value
        let val = ay.read_data();
        assert_eq!(
            val & 0x0F,
            0x08,
            "AY detection should read back the written value"
        );
    }

    #[test]
    fn port_a_input_mask_defaults_to_no_pull() {
        // Default mask is 0xFF — chips with no external wiring should
        // behave as before this configurability landed: read back the
        // stored value unchanged.
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        ay.select_register(7);
        ay.write_data(0xFF); // port A = output mode
        ay.select_register(14);
        ay.write_data(0x42);
        assert_eq!(ay.read_data(), 0x42);
    }

    #[test]
    fn sinclair_128k_port_a_pull_returns_bf_for_register_14() {
        // With the Sinclair 128K wiring (port A pull = 0xBF, CTS pin
        // tied low at bit 6), reading R14 must reflect the pull no
        // matter what was written. This is the difference late-Ocean
        // loaders use to detect "real 128K". Matches FUSE behaviour
        // at `peripherals/sound/ay.c:ay_registerport_read`.
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        ay.set_port_a_input_mask(0xBF);

        // Output mode (r7 bit 6 = 1): chip drives the pin, board pulls
        // AND with the driven value. Writing 0xFF reads back as 0xBF.
        ay.select_register(7);
        ay.write_data(0xFF);
        ay.select_register(14);
        ay.write_data(0xFF);
        assert_eq!(
            ay.read_data(),
            0xBF,
            "output-mode read should mask driven value with board pull"
        );

        // Input mode (r7 bit 6 = 0): pin reads the board pull directly,
        // independent of whatever the register held.
        ay.select_register(7);
        ay.write_data(0x3F); // bit 6 = 0
        ay.select_register(14);
        assert_eq!(
            ay.read_data(),
            0xBF,
            "input-mode read should return the board pull directly"
        );
    }

    /// Measure the steady-state input-clock interval between successive changes
    /// of `value(&ay)`, ignoring the first (startup) gap. `ticks` must be long
    /// enough to see several changes.
    fn steady_state_gap<T: PartialEq + Copy>(
        ay: &mut Ay3_8910,
        ticks: usize,
        mut value: impl FnMut(&Ay3_8910) -> T,
    ) -> usize {
        let mut last = value(ay);
        let mut gaps = Vec::new();
        let mut since = 0usize;
        for _ in 0..ticks {
            ay.tick();
            since += 1;
            let now = value(ay);
            if now != last {
                gaps.push(since);
                since = 0;
                last = now;
            }
        }
        assert!(gaps.len() >= 3, "expected several changes, got {gaps:?}");
        // Every steady-state gap must be identical.
        for &g in &gaps[2..] {
            assert_eq!(g, gaps[1], "non-uniform gaps: {gaps:?}");
        }
        gaps[1]
    }

    #[test]
    fn noise_lfsr_advances_every_16_np_ticks() {
        // #153: the noise LFSR advances at f_clock/(16·NP) — the /8 internal
        // prescaler times a /2 noise prescale. Clocking it every 8·NP made noise
        // an octave too bright. Confirm the rate scales as 16·NP, not 8·NP.
        for np in [1u8, 3, 5] {
            let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
            ay.select_register(6);
            ay.write_data(np);
            let gap = steady_state_gap(&mut ay, 64 * usize::from(np), |ay| ay.noise_lfsr);
            assert_eq!(
                gap,
                16 * usize::from(np),
                "noise should advance every 16·NP ticks (NP={np})"
            );
        }
    }

    #[test]
    fn envelope_steps_every_16_ep_ticks() {
        // #152: the envelope steps every 16·EP input clocks — the /8 prescaler
        // times the AY-3-8910's m_step=2 /2 prescale. Stepping every 8·EP played
        // every volume envelope an octave too fast.
        for ep in [1u8, 2, 4] {
            let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
            ay.select_register(11);
            ay.write_data(ep); // envelope period fine
            ay.select_register(12);
            ay.write_data(0); // envelope period coarse → EP = ep
            ay.select_register(13);
            ay.write_data(0x08); // continuous ramp-down: steps forever, Continue=1
            let gap = steady_state_gap(&mut ay, 64 * usize::from(ep), |ay| ay.env_level);
            assert_eq!(
                gap,
                16 * usize::from(ep),
                "envelope should step every 16·EP ticks (EP={ep})"
            );
        }
    }

    /// The dedup-consecutive sequence of envelope levels a shape produces, one
    /// entry per step (EP=1 so each step is a fixed interval), starting from the
    /// level set by the R13 write.
    fn envelope_level_sequence(shape: u8, ticks: usize) -> Vec<u8> {
        let mut ay = Ay3_8910::new(1_773_400, 44100, 882);
        ay.select_register(11);
        ay.write_data(1); // envelope period fine = 1
        ay.select_register(12);
        ay.write_data(0);
        ay.select_register(13);
        ay.write_data(shape);
        let mut seq = vec![ay.env_level];
        for _ in 0..ticks {
            ay.tick();
            if *seq.last().expect("the sequence is non-empty") != ay.env_level {
                seq.push(ay.env_level);
            }
        }
        seq
    }

    #[test]
    fn envelope_shape_10_makes_a_triangle() {
        // #154: \/\/ — decay to 0, then *ramp* back up (not jump), proving the
        // alternation survives. The old code reset the step each pass and decayed
        // forever. Distinguish the triangle from the sawtooth (shape 8) by the
        // step *after* hitting 0: a triangle rises to 1, a sawtooth jumps to 15.
        let seq = envelope_level_sequence(0x0A, 1000);
        assert_eq!(seq[0], 15, "shape 10 starts at max (decay first): {seq:?}");
        let zero = seq.iter().position(|&l| l == 0).expect("should reach 0");
        assert_eq!(
            seq[zero + 1],
            1,
            "shape 10 should ramp up, not jump: {seq:?}"
        );
        assert!(
            seq[zero..].contains(&15),
            "shape 10 should climb back to 15: {seq:?}"
        );
    }

    #[test]
    fn envelope_shape_14_makes_a_triangle() {
        // #154: /\/\ — attack to 15, then ramp back down.
        let seq = envelope_level_sequence(0x0E, 1000);
        assert_eq!(seq[0], 0, "shape 14 starts at 0 (attack first): {seq:?}");
        let top = seq.iter().position(|&l| l == 15).expect("should reach 15");
        assert_eq!(
            seq[top + 1],
            14,
            "shape 14 should ramp down, not jump: {seq:?}"
        );
        assert!(
            seq[top..].contains(&0),
            "shape 14 should fall back to 0: {seq:?}"
        );
    }

    #[test]
    fn envelope_shapes_11_and_13_hold_at_max() {
        // #155: \¯¯¯ and /¯¯¯ — one sweep, then hold at 15. The old inverted
        // closed form held them at 0 (silence).
        for shape in [0x0Bu8, 0x0D] {
            let seq = envelope_level_sequence(shape, 1200);
            assert_eq!(
                *seq.last().expect("the sequence is non-empty"),
                15,
                "shape {shape:#04x} should hold at max: {seq:?}"
            );
        }
    }

    #[test]
    fn envelope_shapes_9_and_15_hold_at_zero() {
        // The Continue+Hold counterparts that settle to silence.
        for shape in [0x09u8, 0x0F] {
            let seq = envelope_level_sequence(shape, 1200);
            assert_eq!(
                *seq.last().expect("the sequence is non-empty"),
                0,
                "shape {shape:#04x} should hold at 0: {seq:?}"
            );
        }
    }

    #[test]
    fn envelope_continue_zero_shapes_fall_and_hold_at_zero() {
        // Continue=0 (shapes 0-7): a single ramp either way, then silence — the
        // reference's "fall and hold at 0".
        for shape in 0u8..8 {
            let seq = envelope_level_sequence(shape, 1200);
            assert_eq!(
                *seq.last().expect("the sequence is non-empty"),
                0,
                "shape {shape:#04x} should hold at 0: {seq:?}"
            );
        }
    }

    #[test]
    fn envelope_shape_8_repeats_a_falling_sawtooth() {
        // \\\\ — decay 15→0, jump back to 15, repeat (the alternate-free repeat).
        let seq = envelope_level_sequence(0x08, 1000);
        let zero = seq.iter().position(|&l| l == 0).expect("should reach 0");
        assert_eq!(
            seq[zero + 1],
            15,
            "shape 8 should jump back to 15, not ramp: {seq:?}"
        );
    }
}
