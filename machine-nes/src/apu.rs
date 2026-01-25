//! NES Audio Processing Unit (APU).
//!
//! The APU generates audio through 5 channels:
//! - 2 pulse wave channels (square waves with duty cycle)
//! - 1 triangle wave channel
//! - 1 noise channel
//! - 1 DMC (delta modulation) channel for samples

/// Duty cycle waveforms for pulse channels.
/// Each entry is an 8-step sequence where 1 = high, 0 = low.
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0], // 12.5%
    [0, 1, 1, 0, 0, 0, 0, 0], // 25%
    [0, 1, 1, 1, 1, 0, 0, 0], // 50%
    [1, 0, 0, 1, 1, 1, 1, 1], // 75% (inverted 25%)
];

/// Triangle waveform sequence (32 steps).
const TRIANGLE_TABLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
];

/// Noise period lookup table (NTSC).
const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

/// DMC rate lookup table (NTSC, in CPU cycles).
const DMC_RATE_TABLE: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

/// Length counter lookup table.
const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14,
    12, 16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30,
];

/// Pulse channel state.
#[derive(Clone)]
pub struct PulseChannel {
    /// Channel number (0 or 1) - affects sweep negate behavior.
    channel_num: u8,
    /// Duty cycle (0-3).
    duty: u8,
    /// Length counter halt / envelope loop.
    halt: bool,
    /// Constant volume flag.
    constant_volume: bool,
    /// Volume / envelope period.
    volume: u8,
    /// Sweep enabled.
    sweep_enabled: bool,
    /// Sweep period.
    sweep_period: u8,
    /// Sweep negate.
    sweep_negate: bool,
    /// Sweep shift.
    sweep_shift: u8,
    /// Timer period (11-bit).
    timer_period: u16,
    /// Length counter.
    length_counter: u8,
    /// Channel enabled.
    enabled: bool,

    // Internal state
    timer: u16,
    sequencer_pos: u8,
    envelope_counter: u8,
    envelope_divider: u8,
    envelope_start: bool,
    sweep_divider: u8,
    sweep_reload: bool,
    target_period: u16,
}

impl PulseChannel {
    fn new(channel_num: u8) -> Self {
        Self {
            channel_num,
            duty: 0,
            halt: false,
            constant_volume: false,
            volume: 0,
            sweep_enabled: false,
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            timer_period: 0,
            length_counter: 0,
            enabled: false,
            timer: 0,
            sequencer_pos: 0,
            envelope_counter: 0,
            envelope_divider: 0,
            envelope_start: false,
            sweep_divider: 0,
            sweep_reload: false,
            target_period: 0,
        }
    }

    /// Clock the timer (called every APU cycle = every 2 CPU cycles).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.sequencer_pos = (self.sequencer_pos + 1) & 0x07;
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the envelope (called by frame counter).
    fn clock_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_start = false;
            self.envelope_counter = 15;
            self.envelope_divider = self.volume;
        } else if self.envelope_divider > 0 {
            self.envelope_divider -= 1;
        } else {
            self.envelope_divider = self.volume;
            if self.envelope_counter > 0 {
                self.envelope_counter -= 1;
            } else if self.halt {
                self.envelope_counter = 15;
            }
        }
    }

    /// Clock the sweep unit (called by frame counter).
    fn clock_sweep(&mut self) {
        // Calculate target period
        let change = self.timer_period >> self.sweep_shift;
        self.target_period = if self.sweep_negate {
            self.timer_period.saturating_sub(change + if self.channel_num == 0 { 1 } else { 0 })
        } else {
            self.timer_period.saturating_add(change)
        };

        // Update period if sweep is active
        if self.sweep_divider == 0 && self.sweep_enabled && self.sweep_shift > 0 {
            if self.timer_period >= 8 && self.target_period <= 0x7FF {
                self.timer_period = self.target_period;
            }
        }

        if self.sweep_divider == 0 || self.sweep_reload {
            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
        } else {
            self.sweep_divider -= 1;
        }
    }

    /// Clock the length counter (called by frame counter).
    fn clock_length(&mut self) {
        if !self.halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    /// Get current output (0-15).
    fn output(&self) -> u8 {
        // Check if channel is silenced
        if !self.enabled || self.length_counter == 0 {
            return 0;
        }

        // Check timer period (muting for low frequencies)
        if self.timer_period < 8 {
            return 0;
        }

        // Check sweep target period
        if self.target_period > 0x7FF {
            return 0;
        }

        // Get duty cycle output
        if DUTY_TABLE[self.duty as usize][self.sequencer_pos as usize] == 0 {
            return 0;
        }

        // Return envelope or constant volume
        if self.constant_volume {
            self.volume
        } else {
            self.envelope_counter
        }
    }
}

/// Triangle channel state.
#[derive(Clone, Default)]
pub struct TriangleChannel {
    /// Linear counter reload value.
    linear_reload: u8,
    /// Control flag (length counter halt).
    control: bool,
    /// Timer period (11-bit).
    timer_period: u16,
    /// Length counter.
    length_counter: u8,
    /// Channel enabled.
    enabled: bool,

    // Internal state
    timer: u16,
    linear_counter: u8,
    linear_reload_flag: bool,
    sequencer_pos: u8,
}

impl TriangleChannel {
    fn new() -> Self {
        Self::default()
    }

    /// Clock the timer (called every CPU cycle).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            // Only step sequencer if counters are non-zero
            if self.length_counter > 0 && self.linear_counter > 0 {
                self.sequencer_pos = (self.sequencer_pos + 1) & 0x1F;
            }
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the linear counter (called by frame counter).
    fn clock_linear(&mut self) {
        if self.linear_reload_flag {
            self.linear_counter = self.linear_reload;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }

        if !self.control {
            self.linear_reload_flag = false;
        }
    }

    /// Clock the length counter (called by frame counter).
    fn clock_length(&mut self) {
        if !self.control && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    /// Get current output (0-15).
    fn output(&self) -> u8 {
        if !self.enabled || self.length_counter == 0 || self.linear_counter == 0 {
            return 0;
        }

        // Silence ultrasonic frequencies (period < 2)
        if self.timer_period < 2 {
            return 0;
        }

        TRIANGLE_TABLE[self.sequencer_pos as usize]
    }
}

/// Noise channel state.
#[derive(Clone)]
pub struct NoiseChannel {
    /// Length counter halt / envelope loop.
    halt: bool,
    /// Constant volume flag.
    constant_volume: bool,
    /// Volume / envelope period.
    volume: u8,
    /// Mode flag (short/long).
    mode: bool,
    /// Period index.
    period: u8,
    /// Length counter.
    length_counter: u8,
    /// Channel enabled.
    enabled: bool,

    // Internal state
    timer: u16,
    shift_register: u16,
    envelope_counter: u8,
    envelope_divider: u8,
    envelope_start: bool,
}

impl NoiseChannel {
    fn new() -> Self {
        Self {
            halt: false,
            constant_volume: false,
            volume: 0,
            mode: false,
            period: 0,
            length_counter: 0,
            enabled: false,
            timer: 0,
            shift_register: 1,
            envelope_counter: 0,
            envelope_divider: 0,
            envelope_start: false,
        }
    }

    /// Clock the timer (called every APU cycle).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = NOISE_PERIOD_TABLE[self.period as usize];

            // Clock LFSR
            let bit = if self.mode {
                // Mode 1: XOR bits 0 and 6
                ((self.shift_register >> 0) ^ (self.shift_register >> 6)) & 1
            } else {
                // Mode 0: XOR bits 0 and 1
                ((self.shift_register >> 0) ^ (self.shift_register >> 1)) & 1
            };

            self.shift_register = (self.shift_register >> 1) | (bit << 14);
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the envelope (called by frame counter).
    fn clock_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_start = false;
            self.envelope_counter = 15;
            self.envelope_divider = self.volume;
        } else if self.envelope_divider > 0 {
            self.envelope_divider -= 1;
        } else {
            self.envelope_divider = self.volume;
            if self.envelope_counter > 0 {
                self.envelope_counter -= 1;
            } else if self.halt {
                self.envelope_counter = 15;
            }
        }
    }

    /// Clock the length counter (called by frame counter).
    fn clock_length(&mut self) {
        if !self.halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    /// Get current output (0-15).
    fn output(&self) -> u8 {
        if !self.enabled || self.length_counter == 0 {
            return 0;
        }

        // Output is 0 if bit 0 of shift register is set
        if self.shift_register & 1 != 0 {
            return 0;
        }

        if self.constant_volume {
            self.volume
        } else {
            self.envelope_counter
        }
    }
}

/// DMC channel state.
#[derive(Clone, Default)]
pub struct DmcChannel {
    /// IRQ enabled.
    irq_enabled: bool,
    /// Loop flag.
    loop_flag: bool,
    /// Rate index.
    rate: u8,
    /// Sample address.
    sample_address: u16,
    /// Sample length.
    sample_length: u16,
    /// Channel enabled.
    enabled: bool,

    // Internal state
    timer: u16,
    output_level: u8,
    current_address: u16,
    bytes_remaining: u16,
    sample_buffer: u8,
    sample_buffer_empty: bool,
    bits_remaining: u8,
    shift_register: u8,
    irq_pending: bool,
    silence: bool,
}

impl DmcChannel {
    fn new() -> Self {
        Self {
            sample_buffer_empty: true,
            bits_remaining: 8,
            ..Default::default()
        }
    }

    /// Clock the timer (called every CPU cycle).
    /// Returns true if a memory read is needed.
    fn clock_timer(&mut self) -> bool {
        let mut need_read = false;

        if self.timer == 0 {
            self.timer = DMC_RATE_TABLE[self.rate as usize];

            if !self.silence {
                // Output unit
                if self.shift_register & 1 != 0 {
                    if self.output_level <= 125 {
                        self.output_level += 2;
                    }
                } else if self.output_level >= 2 {
                    self.output_level -= 2;
                }
            }

            self.shift_register >>= 1;
            self.bits_remaining -= 1;

            if self.bits_remaining == 0 {
                self.bits_remaining = 8;

                if self.sample_buffer_empty {
                    self.silence = true;
                } else {
                    self.silence = false;
                    self.shift_register = self.sample_buffer;
                    self.sample_buffer_empty = true;
                    need_read = self.bytes_remaining > 0;
                }
            }
        } else {
            self.timer -= 1;
        }

        need_read
    }

    /// Load a sample byte from memory.
    fn load_sample(&mut self, value: u8) {
        self.sample_buffer = value;
        self.sample_buffer_empty = false;

        // Advance address (wraps at $FFFF to $8000)
        self.current_address = if self.current_address == 0xFFFF {
            0x8000
        } else {
            self.current_address + 1
        };

        self.bytes_remaining -= 1;

        if self.bytes_remaining == 0 {
            if self.loop_flag {
                self.restart();
            } else if self.irq_enabled {
                self.irq_pending = true;
            }
        }
    }

    /// Restart sample playback.
    fn restart(&mut self) {
        self.current_address = self.sample_address;
        self.bytes_remaining = self.sample_length;
    }

    /// Get current output (0-127).
    fn output(&self) -> u8 {
        self.output_level
    }
}

/// NES APU.
#[derive(Clone)]
pub struct Apu {
    /// Pulse channel 1.
    pub pulse1: PulseChannel,
    /// Pulse channel 2.
    pub pulse2: PulseChannel,
    /// Triangle channel.
    pub triangle: TriangleChannel,
    /// Noise channel.
    pub noise: NoiseChannel,
    /// DMC channel.
    pub dmc: DmcChannel,
    /// Frame counter mode (false = 4-step, true = 5-step).
    frame_mode: bool,
    /// Frame IRQ inhibit.
    frame_irq_inhibit: bool,
    /// Frame IRQ pending.
    pub frame_irq: bool,

    // Internal state
    frame_counter: u32,
    cycle: u64,
    /// Pending DMC read address.
    dmc_read_pending: Option<u16>,
}

impl Apu {
    /// Create a new APU.
    pub fn new() -> Self {
        Self {
            pulse1: PulseChannel::new(0),
            pulse2: PulseChannel::new(1),
            triangle: TriangleChannel::new(),
            noise: NoiseChannel::new(),
            dmc: DmcChannel::new(),
            frame_mode: false,
            frame_irq_inhibit: false,
            frame_irq: false,
            frame_counter: 0,
            cycle: 0,
            dmc_read_pending: None,
        }
    }

    /// Reset the APU.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Write to APU register.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Pulse 1
            0x4000 => {
                self.pulse1.duty = (value >> 6) & 0x03;
                self.pulse1.halt = value & 0x20 != 0;
                self.pulse1.constant_volume = value & 0x10 != 0;
                self.pulse1.volume = value & 0x0F;
            }
            0x4001 => {
                self.pulse1.sweep_enabled = value & 0x80 != 0;
                self.pulse1.sweep_period = (value >> 4) & 0x07;
                self.pulse1.sweep_negate = value & 0x08 != 0;
                self.pulse1.sweep_shift = value & 0x07;
                self.pulse1.sweep_reload = true;
            }
            0x4002 => {
                self.pulse1.timer_period = (self.pulse1.timer_period & 0x700) | (value as u16);
            }
            0x4003 => {
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0xFF) | (((value as u16) & 0x07) << 8);
                if self.pulse1.enabled {
                    self.pulse1.length_counter = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.pulse1.sequencer_pos = 0;
                self.pulse1.envelope_start = true;
            }

            // Pulse 2
            0x4004 => {
                self.pulse2.duty = (value >> 6) & 0x03;
                self.pulse2.halt = value & 0x20 != 0;
                self.pulse2.constant_volume = value & 0x10 != 0;
                self.pulse2.volume = value & 0x0F;
            }
            0x4005 => {
                self.pulse2.sweep_enabled = value & 0x80 != 0;
                self.pulse2.sweep_period = (value >> 4) & 0x07;
                self.pulse2.sweep_negate = value & 0x08 != 0;
                self.pulse2.sweep_shift = value & 0x07;
                self.pulse2.sweep_reload = true;
            }
            0x4006 => {
                self.pulse2.timer_period = (self.pulse2.timer_period & 0x700) | (value as u16);
            }
            0x4007 => {
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0xFF) | (((value as u16) & 0x07) << 8);
                if self.pulse2.enabled {
                    self.pulse2.length_counter = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.pulse2.sequencer_pos = 0;
                self.pulse2.envelope_start = true;
            }

            // Triangle
            0x4008 => {
                self.triangle.control = value & 0x80 != 0;
                self.triangle.linear_reload = value & 0x7F;
            }
            0x400A => {
                self.triangle.timer_period = (self.triangle.timer_period & 0x700) | (value as u16);
            }
            0x400B => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0xFF) | (((value as u16) & 0x07) << 8);
                if self.triangle.enabled {
                    self.triangle.length_counter = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.triangle.linear_reload_flag = true;
            }

            // Noise
            0x400C => {
                self.noise.halt = value & 0x20 != 0;
                self.noise.constant_volume = value & 0x10 != 0;
                self.noise.volume = value & 0x0F;
            }
            0x400E => {
                self.noise.mode = value & 0x80 != 0;
                self.noise.period = value & 0x0F;
            }
            0x400F => {
                if self.noise.enabled {
                    self.noise.length_counter = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.noise.envelope_start = true;
            }

            // DMC
            0x4010 => {
                self.dmc.irq_enabled = value & 0x80 != 0;
                self.dmc.loop_flag = value & 0x40 != 0;
                self.dmc.rate = value & 0x0F;
                if !self.dmc.irq_enabled {
                    self.dmc.irq_pending = false;
                }
            }
            0x4011 => {
                self.dmc.output_level = value & 0x7F;
            }
            0x4012 => {
                self.dmc.sample_address = 0xC000 | ((value as u16) << 6);
            }
            0x4013 => {
                self.dmc.sample_length = ((value as u16) << 4) | 1;
            }

            // Status
            0x4015 => {
                self.pulse1.enabled = value & 0x01 != 0;
                self.pulse2.enabled = value & 0x02 != 0;
                self.triangle.enabled = value & 0x04 != 0;
                self.noise.enabled = value & 0x08 != 0;

                if !self.pulse1.enabled {
                    self.pulse1.length_counter = 0;
                }
                if !self.pulse2.enabled {
                    self.pulse2.length_counter = 0;
                }
                if !self.triangle.enabled {
                    self.triangle.length_counter = 0;
                }
                if !self.noise.enabled {
                    self.noise.length_counter = 0;
                }

                // DMC
                self.dmc.irq_pending = false;
                if value & 0x10 != 0 {
                    if self.dmc.bytes_remaining == 0 {
                        self.dmc.restart();
                    }
                    self.dmc.enabled = true;
                } else {
                    self.dmc.bytes_remaining = 0;
                    self.dmc.enabled = false;
                }
            }

            // Frame counter
            0x4017 => {
                self.frame_mode = value & 0x80 != 0;
                self.frame_irq_inhibit = value & 0x40 != 0;
                if self.frame_irq_inhibit {
                    self.frame_irq = false;
                }
                // Reset frame counter
                self.frame_counter = 0;
                // Immediately clock if in 5-step mode
                if self.frame_mode {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                }
            }

            _ => {}
        }
    }

    /// Read APU status register.
    pub fn read_status(&mut self) -> u8 {
        let mut status = 0;

        if self.pulse1.length_counter > 0 {
            status |= 0x01;
        }
        if self.pulse2.length_counter > 0 {
            status |= 0x02;
        }
        if self.triangle.length_counter > 0 {
            status |= 0x04;
        }
        if self.noise.length_counter > 0 {
            status |= 0x08;
        }
        if self.dmc.bytes_remaining > 0 {
            status |= 0x10;
        }
        if self.frame_irq {
            status |= 0x40;
        }
        if self.dmc.irq_pending {
            status |= 0x80;
        }

        self.frame_irq = false;
        status
    }

    /// Tick the APU for one CPU cycle.
    /// Returns true if an IRQ should be generated.
    pub fn tick(&mut self) -> bool {
        self.cycle += 1;

        // Triangle clocks every CPU cycle
        self.triangle.clock_timer();

        // DMC clocks every CPU cycle
        if self.dmc.clock_timer() && self.dmc.sample_buffer_empty && self.dmc.bytes_remaining > 0 {
            self.dmc_read_pending = Some(self.dmc.current_address);
        }

        // Other channels clock every APU cycle (every 2 CPU cycles)
        if self.cycle % 2 == 0 {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }

        // Frame counter
        self.frame_counter += 1;
        let irq = self.clock_frame_counter();

        irq || self.dmc.irq_pending
    }

    /// Clock the frame counter and return true if IRQ should fire.
    fn clock_frame_counter(&mut self) -> bool {
        let mut irq = false;

        if self.frame_mode {
            // 5-step mode (no IRQ)
            match self.frame_counter {
                3729 => self.clock_quarter_frame(),
                7457 => {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                }
                11186 => self.clock_quarter_frame(),
                18641 => {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                    self.frame_counter = 0;
                }
                _ => {}
            }
        } else {
            // 4-step mode
            match self.frame_counter {
                3729 => self.clock_quarter_frame(),
                7457 => {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                }
                11186 => self.clock_quarter_frame(),
                14915 => {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                    if !self.frame_irq_inhibit {
                        self.frame_irq = true;
                        irq = true;
                    }
                    self.frame_counter = 0;
                }
                _ => {}
            }
        }

        irq
    }

    /// Clock envelope and triangle linear counter.
    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_envelope();
        self.pulse2.clock_envelope();
        self.triangle.clock_linear();
        self.noise.clock_envelope();
    }

    /// Clock length counters and sweep units.
    fn clock_half_frame(&mut self) {
        self.pulse1.clock_length();
        self.pulse1.clock_sweep();
        self.pulse2.clock_length();
        self.pulse2.clock_sweep();
        self.triangle.clock_length();
        self.noise.clock_length();
    }

    /// Get pending DMC read address (if any).
    pub fn take_dmc_read(&mut self) -> Option<u16> {
        self.dmc_read_pending.take()
    }

    /// Provide DMC sample data from memory read.
    pub fn dmc_sample(&mut self, value: u8) {
        self.dmc.load_sample(value);
    }

    /// Get current audio output as a sample (-1.0 to 1.0).
    pub fn output(&self) -> f32 {
        // Get channel outputs
        let pulse1 = self.pulse1.output() as f32;
        let pulse2 = self.pulse2.output() as f32;
        let triangle = self.triangle.output() as f32;
        let noise = self.noise.output() as f32;
        let dmc = self.dmc.output() as f32;

        // Linear approximation mixing (faster than lookup tables)
        let pulse_out = 0.00752 * (pulse1 + pulse2);
        let tnd_out = 0.00851 * triangle + 0.00494 * noise + 0.00335 * dmc;

        pulse_out + tnd_out
    }

    /// Check if any IRQ is pending.
    pub fn irq_pending(&self) -> bool {
        self.frame_irq || self.dmc.irq_pending
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_apu() {
        let apu = Apu::new();
        assert_eq!(apu.noise.shift_register, 1);
    }

    #[test]
    fn test_status_register() {
        let mut apu = Apu::new();

        // Enable pulse 1
        apu.write(0x4015, 0x01);
        assert!(apu.pulse1.enabled);

        // Write to length counter
        apu.write(0x4003, 0x08); // Length index 1 = 254
        assert!(apu.read_status() & 0x01 != 0);
    }

    #[test]
    fn test_pulse_output() {
        let mut apu = Apu::new();

        // Enable pulse 1 with max volume
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0x3F); // Duty 0, constant volume, volume 15
        apu.write(0x4002, 0x00); // Timer low
        apu.write(0x4003, 0x08); // Timer high, length counter

        // Should produce output
        assert!(apu.pulse1.enabled);
        assert_eq!(apu.pulse1.length_counter, 254);
    }

    #[test]
    fn test_triangle_output() {
        let mut apu = Apu::new();

        // Enable triangle
        apu.write(0x4015, 0x04);
        apu.write(0x4008, 0xFF); // Linear counter
        apu.write(0x400A, 0x00); // Timer low
        apu.write(0x400B, 0x08); // Timer high, length counter

        assert!(apu.triangle.enabled);
        assert_eq!(apu.triangle.length_counter, 254);
    }

    #[test]
    fn test_noise_lfsr() {
        let mut noise = NoiseChannel::new();
        noise.enabled = true;
        noise.length_counter = 10;
        noise.timer = 0;
        noise.period = 0;

        let initial = noise.shift_register;
        noise.clock_timer();
        assert_ne!(noise.shift_register, initial);
    }
}
