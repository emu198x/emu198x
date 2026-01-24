//! SID (Sound Interface Device) emulation.
//!
//! The SID (6581/8580) is the C64's sound chip with:
//! - 3 independent voices
//! - 4 waveforms per voice: triangle, sawtooth, pulse, noise
//! - ADSR envelope per voice
//! - Programmable filter (low-pass, band-pass, high-pass)
//! - Ring modulation and oscillator sync

/// SID clock frequency (PAL)
const SID_CLOCK: u32 = 985248;

/// SID chip state.
pub struct Sid {
    voices: [Voice; 3],
    /// Filter cutoff frequency (11-bit)
    filter_cutoff: u16,
    /// Filter resonance (4-bit)
    filter_resonance: u8,
    /// Filter routing (which voices go through filter)
    filter_routing: u8,
    /// Filter mode (LP/BP/HP) and voice 3 disable
    filter_mode: u8,
    /// Master volume (4-bit)
    volume: u8,
    /// Cycle accumulator for sample generation
    cycle_accumulator: u32,
    /// Filter state variables for state-variable filter
    filter_lp: f32,
    filter_bp: f32,
    filter_hp: f32,
}

/// Single SID voice.
struct Voice {
    /// Frequency (16-bit, determines pitch)
    frequency: u16,
    /// Pulse width (12-bit, for pulse waveform)
    pulse_width: u16,
    /// Control register (waveform, gate, sync, ring mod)
    control: u8,
    /// Attack/Decay rates
    attack_decay: u8,
    /// Sustain level / Release rate
    sustain_release: u8,
    /// Phase accumulator (24-bit)
    phase: u32,
    /// Previous phase (for edge detection)
    prev_phase: u32,
    /// Envelope generator
    envelope: Envelope,
    /// LFSR for noise generation (23-bit)
    lfsr: u32,
    /// Previous MSB of oscillator (for sync)
    prev_msb: bool,
}

/// ADSR envelope generator.
struct Envelope {
    /// Current envelope level (0-255)
    level: u8,
    /// Current state
    state: EnvelopeState,
    /// Rate counter (15-bit)
    rate_counter: u16,
    /// Exponential counter
    exp_counter: u8,
    /// Gate state from previous tick
    prev_gate: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum EnvelopeState {
    Attack,
    DecaySustain,
    Release,
}

// Attack rate table: cycles per envelope step
// These are the actual SID values
const ATTACK_RATES: [u16; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11720, 19532, 31251,
];

// Decay/Release rate table: cycles per envelope step
const DECAY_RATES: [u16; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11720, 19532, 31251,
];

// Exponential counter period lookup based on envelope level
fn exp_counter_period(level: u8) -> u8 {
    match level {
        0..=5 => 1,
        6..=13 => 2,
        14..=25 => 4,
        26..=54 => 8,
        55..=93 => 16,
        _ => 30,
    }
}

impl Voice {
    fn new() -> Self {
        Self {
            frequency: 0,
            pulse_width: 0,
            control: 0,
            attack_decay: 0,
            sustain_release: 0,
            phase: 0,
            prev_phase: 0,
            envelope: Envelope::new(),
            lfsr: 0x7FFFFF, // Initial LFSR state
            prev_msb: false,
        }
    }

    /// Clock the oscillator for one cycle.
    fn clock(&mut self) {
        self.prev_phase = self.phase;

        // Advance 24-bit phase accumulator
        self.phase = (self.phase + self.frequency as u32) & 0xFFFFFF;

        // Clock LFSR when bit 19 transitions from 0 to 1
        let msb = (self.phase & 0x080000) != 0;
        let prev_msb = (self.prev_phase & 0x080000) != 0;

        if msb && !prev_msb {
            // Clock the 23-bit LFSR (Fibonacci, taps at bits 17 and 22)
            let bit = ((self.lfsr >> 22) ^ (self.lfsr >> 17)) & 1;
            self.lfsr = ((self.lfsr << 1) | bit) & 0x7FFFFF;
        }

        // Track MSB for sync
        self.prev_msb = msb;
    }

    /// Get the 12-bit oscillator output (before waveform selection).
    fn osc_output(&self) -> u16 {
        // Upper 12 bits of the 24-bit phase accumulator
        ((self.phase >> 12) & 0xFFF) as u16
    }

    /// Get current waveform output (12-bit unsigned, 0-4095).
    fn waveform_output(&self) -> u16 {
        let waveform = self.control >> 4;

        if waveform == 0 {
            return 0;
        }

        let osc = self.osc_output();
        let mut output: u16 = 0xFFF;

        // Triangle (bit 0)
        if waveform & 0x1 != 0 {
            let tri = if osc & 0x800 != 0 { osc ^ 0xFFF } else { osc };
            // Triangle outputs bits 11 down to 0, shift to get 12-bit value
            let tri_out = (tri << 1) & 0xFFF;
            output &= tri_out;
        }

        // Sawtooth (bit 1)
        if waveform & 0x2 != 0 {
            output &= osc;
        }

        // Pulse (bit 2)
        if waveform & 0x4 != 0 {
            let pulse = if osc >= self.pulse_width {
                0xFFF
            } else {
                0x000
            };
            output &= pulse;
        }

        // Noise (bit 3)
        if waveform & 0x8 != 0 {
            // Noise uses specific bits from LFSR
            let noise = (((self.lfsr >> 22) & 1) << 11)
                | (((self.lfsr >> 20) & 1) << 10)
                | (((self.lfsr >> 16) & 1) << 9)
                | (((self.lfsr >> 13) & 1) << 8)
                | (((self.lfsr >> 11) & 1) << 7)
                | (((self.lfsr >> 7) & 1) << 6)
                | (((self.lfsr >> 4) & 1) << 5)
                | (((self.lfsr >> 2) & 1) << 4);
            output &= noise as u16;
        }

        output
    }

    /// Get the final voice output (waveform * envelope).
    fn output(&self) -> i32 {
        let wave = self.waveform_output() as i32;
        let env = self.envelope.level as i32;
        // Output is 20-bit: 12-bit waveform * 8-bit envelope
        (wave * env) >> 4
    }

    /// Clock the envelope for one cycle.
    fn clock_envelope(&mut self) {
        self.envelope
            .clock(self.control, self.attack_decay, self.sustain_release);
    }
}

impl Envelope {
    fn new() -> Self {
        Self {
            level: 0,
            state: EnvelopeState::Release,
            rate_counter: 0,
            exp_counter: 0,
            prev_gate: false,
        }
    }

    fn clock(&mut self, control: u8, attack_decay: u8, sustain_release: u8) {
        let gate = control & 0x01 != 0;

        // Gate edge detection
        if gate && !self.prev_gate {
            // Gate on: start attack
            self.state = EnvelopeState::Attack;
            self.rate_counter = 0;
        } else if !gate && self.prev_gate {
            // Gate off: start release
            self.state = EnvelopeState::Release;
        }
        self.prev_gate = gate;

        // Get rate based on state
        let rate = match self.state {
            EnvelopeState::Attack => ATTACK_RATES[(attack_decay >> 4) as usize],
            EnvelopeState::DecaySustain => DECAY_RATES[(attack_decay & 0x0F) as usize],
            EnvelopeState::Release => DECAY_RATES[(sustain_release & 0x0F) as usize],
        };

        // Increment rate counter
        self.rate_counter = self.rate_counter.wrapping_add(1);

        if self.rate_counter >= rate {
            self.rate_counter = 0;

            match self.state {
                EnvelopeState::Attack => {
                    // Linear attack
                    if self.level < 255 {
                        self.level = self.level.wrapping_add(1);
                        if self.level == 255 {
                            self.state = EnvelopeState::DecaySustain;
                        }
                    }
                }
                EnvelopeState::DecaySustain => {
                    let sustain = (sustain_release >> 4) | ((sustain_release >> 4) << 4);
                    if self.level > sustain {
                        // Exponential decay
                        self.exp_counter = self.exp_counter.wrapping_add(1);
                        if self.exp_counter >= exp_counter_period(self.level) {
                            self.exp_counter = 0;
                            self.level = self.level.saturating_sub(1);
                        }
                    }
                }
                EnvelopeState::Release => {
                    if self.level > 0 {
                        // Exponential release
                        self.exp_counter = self.exp_counter.wrapping_add(1);
                        if self.exp_counter >= exp_counter_period(self.level) {
                            self.exp_counter = 0;
                            self.level = self.level.saturating_sub(1);
                        }
                    }
                }
            }
        }
    }
}

impl Sid {
    pub fn new() -> Self {
        Self {
            voices: [Voice::new(), Voice::new(), Voice::new()],
            filter_cutoff: 0,
            filter_resonance: 0,
            filter_routing: 0,
            filter_mode: 0,
            volume: 0,
            cycle_accumulator: 0,
            filter_lp: 0.0,
            filter_bp: 0.0,
            filter_hp: 0.0,
        }
    }

    /// Write to a SID register.
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr {
            // Voice 1
            0x00 => self.voices[0].frequency = (self.voices[0].frequency & 0xFF00) | value as u16,
            0x01 => {
                self.voices[0].frequency =
                    (self.voices[0].frequency & 0x00FF) | ((value as u16) << 8)
            }
            0x02 => {
                self.voices[0].pulse_width = (self.voices[0].pulse_width & 0x0F00) | value as u16
            }
            0x03 => {
                self.voices[0].pulse_width =
                    (self.voices[0].pulse_width & 0x00FF) | (((value & 0x0F) as u16) << 8)
            }
            0x04 => self.voices[0].control = value,
            0x05 => self.voices[0].attack_decay = value,
            0x06 => self.voices[0].sustain_release = value,

            // Voice 2
            0x07 => self.voices[1].frequency = (self.voices[1].frequency & 0xFF00) | value as u16,
            0x08 => {
                self.voices[1].frequency =
                    (self.voices[1].frequency & 0x00FF) | ((value as u16) << 8)
            }
            0x09 => {
                self.voices[1].pulse_width = (self.voices[1].pulse_width & 0x0F00) | value as u16
            }
            0x0A => {
                self.voices[1].pulse_width =
                    (self.voices[1].pulse_width & 0x00FF) | (((value & 0x0F) as u16) << 8)
            }
            0x0B => self.voices[1].control = value,
            0x0C => self.voices[1].attack_decay = value,
            0x0D => self.voices[1].sustain_release = value,

            // Voice 3
            0x0E => self.voices[2].frequency = (self.voices[2].frequency & 0xFF00) | value as u16,
            0x0F => {
                self.voices[2].frequency =
                    (self.voices[2].frequency & 0x00FF) | ((value as u16) << 8)
            }
            0x10 => {
                self.voices[2].pulse_width = (self.voices[2].pulse_width & 0x0F00) | value as u16
            }
            0x11 => {
                self.voices[2].pulse_width =
                    (self.voices[2].pulse_width & 0x00FF) | (((value & 0x0F) as u16) << 8)
            }
            0x12 => self.voices[2].control = value,
            0x13 => self.voices[2].attack_decay = value,
            0x14 => self.voices[2].sustain_release = value,

            // Filter
            0x15 => self.filter_cutoff = (self.filter_cutoff & 0x7F8) | (value & 0x07) as u16,
            0x16 => self.filter_cutoff = (self.filter_cutoff & 0x007) | ((value as u16) << 3),
            0x17 => {
                self.filter_resonance = value >> 4;
                self.filter_routing = value & 0x0F;
            }
            0x18 => {
                self.filter_mode = value >> 4;
                self.volume = value & 0x0F;
            }

            _ => {}
        }
    }

    /// Read from a SID register (most are write-only).
    pub fn read(&self, addr: u8) -> u8 {
        match addr {
            // Oscillator 3 output (for random numbers)
            0x1B => ((self.voices[2].phase >> 16) & 0xFF) as u8,
            // Envelope 3 output
            0x1C => self.voices[2].envelope.level,
            _ => 0,
        }
    }

    /// Advance the SID by the given number of CPU cycles.
    /// This is called during emulation to keep the SID in sync.
    pub fn tick(&mut self, cycles: u32) {
        self.cycle_accumulator += cycles;
    }

    /// Generate audio samples.
    /// This runs the SID for the appropriate number of cycles and generates samples.
    pub fn generate_samples(&mut self, buffer: &mut [f32], _cpu_clock: u32, sample_rate: u32) {
        // Calculate cycles per sample
        let cycles_per_sample = SID_CLOCK as f64 / sample_rate as f64;
        let mut cycle_fraction: f64 = 0.0;

        for sample in buffer.iter_mut() {
            // Determine how many cycles to run for this sample
            cycle_fraction += cycles_per_sample;
            let cycles_this_sample = cycle_fraction as u32;
            cycle_fraction -= cycles_this_sample as f64;

            // Run the SID for these cycles
            for _ in 0..cycles_this_sample {
                self.clock_once();
            }

            // Generate the output sample
            *sample = self.output_sample();
        }

        self.cycle_accumulator = 0;
    }

    /// Clock the SID for one cycle.
    fn clock_once(&mut self) {
        // Clock all oscillators
        for voice in &mut self.voices {
            voice.clock();
        }

        // Clock all envelopes
        for voice in &mut self.voices {
            voice.clock_envelope();
        }

        // Handle sync (voice N syncs from voice N-1, with wraparound)
        for i in 0..3 {
            let sync_source = (i + 2) % 3; // Voice 0 syncs from 2, 1 from 0, 2 from 1
            if self.voices[i].control & 0x02 != 0 {
                // Sync enabled - reset phase when sync source MSB transitions
                if self.voices[sync_source].prev_msb
                    && !((self.voices[sync_source].prev_phase & 0x800000) != 0)
                {
                    self.voices[i].phase = 0;
                }
            }
        }
    }

    /// Generate one output sample.
    fn output_sample(&mut self) -> f32 {
        let mut filtered: i32 = 0;
        let mut unfiltered: i32 = 0;
        let mix: i32;

        for (i, voice) in self.voices.iter().enumerate() {
            // Check if voice 3 is disabled for output
            if i == 2 && self.filter_mode & 0x80 != 0 {
                continue;
            }

            let output = voice.output();

            // Route to filter or direct output
            if self.filter_routing & (1 << i) != 0 {
                filtered += output;
            } else {
                unfiltered += output;
            }
        }

        // Apply filter if any filter mode is enabled
        if self.filter_mode & 0x70 != 0 && self.filter_routing != 0 {
            // Simple state-variable filter approximation
            // Cutoff frequency mapping (very approximate)
            let fc = (self.filter_cutoff as f32) / 2048.0;
            let cutoff = (fc * fc * 0.25).clamp(0.002, 0.99);

            // Resonance (Q factor)
            let resonance = 1.0 - (self.filter_resonance as f32 / 17.0);

            // State variable filter
            let input = filtered as f32 / 65536.0;
            self.filter_hp = input - self.filter_lp - resonance * self.filter_bp;
            self.filter_bp += cutoff * self.filter_hp;
            self.filter_lp += cutoff * self.filter_bp;

            // Select filter output based on mode
            let mut filter_out = 0.0;
            if self.filter_mode & 0x10 != 0 {
                filter_out += self.filter_lp; // Low-pass
            }
            if self.filter_mode & 0x20 != 0 {
                filter_out += self.filter_bp; // Band-pass
            }
            if self.filter_mode & 0x40 != 0 {
                filter_out += self.filter_hp; // High-pass
            }

            mix = (filter_out * 65536.0) as i32 + unfiltered;
        } else {
            mix = filtered + unfiltered;
        }

        // Apply master volume
        let output = (mix as f32 / 65536.0) * (self.volume as f32 / 15.0);

        // Clamp to valid range
        output.clamp(-1.0, 1.0)
    }

    /// Reset the SID.
    pub fn reset(&mut self) {
        self.voices = [Voice::new(), Voice::new(), Voice::new()];
        self.filter_cutoff = 0;
        self.filter_resonance = 0;
        self.filter_routing = 0;
        self.filter_mode = 0;
        self.volume = 0;
        self.cycle_accumulator = 0;
        self.filter_lp = 0.0;
        self.filter_bp = 0.0;
        self.filter_hp = 0.0;
    }
}

impl Default for Sid {
    fn default() -> Self {
        Self::new()
    }
}
