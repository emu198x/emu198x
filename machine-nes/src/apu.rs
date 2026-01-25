//! NES Audio Processing Unit (APU).
//!
//! The APU generates audio through 5 channels:
//! - 2 pulse wave channels (square waves with duty cycle)
//! - 1 triangle wave channel
//! - 1 noise channel
//! - 1 DMC (delta modulation) channel for samples

/// APU register addresses.
pub mod regs {
    // Pulse 1 ($4000-$4003)
    pub const PULSE1_DUTY: u16 = 0x4000;
    pub const PULSE1_SWEEP: u16 = 0x4001;
    pub const PULSE1_TIMER_LO: u16 = 0x4002;
    pub const PULSE1_TIMER_HI: u16 = 0x4003;

    // Pulse 2 ($4004-$4007)
    pub const PULSE2_DUTY: u16 = 0x4004;
    pub const PULSE2_SWEEP: u16 = 0x4005;
    pub const PULSE2_TIMER_LO: u16 = 0x4006;
    pub const PULSE2_TIMER_HI: u16 = 0x4007;

    // Triangle ($4008-$400B)
    pub const TRI_LINEAR: u16 = 0x4008;
    pub const TRI_TIMER_LO: u16 = 0x400A;
    pub const TRI_TIMER_HI: u16 = 0x400B;

    // Noise ($400C-$400F)
    pub const NOISE_ENVELOPE: u16 = 0x400C;
    pub const NOISE_PERIOD: u16 = 0x400E;
    pub const NOISE_LENGTH: u16 = 0x400F;

    // DMC ($4010-$4013)
    pub const DMC_FREQ: u16 = 0x4010;
    pub const DMC_RAW: u16 = 0x4011;
    pub const DMC_START: u16 = 0x4012;
    pub const DMC_LEN: u16 = 0x4013;

    // Control/Status
    pub const STATUS: u16 = 0x4015;
    pub const FRAME_COUNTER: u16 = 0x4017;
}

/// Pulse channel state.
#[derive(Clone, Default)]
pub struct PulseChannel {
    /// Duty cycle (0-3).
    pub duty: u8,
    /// Length counter halt / envelope loop.
    pub halt: bool,
    /// Constant volume flag.
    pub constant_volume: bool,
    /// Volume / envelope divider.
    pub volume: u8,
    /// Sweep enabled.
    pub sweep_enabled: bool,
    /// Sweep period.
    pub sweep_period: u8,
    /// Sweep negate.
    pub sweep_negate: bool,
    /// Sweep shift.
    pub sweep_shift: u8,
    /// Timer period.
    pub timer_period: u16,
    /// Length counter.
    pub length_counter: u8,
    /// Channel enabled.
    pub enabled: bool,

    // Internal state
    timer: u16,
    sequencer_pos: u8,
    envelope_counter: u8,
    envelope_divider: u8,
    sweep_divider: u8,
    sweep_reload: bool,
}

/// Triangle channel state.
#[derive(Clone, Default)]
pub struct TriangleChannel {
    /// Linear counter reload value.
    pub linear_reload: u8,
    /// Control flag (length counter halt).
    pub control: bool,
    /// Timer period.
    pub timer_period: u16,
    /// Length counter.
    pub length_counter: u8,
    /// Channel enabled.
    pub enabled: bool,

    // Internal state
    timer: u16,
    linear_counter: u8,
    linear_reload_flag: bool,
    sequencer_pos: u8,
}

/// Noise channel state.
#[derive(Clone, Default)]
pub struct NoiseChannel {
    /// Length counter halt / envelope loop.
    pub halt: bool,
    /// Constant volume flag.
    pub constant_volume: bool,
    /// Volume / envelope divider.
    pub volume: u8,
    /// Mode flag (short/long).
    pub mode: bool,
    /// Period index.
    pub period: u8,
    /// Length counter.
    pub length_counter: u8,
    /// Channel enabled.
    pub enabled: bool,

    // Internal state
    timer: u16,
    shift_register: u16,
    envelope_counter: u8,
    envelope_divider: u8,
}

/// DMC channel state.
#[derive(Clone, Default)]
pub struct DmcChannel {
    /// IRQ enabled.
    pub irq_enabled: bool,
    /// Loop flag.
    pub loop_flag: bool,
    /// Rate index.
    pub rate: u8,
    /// Direct load value.
    pub direct_load: u8,
    /// Sample address.
    pub sample_address: u16,
    /// Sample length.
    pub sample_length: u16,
    /// Channel enabled.
    pub enabled: bool,

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
}

/// NES APU.
#[derive(Clone, Default)]
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
    /// Frame counter mode (0 = 4-step, 1 = 5-step).
    pub frame_mode: bool,
    /// Frame IRQ inhibit.
    pub frame_irq_inhibit: bool,
    /// Frame IRQ pending.
    pub frame_irq: bool,

    // Internal state
    frame_counter: u32,
    cycle: u64,
}

impl Apu {
    /// Create a new APU.
    pub fn new() -> Self {
        let mut apu = Self::default();
        apu.noise.shift_register = 1;
        apu
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
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0x700) | (value as u16);
            }
            0x4003 => {
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0xFF) | (((value as u16) & 0x07) << 8);
                if self.pulse1.enabled {
                    self.pulse1.length_counter = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.pulse1.sequencer_pos = 0;
                self.pulse1.envelope_counter = 15;
            }

            // Pulse 2 (same as Pulse 1)
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
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0x700) | (value as u16);
            }
            0x4007 => {
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0xFF) | (((value as u16) & 0x07) << 8);
                if self.pulse2.enabled {
                    self.pulse2.length_counter = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.pulse2.sequencer_pos = 0;
                self.pulse2.envelope_counter = 15;
            }

            // Triangle
            0x4008 => {
                self.triangle.control = value & 0x80 != 0;
                self.triangle.linear_reload = value & 0x7F;
            }
            0x400A => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x700) | (value as u16);
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
                self.noise.envelope_counter = 15;
            }

            // DMC
            0x4010 => {
                self.dmc.irq_enabled = value & 0x80 != 0;
                self.dmc.loop_flag = value & 0x40 != 0;
                self.dmc.rate = value & 0x0F;
            }
            0x4011 => {
                self.dmc.direct_load = value & 0x7F;
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
                self.dmc.enabled = value & 0x10 != 0;

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

                self.dmc.irq_pending = false;
            }

            // Frame counter
            0x4017 => {
                self.frame_mode = value & 0x80 != 0;
                self.frame_irq_inhibit = value & 0x40 != 0;
                if self.frame_irq_inhibit {
                    self.frame_irq = false;
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
    pub fn tick(&mut self) {
        self.cycle += 1;
        // TODO: Implement actual APU timing and sample generation
    }

    /// Get current audio output (mixed).
    pub fn output(&self) -> f32 {
        // TODO: Implement proper mixing
        0.0
    }
}

/// Length counter lookup table.
static LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

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
}
