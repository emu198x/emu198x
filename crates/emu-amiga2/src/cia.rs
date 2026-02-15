//! CIA 8520 Complex Interface Adapter.
//!
//! The Amiga has two 8520 CIAs, structurally identical to the C64's 6526
//! but with a different TOD counter (8520 counts up, 6526 counts BCD).
//!
//! - CIA-A ($BFE001, odd bytes): OVL control, keyboard, parallel port
//!   - PRA bit 0: OVL (overlay control)
//!   - IRQ -> INTREQ bit 3 (PORTS, IPL level 2)
//! - CIA-B ($BFD000, even bytes): serial port, disk control
//!   - IRQ -> INTREQ bit 13 (EXTER, IPL level 6)
//!
//! Address decoding:
//! - CIA-A: register = (addr >> 8) & 0x0F, odd bytes only
//! - CIA-B: register = (addr >> 8) & 0x0F, even bytes only
//!
//! Tick rate: E-clock = crystal / 40 = 709,379 Hz

#![allow(clippy::cast_possible_truncation, clippy::struct_excessive_bools)]

/// CIA 8520 instance.
pub struct Cia {
    /// Port A output register.
    port_a: u8,
    /// Port B output register.
    port_b: u8,
    /// Port A data direction register (1 = output).
    ddr_a: u8,
    /// Port B data direction register (1 = output).
    ddr_b: u8,
    /// External input state for port A pins (active when DDR bit = 0).
    /// Undriven inputs default to 1; set specific bits to 0 for active-low
    /// signals like /CHNG (disk changed).
    pub external_a: u8,
    /// External input state for port B pins.
    pub external_b: u8,

    /// Timer A counter.
    timer_a: u16,
    /// Timer A latch.
    timer_a_latch: u16,
    /// Timer A running.
    timer_a_running: bool,
    /// Timer A one-shot mode.
    timer_a_oneshot: bool,
    /// Timer A force-load strobe pending.
    timer_a_force_load: bool,

    /// Timer B counter.
    timer_b: u16,
    /// Timer B latch.
    timer_b_latch: u16,
    /// Timer B running.
    timer_b_running: bool,
    /// Timer B one-shot mode.
    timer_b_oneshot: bool,
    /// Timer B force-load strobe pending.
    timer_b_force_load: bool,

    /// Interrupt control: status flags (bits 0-4).
    icr_status: u8,
    /// Interrupt control: enable mask (bits 0-4).
    icr_mask: u8,

    /// Control register A.
    cra: u8,
    /// Control register B.
    crb: u8,

    /// Serial data register.
    sdr: u8,
    /// Time-of-day counter (24-bit, binary).
    tod_counter: u32,
    /// Time-of-day alarm value (24-bit, binary).
    tod_alarm: u32,
    /// Divider to approximate the TOD input clock from E-clock.
    tod_divider: u16,
}

impl Cia {
    /// CIA E-clock ticks per TOD increment (approx. 50 Hz on PAL systems).
    const TOD_DIVISOR: u16 = 14_188;

    #[must_use]
    pub fn new() -> Self {
        Self {
            port_a: 0xFF,
            port_b: 0xFF,
            ddr_a: 0,
            ddr_b: 0,
            external_a: 0xFF,
            external_b: 0xFF,
            timer_a: 0xFFFF,
            timer_a_latch: 0xFFFF,
            timer_a_running: false,
            timer_a_oneshot: false,
            timer_a_force_load: false,
            timer_b: 0xFFFF,
            timer_b_latch: 0xFFFF,
            timer_b_running: false,
            timer_b_oneshot: false,
            timer_b_force_load: false,
            icr_status: 0,
            icr_mask: 0,
            cra: 0,
            crb: 0,
            sdr: 0,
            tod_counter: 0,
            tod_alarm: 0,
            tod_divider: 0,
        }
    }

    /// Reset CIA state to power-on defaults.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Queue a serial byte into the SDR and raise the serial ICR flag.
    pub fn queue_serial_byte(&mut self, value: u8) {
        self.sdr = value;
        self.raise_interrupt_flag(3);
    }

    /// Tick the CIA for one E-clock cycle.
    pub fn tick(&mut self) {
        let mut timer_a_underflow = false;

        if self.timer_a_force_load {
            self.timer_a = self.timer_a_latch;
            self.timer_a_force_load = false;
        }

        let timer_a_counts_eclock = self.cra & 0x20 == 0;
        if self.timer_a_running && timer_a_counts_eclock {
            if self.timer_a == 0 {
                self.raise_interrupt_flag(0);
                timer_a_underflow = true;
                self.timer_a = self.timer_a_latch;
                if self.timer_a_oneshot {
                    self.timer_a_running = false;
                    self.cra &= !0x01;
                }
            } else {
                self.timer_a -= 1;
            }
        }

        if self.timer_b_force_load {
            self.timer_b = self.timer_b_latch;
            self.timer_b_force_load = false;
        }

        // CRB bits 6-5 select Timer B clock source:
        //   00 = E-clock
        //   01 = CNT (not wired in this model)
        //   10 = Timer A underflow
        //   11 = Timer A underflow + CNT (treated as Timer A underflow here)
        if self.timer_b_running {
            let timer_b_source = (self.crb >> 5) & 0x03;
            let timer_b_should_count = match timer_b_source {
                0x00 => true,
                0x02 | 0x03 => timer_a_underflow,
                _ => false,
            };

            if timer_b_should_count {
                if self.timer_b == 0 {
                    self.raise_interrupt_flag(1);
                    self.timer_b = self.timer_b_latch;
                    if self.timer_b_oneshot {
                        self.timer_b_running = false;
                        self.crb &= !0x01;
                    }
                } else {
                    self.timer_b -= 1;
                }
            }
        }

        self.tick_tod();
    }

    /// Check if the CIA has an active interrupt.
    #[must_use]
    pub fn irq_active(&self) -> bool {
        (self.icr_status & self.icr_mask & 0x1F) != 0
    }

    /// Read a CIA register.
    #[must_use]
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => (self.port_a & self.ddr_a) | (self.external_a & !self.ddr_a),
            0x01 => (self.port_b & self.ddr_b) | (self.external_b & !self.ddr_b),
            0x02 => self.ddr_a,
            0x03 => self.ddr_b,
            0x04 => self.timer_a as u8,
            0x05 => (self.timer_a >> 8) as u8,
            0x06 => self.timer_b as u8,
            0x07 => (self.timer_b >> 8) as u8,
            0x08 => self.tod_counter as u8,
            0x09 => (self.tod_counter >> 8) as u8,
            0x0A => (self.tod_counter >> 16) as u8,
            0x0B => 0,
            0x0C => self.sdr,
            0x0D => {
                let any = if (self.icr_status & self.icr_mask & 0x1F) != 0 {
                    0x80
                } else {
                    0x00
                };
                self.icr_status | any
            }
            0x0E => self.cra,
            0x0F => self.crb,
            _ => 0xFF,
        }
    }

    /// Read ICR and clear status (side-effectful read).
    pub fn read_icr_and_clear(&mut self) -> u8 {
        let any = if (self.icr_status & self.icr_mask & 0x1F) != 0 {
            0x80
        } else {
            0x00
        };
        let result = self.icr_status | any;
        self.icr_status = 0;
        result
    }

    /// Write a CIA register.
    #[allow(clippy::match_same_arms)]
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => self.port_a = value,
            0x01 => self.port_b = value,
            0x02 => self.ddr_a = value,
            0x03 => self.ddr_b = value,
            0x04 => {
                self.timer_a_latch = (self.timer_a_latch & 0xFF00) | u16::from(value);
            }
            0x05 => {
                self.timer_a_latch = (self.timer_a_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_a_running {
                    self.timer_a = self.timer_a_latch;
                }
            }
            0x06 => {
                self.timer_b_latch = (self.timer_b_latch & 0xFF00) | u16::from(value);
            }
            0x07 => {
                self.timer_b_latch = (self.timer_b_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_b_running {
                    self.timer_b = self.timer_b_latch;
                }
            }
            0x08 => self.write_tod_register(0, value),
            0x09 => self.write_tod_register(1, value),
            0x0A => self.write_tod_register(2, value),
            0x0B => {}
            0x0C => self.sdr = value,
            0x0D => {
                if value & 0x80 != 0 {
                    self.icr_mask |= value & 0x1F;
                } else {
                    self.icr_mask &= !(value & 0x1F);
                }
            }
            0x0E => {
                self.cra = value;
                self.timer_a_running = value & 0x01 != 0;
                self.timer_a_oneshot = value & 0x08 != 0;
                if value & 0x10 != 0 {
                    self.timer_a_force_load = true;
                }
            }
            0x0F => {
                self.crb = value;
                self.timer_b_running = value & 0x01 != 0;
                self.timer_b_oneshot = value & 0x08 != 0;
                if value & 0x10 != 0 {
                    self.timer_b_force_load = true;
                }
            }
            _ => {}
        }
    }

    fn raise_interrupt_flag(&mut self, bit: u8) {
        self.icr_status |= 1 << bit;
    }

    fn tick_tod(&mut self) {
        self.tod_divider = self.tod_divider.wrapping_add(1);
        if self.tod_divider < Self::TOD_DIVISOR {
            return;
        }

        self.tod_divider = 0;
        // 8520 TOD counts up (unlike 6526 BCD behavior on C64-era CIAs).
        self.tod_counter = self.tod_counter.wrapping_add(1) & 0x00FF_FFFF;
        if self.tod_counter == self.tod_alarm {
            self.raise_interrupt_flag(2);
        }
    }

    fn write_tod_register(&mut self, byte_index: u8, value: u8) {
        let shift = u32::from(byte_index) * 8;
        let mask = !(0xFFu32 << shift);

        // CRB bit 7 selects write target:
        //   0 = TOD counter
        //   1 = TOD alarm
        if self.crb & 0x80 != 0 {
            self.tod_alarm = (self.tod_alarm & mask) | (u32::from(value) << shift);
            self.tod_alarm &= 0x00FF_FFFF;
        } else {
            self.tod_counter = (self.tod_counter & mask) | (u32::from(value) << shift);
            self.tod_counter &= 0x00FF_FFFF;
        }
    }

    /// Get port A output value.
    #[must_use]
    pub fn port_a_output(&self) -> u8 {
        (self.port_a & self.ddr_a) | (self.external_a & !self.ddr_a)
    }

    /// Get port B output value.
    #[must_use]
    pub fn port_b_output(&self) -> u8 {
        (self.port_b & self.ddr_b) | (self.external_b & !self.ddr_b)
    }

    /// Debug: Timer A counter value.
    #[must_use]
    pub fn timer_a(&self) -> u16 {
        self.timer_a
    }

    /// Debug: Timer B counter value.
    #[must_use]
    pub fn timer_b(&self) -> u16 {
        self.timer_b
    }

    /// Debug: ICR status flags.
    #[must_use]
    pub fn icr_status(&self) -> u8 {
        self.icr_status
    }

    /// Debug: ICR mask bits.
    #[must_use]
    pub fn icr_mask(&self) -> u8 {
        self.icr_mask
    }

    /// Debug: CIA control register A.
    #[must_use]
    pub fn cra(&self) -> u8 {
        self.cra
    }

    /// Debug: CIA control register B.
    #[must_use]
    pub fn crb(&self) -> u8 {
        self.crb
    }

    /// Debug: TOD counter value (24-bit).
    #[must_use]
    pub fn tod_counter(&self) -> u32 {
        self.tod_counter
    }

    /// Debug: TOD alarm value (24-bit).
    #[must_use]
    pub fn tod_alarm(&self) -> u32 {
        self.tod_alarm
    }
}

impl Default for Cia {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_a_countdown() {
        let mut cia = Cia::new();
        cia.write(0x04, 10);
        cia.write(0x05, 0);
        cia.write(0x0E, 0x01);
        for _ in 0..11 {
            cia.tick();
        }
        assert!(cia.icr_status & 0x01 != 0);
    }

    #[test]
    fn timer_a_oneshot() {
        let mut cia = Cia::new();
        cia.write(0x04, 5);
        cia.write(0x05, 0);
        cia.write(0x0E, 0x09);
        for _ in 0..6 {
            cia.tick();
        }
        assert!(cia.icr_status & 0x01 != 0);
        assert!(!cia.timer_a_running);
    }

    #[test]
    fn icr_read_clears_status() {
        let mut cia = Cia::new();
        cia.icr_status = 0x01;
        cia.icr_mask = 0x01;
        let val = cia.read_icr_and_clear();
        assert_eq!(val, 0x81);
        assert_eq!(cia.icr_status, 0);
    }

    #[test]
    fn icr_mask_set_clear() {
        let mut cia = Cia::new();
        cia.write(0x0D, 0x83);
        assert_eq!(cia.icr_mask, 0x03);
        cia.write(0x0D, 0x01);
        assert_eq!(cia.icr_mask, 0x02);
    }

    #[test]
    fn irq_active_when_status_and_mask() {
        let mut cia = Cia::new();
        cia.icr_status = 0x01;
        cia.icr_mask = 0x00;
        assert!(!cia.irq_active());
        cia.icr_mask = 0x01;
        assert!(cia.irq_active());
    }

    #[test]
    fn tod_alarm_raises_interrupt() {
        let mut cia = Cia::new();
        cia.write(0x0F, 0x80); // CRB bit 7: alarm select
        cia.write(0x08, 0x01);
        cia.write(0x09, 0x00);
        cia.write(0x0A, 0x00);

        cia.write(0x0F, 0x00); // CRB bit 7: TOD select
        cia.write(0x08, 0x02);
        cia.write(0x09, 0x00);
        cia.write(0x0A, 0x00);

        for _ in 0..Cia::TOD_DIVISOR {
            cia.tick();
        }

        assert_ne!(cia.icr_status() & 0x04, 0);
    }

    #[test]
    fn timer_b_can_count_timer_a_underflows() {
        let mut cia = Cia::new();

        // Timer A underflows every 2 E-clocks.
        cia.write(0x04, 0x01);
        cia.write(0x05, 0x00);
        cia.write(0x0E, 0x01);

        // Timer B source = Timer A underflow (CRB bits 6-5 = 10), start.
        cia.write(0x06, 0x01);
        cia.write(0x07, 0x00);
        cia.write(0x0F, 0x41);

        for _ in 0..6 {
            cia.tick();
        }

        assert_ne!(cia.icr_status() & 0x02, 0);
    }
}
