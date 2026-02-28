//! NES APU (Audio Processing Unit).
//!
//! The APU lives on the 2A03 CPU die. It produces audio via two pulse
//! channels, one triangle channel, one noise channel, and a DMC (delta
//! modulation) channel. The DMC fetches 1-bit delta-encoded samples from
//! PRG memory via DMA, stealing CPU cycles one byte at a time.
//!
//! The APU is ticked once per CPU cycle (~1.789 MHz NTSC). Pulse and
//! noise timers decrement every other CPU cycle (APU cycle). The triangle
//! timer decrements every CPU cycle. The frame counter divides CPU cycles
//! into quarter-frame and half-frame events for envelope, length counter,
//! linear counter, and sweep updates.
//!
//! Output is mixed through a non-linear mixer (nesdev formula) and
//! downsampled to 48 kHz.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]

// ---------------------------------------------------------------------------
// Lookup tables
// ---------------------------------------------------------------------------

/// Length counter load values, indexed by the top 5 bits of the register write.
const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96,
    22, 192, 24, 72, 26, 16, 28, 32, 30,
];

/// Noise timer period lookup (NTSC).
const NOISE_PERIOD_TABLE_NTSC: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

/// Noise timer period lookup (PAL).
const NOISE_PERIOD_TABLE_PAL: [u16; 16] = [
    4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
];

/// DMC rate table (NTSC) — CPU cycles per sample bit output.
const DMC_RATE_TABLE_NTSC: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

/// DMC rate table (PAL) — CPU cycles per sample bit output.
const DMC_RATE_TABLE_PAL: [u16; 16] = [
    398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
];

/// Triangle waveform: 32-step sequence (0–15 up, 15–0 down).
const TRIANGLE_SEQUENCE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
    11, 12, 13, 14, 15,
];

/// Pulse duty cycle waveforms: 4 patterns × 8 steps.
/// 0 = 12.5%, 1 = 25%, 2 = 50%, 3 = 75% (negated 25%).
const PULSE_DUTY: [[bool; 8]; 4] = [
    [false, true, false, false, false, false, false, false],  // 12.5%
    [false, true, true, false, false, false, false, false],   // 25%
    [false, true, true, true, true, false, false, false],     // 50%
    [true, false, false, true, true, true, true, true],       // 75%
];

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// Decay envelope, clocked at quarter-frame rate.
///
/// When the loop flag is clear, the envelope counts down from 15 to 0 and
/// stays there. When loop is set, it wraps from 0 back to 15.
struct Envelope {
    start_flag: bool,
    divider: u8,
    decay_level: u8,
    /// Volume / divider period (from register bits 0–3).
    volume: u8,
    /// Constant volume flag (register bit 4).
    constant_volume: bool,
    /// Loop flag (register bit 5, shared with length counter halt).
    loop_flag: bool,
}

impl Envelope {
    fn new() -> Self {
        Self {
            start_flag: false,
            divider: 0,
            decay_level: 0,
            volume: 0,
            constant_volume: false,
            loop_flag: false,
        }
    }

    /// Clock the envelope (called at quarter-frame rate).
    fn clock(&mut self) {
        if self.start_flag {
            self.start_flag = false;
            self.decay_level = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay_level > 0 {
                self.decay_level -= 1;
            } else if self.loop_flag {
                self.decay_level = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    /// Current output level (0–15).
    fn output(&self) -> u8 {
        if self.constant_volume {
            self.volume
        } else {
            self.decay_level
        }
    }
}

// ---------------------------------------------------------------------------
// Length counter
// ---------------------------------------------------------------------------

/// Length counter — counts down at half-frame rate. When it reaches zero
/// the channel is silenced.
struct LengthCounter {
    counter: u8,
    halt: bool,
    enabled: bool,
}

impl LengthCounter {
    fn new() -> Self {
        Self {
            counter: 0,
            halt: false,
            enabled: false,
        }
    }

    /// Clock the length counter (called at half-frame rate).
    fn clock(&mut self) {
        if !self.halt && self.counter > 0 {
            self.counter -= 1;
        }
    }

    /// Whether the channel is active (length counter > 0).
    fn active(&self) -> bool {
        self.counter > 0
    }

    /// Load a new value from the length table.
    fn load(&mut self, index: u8) {
        if self.enabled {
            self.counter = LENGTH_TABLE[index as usize];
        }
    }

    /// Set enabled state. Disabling clears the counter.
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.counter = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Sweep unit
// ---------------------------------------------------------------------------

/// Sweep unit for pulse channels. Adjusts the pulse timer period up or
/// down over time. Pulse 1 uses one's-complement negation (period =
/// period - (period >> shift) - 1). Pulse 2 uses two's-complement
/// (period = period - (period >> shift)).
struct Sweep {
    enabled: bool,
    negate: bool,
    shift: u8,
    period: u8,
    divider: u8,
    reload_flag: bool,
    /// True for pulse 1 (one's-complement negate), false for pulse 2.
    ones_complement: bool,
}

impl Sweep {
    fn new(ones_complement: bool) -> Self {
        Self {
            enabled: false,
            negate: false,
            shift: 0,
            period: 0,
            divider: 0,
            reload_flag: false,
            ones_complement,
        }
    }

    /// Compute the target period given the current timer period.
    fn target_period(&self, current_period: u16) -> u16 {
        let shift_result = current_period >> self.shift;
        if self.negate {
            if self.ones_complement {
                current_period.wrapping_sub(shift_result).wrapping_sub(1)
            } else {
                current_period.wrapping_sub(shift_result)
            }
        } else {
            current_period.wrapping_add(shift_result)
        }
    }

    /// Whether the sweep is muting the channel.
    fn muting(&self, current_period: u16) -> bool {
        current_period < 8 || self.target_period(current_period) > 0x7FF
    }

    /// Clock the sweep (called at half-frame rate). Returns new timer period.
    fn clock(&mut self, current_period: u16) -> u16 {
        let mut new_period = current_period;

        if self.divider == 0 && self.enabled && self.shift > 0 && !self.muting(current_period) {
            let target = self.target_period(current_period);
            if target <= 0x7FF {
                new_period = target;
            }
        }

        if self.divider == 0 || self.reload_flag {
            self.divider = self.period;
            self.reload_flag = false;
        } else {
            self.divider -= 1;
        }

        new_period
    }
}

// ---------------------------------------------------------------------------
// Pulse channel
// ---------------------------------------------------------------------------

/// Pulse wave channel (two instances: pulse 1 and pulse 2).
struct Pulse {
    /// 11-bit timer period (from registers).
    timer_period: u16,
    /// Timer countdown.
    timer: u16,
    /// 8-step duty sequencer position.
    duty_pos: u8,
    /// Duty cycle selection (0–3).
    duty: u8,
    envelope: Envelope,
    length: LengthCounter,
    sweep: Sweep,
}

impl Pulse {
    fn new(ones_complement_negate: bool) -> Self {
        Self {
            timer_period: 0,
            timer: 0,
            duty_pos: 0,
            duty: 0,
            envelope: Envelope::new(),
            length: LengthCounter::new(),
            sweep: Sweep::new(ones_complement_negate),
        }
    }

    /// Clock the pulse timer (called at APU cycle rate = CPU/2).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.duty_pos = (self.duty_pos + 1) % 8;
        } else {
            self.timer -= 1;
        }
    }

    /// Current output (0–15).
    fn output(&self) -> u8 {
        if !self.length.active() {
            return 0;
        }
        if self.sweep.muting(self.timer_period) {
            return 0;
        }
        if !PULSE_DUTY[self.duty as usize][self.duty_pos as usize] {
            return 0;
        }
        self.envelope.output()
    }
}

// ---------------------------------------------------------------------------
// Triangle channel
// ---------------------------------------------------------------------------

/// Triangle wave channel. The timer ticks at CPU rate (not APU rate).
/// Uses a 32-step sequence and has both a length counter and a linear
/// counter.
struct Triangle {
    timer_period: u16,
    timer: u16,
    sequence_pos: u8,
    length: LengthCounter,
    /// Linear counter value.
    linear_counter: u8,
    /// Linear counter reload value (from register).
    linear_counter_reload: u8,
    /// Linear counter reload flag.
    linear_reload_flag: bool,
    /// Control flag (shared with length counter halt).
    control_flag: bool,
}

impl Triangle {
    fn new() -> Self {
        Self {
            timer_period: 0,
            timer: 0,
            sequence_pos: 0,
            length: LengthCounter::new(),
            linear_counter: 0,
            linear_counter_reload: 0,
            linear_reload_flag: false,
            control_flag: false,
        }
    }

    /// Clock the triangle timer (called every CPU cycle).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            // Only advance sequence when both counters are non-zero
            if self.length.active() && self.linear_counter > 0 {
                self.sequence_pos = (self.sequence_pos + 1) % 32;
            }
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the linear counter (called at quarter-frame rate).
    fn clock_linear_counter(&mut self) {
        if self.linear_reload_flag {
            self.linear_counter = self.linear_counter_reload;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
        if !self.control_flag {
            self.linear_reload_flag = false;
        }
    }

    /// Current output (0–15).
    fn output(&self) -> u8 {
        if !self.length.active() || self.linear_counter == 0 {
            return 0;
        }
        // Silence ultrasonic frequencies to avoid aliasing
        if self.timer_period < 2 {
            return 0;
        }
        TRIANGLE_SEQUENCE[self.sequence_pos as usize]
    }
}

// ---------------------------------------------------------------------------
// Noise channel
// ---------------------------------------------------------------------------

/// Noise channel. Uses a 15-bit LFSR with selectable feedback tap
/// (bit 1 for long mode, bit 6 for short mode).
struct Noise {
    timer_period: u16,
    timer: u16,
    /// 15-bit linear feedback shift register.
    shift_register: u16,
    /// Short mode: use bit 6 for feedback instead of bit 1.
    mode: bool,
    envelope: Envelope,
    length: LengthCounter,
}

impl Noise {
    fn new() -> Self {
        Self {
            timer_period: 0,
            timer: 0,
            shift_register: 1, // Initial state
            mode: false,
            envelope: Envelope::new(),
            length: LengthCounter::new(),
        }
    }

    /// Clock the noise timer (called at APU cycle rate = CPU/2).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            // Feedback: XOR bit 0 with bit 1 (normal) or bit 6 (short mode)
            let feedback_bit = if self.mode { 6 } else { 1 };
            let feedback =
                (self.shift_register & 1) ^ ((self.shift_register >> feedback_bit) & 1);
            self.shift_register >>= 1;
            self.shift_register |= feedback << 14;
        } else {
            self.timer -= 1;
        }
    }

    /// Current output (0–15).
    fn output(&self) -> u8 {
        if !self.length.active() {
            return 0;
        }
        // Bit 0 of shift register gates output (inverted: 0 = output, 1 = silence)
        if self.shift_register & 1 != 0 {
            return 0;
        }
        self.envelope.output()
    }
}

// ---------------------------------------------------------------------------
// DMC channel
// ---------------------------------------------------------------------------

/// DMC (delta modulation) channel. Fetches 1-bit delta-encoded samples from
/// PRG memory via DMA, producing drums, bass, and speech. A timer clocks
/// the output shift register; when the shift register is exhausted, the
/// sample buffer is loaded. When the sample buffer is empty and bytes
/// remain, `dma_pending` signals the tick loop to steal a CPU cycle.
pub(crate) struct Dmc {
    /// 7-bit output level (0–127), written directly by $4011.
    pub(crate) output_level: u8,
    /// IRQ enable flag (bit 7 of $4010).
    irq_enabled: bool,
    /// IRQ pending flag, read via bit 7 of $4015.
    pub(crate) irq_flag: bool,
    /// Loop flag (bit 6 of $4010).
    loop_flag: bool,
    /// Rate index (bits 0–3 of $4010).
    rate_index: u8,
    /// Countdown timer, clocked every CPU cycle.
    timer: u16,
    /// Timer reload value from `DMC_RATE_TABLE[rate_index]`.
    timer_period: u16,
    /// Starting sample address (from $4012).
    sample_address: u16,
    /// Total sample length in bytes (from $4013).
    sample_length: u16,
    /// Current DMA fetch address.
    pub(crate) current_address: u16,
    /// Bytes remaining to fetch.
    pub(crate) bytes_remaining: u16,
    /// Last byte fetched from memory.
    sample_buffer: u8,
    /// True when the sample buffer has been consumed.
    sample_buffer_empty: bool,
    /// 8-bit output shift register.
    shift_register: u8,
    /// Bits remaining in the shift register (counts down from 8).
    bits_remaining: u8,
    /// True when no sample data is available for output.
    silence_flag: bool,
    /// Controlled by bit 4 of $4015.
    enabled: bool,
    /// Signals the tick loop to steal a CPU cycle for a DMA fetch.
    pub(crate) dma_pending: bool,
}

impl Dmc {
    fn new() -> Self {
        Self {
            output_level: 0,
            irq_enabled: false,
            irq_flag: false,
            loop_flag: false,
            rate_index: 0,
            timer: DMC_RATE_TABLE_NTSC[0],
            timer_period: DMC_RATE_TABLE_NTSC[0],
            sample_address: 0xC000,
            sample_length: 1,
            current_address: 0xC000,
            bytes_remaining: 0,
            sample_buffer: 0,
            sample_buffer_empty: true,
            shift_register: 0,
            bits_remaining: 8,
            silence_flag: true,
            enabled: false,
            dma_pending: false,
        }
    }

    /// Clock the DMC timer. Called every CPU cycle.
    fn tick(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.clock_output();
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the output unit: shift one bit and update `output_level`.
    fn clock_output(&mut self) {
        // Update output level from the shift register
        if !self.silence_flag {
            if self.shift_register & 1 != 0 {
                if self.output_level <= 125 {
                    self.output_level += 2;
                }
            } else if self.output_level >= 2 {
                self.output_level -= 2;
            }
            self.shift_register >>= 1;
        }

        // Count down bits; reload from sample buffer when exhausted
        self.bits_remaining -= 1;
        if self.bits_remaining == 0 {
            self.bits_remaining = 8;
            if self.sample_buffer_empty {
                self.silence_flag = true;
            } else {
                self.silence_flag = false;
                self.shift_register = self.sample_buffer;
                self.sample_buffer_empty = true;
            }
            // Request the next byte if there are more to fetch
            if self.sample_buffer_empty && self.bytes_remaining > 0 {
                self.dma_pending = true;
            }
        }
    }

    /// Deliver a byte fetched by the DMA controller.
    pub(crate) fn receive_dma_byte(&mut self, byte: u8) {
        self.sample_buffer = byte;
        self.sample_buffer_empty = false;
        self.dma_pending = false;

        // Advance address (wraps $FFFF → $8000)
        self.current_address = if self.current_address == 0xFFFF {
            0x8000
        } else {
            self.current_address + 1
        };

        self.bytes_remaining -= 1;
        if self.bytes_remaining == 0 {
            if self.loop_flag {
                self.current_address = self.sample_address;
                self.bytes_remaining = self.sample_length;
            } else if self.irq_enabled {
                self.irq_flag = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Frame counter
// ---------------------------------------------------------------------------

/// Frame counter mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameCounterMode {
    /// 4-step: generates IRQ, 4 quarter-frame events per sequence.
    FourStep,
    /// 5-step: no IRQ, 5 quarter-frame events per sequence.
    FiveStep,
}

// Frame counter step boundaries (CPU cycles).
// NTSC 4-step: events at 7457, 14913, 22371, 29829 (IRQ at step 4)
// NTSC 5-step: events at 7457, 14913, 22371, 29829, 37281 (no IRQ)
const FOUR_STEP_SEQUENCE_NTSC: [u16; 4] = [7457, 14913, 22371, 29829];
const FIVE_STEP_SEQUENCE_NTSC: [u16; 5] = [7457, 14913, 22371, 29829, 37281];

// PAL frame counter boundaries (adjusted for ~50 Hz frame rate).
const FOUR_STEP_SEQUENCE_PAL: [u16; 4] = [8313, 16627, 24939, 33253];
const FIVE_STEP_SEQUENCE_PAL: [u16; 5] = [8313, 16627, 24939, 33253, 41565];

// ---------------------------------------------------------------------------
// APU
// ---------------------------------------------------------------------------

/// NES APU.
pub struct Apu {
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    pub(crate) dmc: Dmc,

    // Frame counter
    frame_mode: FrameCounterMode,
    frame_counter: u16,
    frame_step: u8,
    frame_irq_inhibit: bool,
    frame_irq_flag: bool,

    /// CPU cycle parity: true on odd CPU cycles (pulse/noise tick on even).
    odd_cycle: bool,

    // Region-dependent tables
    noise_period_table: &'static [u16; 16],
    dmc_rate_table: &'static [u16; 16],
    four_step_seq: &'static [u16; 4],
    five_step_seq: &'static [u16; 5],

    // Downsampling
    accumulator: f32,
    sample_count: u32,
    ticks_per_sample: f32,
    buffer: Vec<f32>,

    // DC-blocking high-pass filter (applied at output sample rate).
    // Removes the large DC offset inherent in the non-linear mixer.
    // First-order high-pass: y[n] = α * (y[n-1] + x[n] - x[n-1])
    // Cutoff ~37 Hz at 48 kHz: α ≈ 0.9952
    hp_prev_in: f32,
    hp_prev_out: f32,
}

impl Apu {
    /// Output sample rate.
    const SAMPLE_RATE: u32 = 48_000;

    #[must_use]
    pub fn new() -> Self {
        Self::new_with_cpu_freq(crate::config::NesRegion::Ntsc)
    }

    /// Create an APU with region-specific timing tables.
    #[must_use]
    pub fn new_with_cpu_freq(region: crate::config::NesRegion) -> Self {
        let cpu_freq = region.cpu_hz();
        let (noise_table, dmc_table, four_step, five_step) = match region {
            crate::config::NesRegion::Ntsc => (
                &NOISE_PERIOD_TABLE_NTSC,
                &DMC_RATE_TABLE_NTSC,
                &FOUR_STEP_SEQUENCE_NTSC,
                &FIVE_STEP_SEQUENCE_NTSC,
            ),
            crate::config::NesRegion::Pal => (
                &NOISE_PERIOD_TABLE_PAL,
                &DMC_RATE_TABLE_PAL,
                &FOUR_STEP_SEQUENCE_PAL,
                &FIVE_STEP_SEQUENCE_PAL,
            ),
        };
        Self {
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
            frame_mode: FrameCounterMode::FourStep,
            frame_counter: 0,
            frame_step: 0,
            frame_irq_inhibit: false,
            frame_irq_flag: false,
            odd_cycle: false,
            noise_period_table: noise_table,
            dmc_rate_table: dmc_table,
            four_step_seq: four_step,
            five_step_seq: five_step,
            accumulator: 0.0,
            sample_count: 0,
            ticks_per_sample: cpu_freq as f32 / Self::SAMPLE_RATE as f32,
            buffer: Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
        }
    }

    /// Read an APU register ($4015 is the only readable APU register).
    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x4015 => {
                let mut status = 0u8;
                if self.pulse1.length.active() {
                    status |= 0x01;
                }
                if self.pulse2.length.active() {
                    status |= 0x02;
                }
                if self.triangle.length.active() {
                    status |= 0x04;
                }
                if self.noise.length.active() {
                    status |= 0x08;
                }
                if self.dmc.bytes_remaining > 0 {
                    status |= 0x10;
                }
                if self.frame_irq_flag {
                    status |= 0x40;
                }
                if self.dmc.irq_flag {
                    status |= 0x80;
                }
                // Reading $4015 clears the frame IRQ flag
                self.frame_irq_flag = false;
                status
            }
            _ => 0,
        }
    }

    /// Write an APU register ($4000–$4013, $4015, $4017).
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Pulse 1: $4000–$4003
            0x4000 => {
                self.pulse1.duty = (value >> 6) & 0x03;
                self.pulse1.envelope.loop_flag = value & 0x20 != 0;
                self.pulse1.length.halt = value & 0x20 != 0;
                self.pulse1.envelope.constant_volume = value & 0x10 != 0;
                self.pulse1.envelope.volume = value & 0x0F;
            }
            0x4001 => {
                self.pulse1.sweep.enabled = value & 0x80 != 0;
                self.pulse1.sweep.period = (value >> 4) & 0x07;
                self.pulse1.sweep.negate = value & 0x08 != 0;
                self.pulse1.sweep.shift = value & 0x07;
                self.pulse1.sweep.reload_flag = true;
            }
            0x4002 => {
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0x0700) | u16::from(value);
            }
            0x4003 => {
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                self.pulse1.length.load((value >> 3) & 0x1F);
                self.pulse1.envelope.start_flag = true;
                self.pulse1.duty_pos = 0;
            }

            // Pulse 2: $4004–$4007
            0x4004 => {
                self.pulse2.duty = (value >> 6) & 0x03;
                self.pulse2.envelope.loop_flag = value & 0x20 != 0;
                self.pulse2.length.halt = value & 0x20 != 0;
                self.pulse2.envelope.constant_volume = value & 0x10 != 0;
                self.pulse2.envelope.volume = value & 0x0F;
            }
            0x4005 => {
                self.pulse2.sweep.enabled = value & 0x80 != 0;
                self.pulse2.sweep.period = (value >> 4) & 0x07;
                self.pulse2.sweep.negate = value & 0x08 != 0;
                self.pulse2.sweep.shift = value & 0x07;
                self.pulse2.sweep.reload_flag = true;
            }
            0x4006 => {
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0x0700) | u16::from(value);
            }
            0x4007 => {
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                self.pulse2.length.load((value >> 3) & 0x1F);
                self.pulse2.envelope.start_flag = true;
                self.pulse2.duty_pos = 0;
            }

            // Triangle: $4008–$400B
            0x4008 => {
                self.triangle.control_flag = value & 0x80 != 0;
                self.triangle.length.halt = value & 0x80 != 0;
                self.triangle.linear_counter_reload = value & 0x7F;
            }
            0x4009 => {} // Unused
            0x400A => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x0700) | u16::from(value);
            }
            0x400B => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                self.triangle.length.load((value >> 3) & 0x1F);
                self.triangle.linear_reload_flag = true;
            }

            // Noise: $400C–$400F
            0x400C => {
                self.noise.envelope.loop_flag = value & 0x20 != 0;
                self.noise.length.halt = value & 0x20 != 0;
                self.noise.envelope.constant_volume = value & 0x10 != 0;
                self.noise.envelope.volume = value & 0x0F;
            }
            0x400D => {} // Unused
            0x400E => {
                self.noise.mode = value & 0x80 != 0;
                self.noise.timer_period = self.noise_period_table[(value & 0x0F) as usize];
            }
            0x400F => {
                self.noise.length.load((value >> 3) & 0x1F);
                self.noise.envelope.start_flag = true;
            }

            // DMC: $4010–$4013
            0x4010 => {
                self.dmc.irq_enabled = value & 0x80 != 0;
                self.dmc.loop_flag = value & 0x40 != 0;
                self.dmc.rate_index = value & 0x0F;
                self.dmc.timer_period = self.dmc_rate_table[self.dmc.rate_index as usize];
                if !self.dmc.irq_enabled {
                    self.dmc.irq_flag = false;
                }
            }
            0x4011 => {
                // Direct load: 7-bit output level
                self.dmc.output_level = value & 0x7F;
            }
            0x4012 => {
                self.dmc.sample_address = 0xC000 + u16::from(value) * 64;
            }
            0x4013 => {
                self.dmc.sample_length = u16::from(value) * 16 + 1;
            }

            // Status: $4015
            0x4015 => {
                self.pulse1.length.set_enabled(value & 0x01 != 0);
                self.pulse2.length.set_enabled(value & 0x02 != 0);
                self.triangle.length.set_enabled(value & 0x04 != 0);
                self.noise.length.set_enabled(value & 0x08 != 0);

                // DMC enable (bit 4)
                let dmc_enable = value & 0x10 != 0;
                if dmc_enable {
                    if self.dmc.bytes_remaining == 0 {
                        self.dmc.current_address = self.dmc.sample_address;
                        self.dmc.bytes_remaining = self.dmc.sample_length;
                        if self.dmc.sample_buffer_empty {
                            self.dmc.dma_pending = true;
                        }
                    }
                } else {
                    self.dmc.bytes_remaining = 0;
                }
                self.dmc.enabled = dmc_enable;
                self.dmc.irq_flag = false;
            }

            // Frame counter: $4017
            0x4017 => {
                self.frame_mode = if value & 0x80 != 0 {
                    FrameCounterMode::FiveStep
                } else {
                    FrameCounterMode::FourStep
                };
                self.frame_irq_inhibit = value & 0x40 != 0;
                if self.frame_irq_inhibit {
                    self.frame_irq_flag = false;
                }
                // Reset frame counter
                self.frame_counter = 0;
                self.frame_step = 0;
                // In 5-step mode, immediately clock all units
                if self.frame_mode == FrameCounterMode::FiveStep {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                }
            }

            _ => {}
        }
    }

    /// Tick the APU one CPU cycle.
    pub fn tick(&mut self) {
        // Triangle timer ticks every CPU cycle
        self.triangle.clock_timer();

        // Pulse and noise timers tick every other CPU cycle (APU cycle)
        if self.odd_cycle {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }
        self.odd_cycle = !self.odd_cycle;

        // DMC timer ticks every CPU cycle
        self.dmc.tick();

        // Frame counter
        self.clock_frame_counter();

        // Mix and downsample
        let sample = self.mix();
        self.accumulator += sample;
        self.sample_count += 1;

        if self.sample_count as f32 >= self.ticks_per_sample {
            let avg = self.accumulator / self.sample_count as f32;

            // DC-blocking high-pass filter: removes mixer's inherent DC offset.
            // y[n] = α * (y[n-1] + x[n] - x[n-1]), α ≈ 0.9952 (~37 Hz at 48 kHz)
            const ALPHA: f32 = 0.9952;
            let filtered = ALPHA * (self.hp_prev_out + avg - self.hp_prev_in);
            self.hp_prev_in = avg;
            self.hp_prev_out = filtered;

            self.buffer.push(filtered);
            self.accumulator = 0.0;
            self.sample_count = 0;
        }
    }

    /// Clock the frame counter. Generates quarter-frame and half-frame
    /// events at the appropriate CPU cycle counts.
    fn clock_frame_counter(&mut self) {
        self.frame_counter += 1;

        match self.frame_mode {
            FrameCounterMode::FourStep => {
                if self.frame_step < 4
                    && self.frame_counter >= self.four_step_seq[self.frame_step as usize]
                {
                    match self.frame_step {
                        0 => self.clock_quarter_frame(),
                        1 => {
                            self.clock_quarter_frame();
                            self.clock_half_frame();
                        }
                        2 => self.clock_quarter_frame(),
                        3 => {
                            self.clock_quarter_frame();
                            self.clock_half_frame();
                            if !self.frame_irq_inhibit {
                                self.frame_irq_flag = true;
                            }
                            self.frame_counter = 0;
                        }
                        _ => {}
                    }
                    self.frame_step += 1;
                    if self.frame_step >= 4 {
                        self.frame_step = 0;
                    }
                }
            }
            FrameCounterMode::FiveStep => {
                if self.frame_step < 5
                    && self.frame_counter >= self.five_step_seq[self.frame_step as usize]
                {
                    match self.frame_step {
                        0 => self.clock_quarter_frame(),
                        1 => {
                            self.clock_quarter_frame();
                            self.clock_half_frame();
                        }
                        2 => self.clock_quarter_frame(),
                        3 => {} // No clocking on step 4 of 5-step
                        4 => {
                            self.clock_quarter_frame();
                            self.clock_half_frame();
                            self.frame_counter = 0;
                        }
                        _ => {}
                    }
                    self.frame_step += 1;
                    if self.frame_step >= 5 {
                        self.frame_step = 0;
                    }
                }
            }
        }
    }

    /// Quarter-frame: clock envelopes and triangle linear counter.
    fn clock_quarter_frame(&mut self) {
        self.pulse1.envelope.clock();
        self.pulse2.envelope.clock();
        self.noise.envelope.clock();
        self.triangle.clock_linear_counter();
    }

    /// Half-frame: clock length counters and sweep units.
    fn clock_half_frame(&mut self) {
        self.pulse1.length.clock();
        self.pulse2.length.clock();
        self.triangle.length.clock();
        self.noise.length.clock();
        let p = self.pulse1.sweep.clock(self.pulse1.timer_period);
        self.pulse1.timer_period = p;
        let p = self.pulse2.sweep.clock(self.pulse2.timer_period);
        self.pulse2.timer_period = p;
    }

    /// Non-linear mixer (nesdev formula).
    fn mix(&self) -> f32 {
        let p1 = self.pulse1.output() as f32;
        let p2 = self.pulse2.output() as f32;
        let tri = self.triangle.output() as f32;
        let noi = self.noise.output() as f32;
        let dmc = self.dmc.output_level as f32;

        let pulse_out = if p1 + p2 > 0.0 {
            95.88 / (8128.0 / (p1 + p2) + 100.0)
        } else {
            0.0
        };

        let tnd_sum = tri / 8227.0 + noi / 12241.0 + dmc / 22638.0;
        let tnd_out = if tnd_sum > 0.0 {
            159.79 / (1.0 / tnd_sum + 100.0)
        } else {
            0.0
        };

        // Raw mixer output is 0.0 to ~0.8. The DC-blocking high-pass
        // filter in the downsample path centres this around zero.
        pulse_out + tnd_out
    }

    /// Whether an IRQ is pending (frame counter or DMC).
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        self.frame_irq_flag || self.dmc.irq_flag
    }

    /// Take the audio output buffer (drains it).
    ///
    /// Returns mono f32 samples in the range -1.0 to 1.0, at 48 kHz.
    pub fn take_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    /// Number of audio samples pending in the buffer.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    // -----------------------------------------------------------------------
    // Observable state
    // -----------------------------------------------------------------------

    /// Query APU state by path.
    #[must_use]
    pub fn query(&self, path: &str) -> Option<emu_core::Value> {
        match path {
            "pulse1.period" => Some(self.pulse1.timer_period.into()),
            "pulse1.length" => Some(self.pulse1.length.counter.into()),
            "pulse1.envelope" => Some(self.pulse1.envelope.output().into()),
            "pulse1.duty" => Some(self.pulse1.duty.into()),
            "pulse2.period" => Some(self.pulse2.timer_period.into()),
            "pulse2.length" => Some(self.pulse2.length.counter.into()),
            "pulse2.envelope" => Some(self.pulse2.envelope.output().into()),
            "pulse2.duty" => Some(self.pulse2.duty.into()),
            "triangle.period" => Some(self.triangle.timer_period.into()),
            "triangle.length" => Some(self.triangle.length.counter.into()),
            "triangle.linear" => Some(self.triangle.linear_counter.into()),
            "noise.period" => Some(self.noise.timer_period.into()),
            "noise.length" => Some(self.noise.length.counter.into()),
            "noise.envelope" => Some(self.noise.envelope.output().into()),
            "frame_counter.mode" => {
                let mode: u8 = match self.frame_mode {
                    FrameCounterMode::FourStep => 0,
                    FrameCounterMode::FiveStep => 1,
                };
                Some(mode.into())
            }
            _ => None,
        }
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_by_default() {
        let mut apu = Apu::new();
        // Tick for enough cycles to produce samples
        for _ in 0..100 {
            apu.tick();
        }
        let buf = apu.take_buffer();
        // With DC-blocking filter, silence should be near zero
        for &s in &buf {
            assert!(
                s.abs() < 0.1,
                "Silent APU should produce near-zero output, got {s}"
            );
        }
    }

    #[test]
    fn pulse_produces_audio() {
        let mut apu = Apu::new();

        // Enable pulse 1
        apu.write(0x4015, 0x01);
        // Duty 50%, constant volume 15
        apu.write(0x4000, 0xBF); // 10_1_1_1111
        // Timer period low = $FD (A4 = 440 Hz → period ≈ 253)
        apu.write(0x4002, 0xFD);
        // Timer period high + length load
        apu.write(0x4003, 0x00 | (0x01 << 3)); // period high = 0, length index = 1

        // Need enough ticks for multiple full 8-step duty cycles.
        // One duty cycle = 8 * (period+1) * 2 CPU ticks = 8 * 254 * 2 = 4064.
        // Run 10000 to see several cycles.
        for _ in 0..10000 {
            apu.tick();
        }

        let buf = apu.take_buffer();
        assert!(!buf.is_empty(), "Pulse channel should produce audio samples");

        // Check that output isn't flat — should have variation from duty cycle
        let min = buf.iter().copied().fold(f32::INFINITY, f32::min);
        let max = buf.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 0.01,
            "Pulse output should have dynamic range, got min={min} max={max}"
        );
    }

    #[test]
    fn triangle_produces_audio() {
        let mut apu = Apu::new();

        // Enable triangle
        apu.write(0x4015, 0x04);
        // Linear counter reload = 127, control flag set
        apu.write(0x4008, 0xFF);
        // Timer period
        apu.write(0x400A, 0xFD);
        apu.write(0x400B, 0x00 | (0x01 << 3)); // length index = 1

        // Triangle needs a quarter-frame event (7457 CPU cycles) to load
        // its linear counter from the reload value. Run 10000 ticks to
        // pass the first quarter-frame and produce audible output.
        for _ in 0..10000 {
            apu.tick();
        }

        let buf = apu.take_buffer();
        assert!(
            !buf.is_empty(),
            "Triangle channel should produce audio samples"
        );

        let min = buf.iter().copied().fold(f32::INFINITY, f32::min);
        let max = buf.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 0.01,
            "Triangle output should have dynamic range, got min={min} max={max}"
        );
    }

    #[test]
    fn noise_produces_audio() {
        let mut apu = Apu::new();

        // Enable noise
        apu.write(0x4015, 0x08);
        // Constant volume 15
        apu.write(0x400C, 0x3F); // halt + constant + vol=15
        // Period index 4 (medium)
        apu.write(0x400E, 0x04);
        // Length load
        apu.write(0x400F, 0x01 << 3);

        for _ in 0..2000 {
            apu.tick();
        }

        let buf = apu.take_buffer();
        assert!(!buf.is_empty(), "Noise channel should produce audio samples");

        let min = buf.iter().copied().fold(f32::INFINITY, f32::min);
        let max = buf.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 0.01,
            "Noise output should have dynamic range, got min={min} max={max}"
        );
    }

    #[test]
    fn status_register_reflects_length() {
        let mut apu = Apu::new();

        // Enable pulse 1 and triangle
        apu.write(0x4015, 0x05);
        // Load pulse 1 length
        apu.write(0x4003, 0x01 << 3); // length index 1 = 254
        // Load triangle length
        apu.write(0x4008, 0xFF);
        apu.write(0x400B, 0x01 << 3); // length index 1 = 254

        let status = apu.read(0x4015);
        assert!(status & 0x01 != 0, "Pulse 1 length should be active");
        assert!(status & 0x04 != 0, "Triangle length should be active");
        assert!(status & 0x02 == 0, "Pulse 2 length should be inactive");
        assert!(status & 0x08 == 0, "Noise length should be inactive");
    }

    #[test]
    fn disable_channel_clears_length() {
        let mut apu = Apu::new();

        // Enable pulse 1
        apu.write(0x4015, 0x01);
        apu.write(0x4003, 0x01 << 3); // Load length
        assert!(apu.read(0x4015) & 0x01 != 0);

        // Disable pulse 1
        apu.write(0x4015, 0x00);
        assert!(apu.read(0x4015) & 0x01 == 0, "Length should be cleared");
    }

    #[test]
    fn frame_irq_in_four_step_mode() {
        let mut apu = Apu::new();

        // 4-step mode, IRQ enabled
        apu.write(0x4017, 0x00);

        // Tick through a full 4-step sequence (~29830 CPU cycles)
        for _ in 0..29830 {
            apu.tick();
        }

        assert!(
            apu.irq_pending(),
            "Frame counter IRQ should fire in 4-step mode"
        );
    }

    #[test]
    fn no_irq_in_five_step_mode() {
        let mut apu = Apu::new();

        // 5-step mode
        apu.write(0x4017, 0x80);

        // Tick through a full sequence
        for _ in 0..40000 {
            apu.tick();
        }

        assert!(
            !apu.frame_irq_flag,
            "Frame counter should not generate IRQ in 5-step mode"
        );
    }

    #[test]
    fn buffer_drain() {
        let mut apu = Apu::new();

        for _ in 0..1000 {
            apu.tick();
        }

        let len1 = apu.buffer_len();
        assert!(len1 > 0, "Buffer should have samples after ticking");

        let buf = apu.take_buffer();
        assert_eq!(buf.len(), len1);
        assert_eq!(apu.buffer_len(), 0, "Buffer should be empty after take");
    }

    #[test]
    fn dmc_direct_load() {
        let mut apu = Apu::new();

        apu.write(0x4011, 0x40); // Direct load = 64

        // The DMC output level affects the mix
        let sample = apu.mix();
        // With only DMC at 64, tnd_out should be non-zero
        assert!(sample > -1.0, "DMC direct load should shift output, got {sample}");
    }

    // -----------------------------------------------------------------------
    // DMC DMA tests
    // -----------------------------------------------------------------------

    #[test]
    fn dmc_rate_table_length() {
        assert_eq!(DMC_RATE_TABLE_NTSC.len(), 16);
        assert_eq!(DMC_RATE_TABLE_PAL.len(), 16);
    }

    #[test]
    fn dmc_address_formula() {
        let mut apu = Apu::new();
        // $4012 value 0 → $C000, value 1 → $C040, value $FF → $FFC0
        apu.write(0x4012, 0x00);
        assert_eq!(apu.dmc.sample_address, 0xC000);
        apu.write(0x4012, 0x01);
        assert_eq!(apu.dmc.sample_address, 0xC040);
        apu.write(0x4012, 0xFF);
        assert_eq!(apu.dmc.sample_address, 0xFFC0);
    }

    #[test]
    fn dmc_length_formula() {
        let mut apu = Apu::new();
        // $4013 value 0 → 1, value 1 → 17, value $FF → 4081
        apu.write(0x4013, 0x00);
        assert_eq!(apu.dmc.sample_length, 1);
        apu.write(0x4013, 0x01);
        assert_eq!(apu.dmc.sample_length, 17);
        apu.write(0x4013, 0xFF);
        assert_eq!(apu.dmc.sample_length, 4081);
    }

    #[test]
    fn dmc_enable_starts_sample() {
        let mut apu = Apu::new();
        apu.write(0x4012, 0x00); // address = $C000
        apu.write(0x4013, 0x01); // length = 17
        apu.write(0x4015, 0x10); // enable DMC

        assert!(apu.dmc.bytes_remaining > 0, "DMC should have bytes to fetch");
        assert!(apu.dmc.dma_pending, "DMC should request first DMA fetch");
        assert_eq!(apu.dmc.current_address, 0xC000);
    }

    #[test]
    fn dmc_disable_stops() {
        let mut apu = Apu::new();
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x01);
        apu.write(0x4015, 0x10); // enable
        assert!(apu.dmc.bytes_remaining > 0);

        apu.write(0x4015, 0x00); // disable
        assert_eq!(apu.dmc.bytes_remaining, 0, "DMC should stop immediately");
    }

    #[test]
    fn dmc_status_bit4_active() {
        let mut apu = Apu::new();
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x01);
        apu.write(0x4015, 0x10);

        let status = apu.read(0x4015);
        assert!(status & 0x10 != 0, "Bit 4 should reflect DMC active");
    }

    #[test]
    fn dmc_timer_output_changes() {
        let mut apu = Apu::new();
        // Set rate index 0 (period = 428)
        apu.write(0x4010, 0x00);
        // Start at output_level 64
        apu.write(0x4011, 64);

        // Manually feed a byte with all 1-bits into the DMC
        apu.dmc.sample_buffer = 0xFF;
        apu.dmc.sample_buffer_empty = false;
        apu.dmc.silence_flag = false;
        apu.dmc.shift_register = 0xFF;
        apu.dmc.bits_remaining = 8;

        let before = apu.dmc.output_level;

        // Tick through one full timer period + 1 to trigger an output clock
        for _ in 0..=(apu.dmc.timer_period + 1) {
            apu.dmc.tick();
        }

        assert_ne!(
            apu.dmc.output_level, before,
            "Output level should change after clocking the shift register"
        );
    }

    #[test]
    fn dmc_loop_restarts() {
        let mut apu = Apu::new();
        apu.write(0x4010, 0x40); // loop flag set, no IRQ
        apu.write(0x4012, 0x00); // address = $C000
        apu.write(0x4013, 0x00); // length = 1
        apu.write(0x4015, 0x10); // enable

        // Deliver the single byte — should restart
        apu.dmc.receive_dma_byte(0xAA);
        assert_eq!(
            apu.dmc.bytes_remaining, 1,
            "Loop should restart bytes_remaining"
        );
        assert_eq!(
            apu.dmc.current_address, 0xC000,
            "Loop should reset address"
        );
    }

    #[test]
    fn dmc_irq_at_end() {
        let mut apu = Apu::new();
        apu.write(0x4010, 0x80); // IRQ enabled, no loop
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x00); // length = 1
        apu.write(0x4015, 0x10); // enable (clears irq_flag)

        apu.dmc.receive_dma_byte(0x00);
        assert!(
            apu.dmc.irq_flag,
            "IRQ flag should be set when sample ends with IRQ enabled"
        );
    }

    #[test]
    fn dmc_irq_disabled_no_flag() {
        let mut apu = Apu::new();
        apu.write(0x4010, 0x00); // no IRQ, no loop
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x00); // length = 1
        apu.write(0x4015, 0x10); // enable

        apu.dmc.receive_dma_byte(0x00);
        assert!(
            !apu.dmc.irq_flag,
            "IRQ flag should not be set when IRQ is disabled"
        );
    }
}
