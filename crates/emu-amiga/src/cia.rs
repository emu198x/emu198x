//! CIA 8520 Complex Interface Adapter.
//!
//! The Amiga has two 8520 CIAs, structurally identical to the C64's 6526
//! but with a different TOD counter (8520 counts up, 6526 counts BCD).
//! For Phase 1 boot, TOD is stubbed.
//!
//! - CIA-A ($BFE001, odd bytes): OVL control, keyboard, parallel port
//!   - PRA bit 0: OVL (overlay control)
//!   - IRQ → INTREQ bit 3 (PORTS, IPL level 2)
//! - CIA-B ($BFD000, even bytes): serial port, disk control
//!   - IRQ → INTREQ bit 13 (EXTER, IPL level 6)
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

    /// Serial data register (stub for keyboard).
    sdr: u8,
    /// TOD 1/10s counter (simple stub for boot timing checks).
    tod_10ths: u8,
}

impl Cia {
    #[must_use]
    pub fn new() -> Self {
        Self {
            port_a: 0xFF,
            port_b: 0xFF,
            ddr_a: 0,
            ddr_b: 0,
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
            tod_10ths: 0,
        }
    }

    /// Reset CIA state to power-on defaults.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Queue a serial byte into the SDR and raise the serial ICR flag.
    pub fn queue_serial_byte(&mut self, value: u8) {
        self.sdr = value;
        // ICR bit 3 = serial port
        self.icr_status |= 0x08;
    }

    /// Tick the CIA for one E-clock cycle.
    pub fn tick(&mut self) {
        // Timer A force load
        if self.timer_a_force_load {
            self.timer_a = self.timer_a_latch;
            self.timer_a_force_load = false;
        }

        // Timer A countdown
        if self.timer_a_running {
            if self.timer_a == 0 {
                self.icr_status |= 0x01; // Timer A underflow
                self.timer_a = self.timer_a_latch;
                if self.timer_a_oneshot {
                    self.timer_a_running = false;
                    self.cra &= !0x01;
                }
            } else {
                self.timer_a -= 1;
            }
        }

        // Timer B force load
        if self.timer_b_force_load {
            self.timer_b = self.timer_b_latch;
            self.timer_b_force_load = false;
        }

        // Timer B countdown
        if self.timer_b_running {
            if self.timer_b == 0 {
                self.icr_status |= 0x02; // Timer B underflow
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

    /// Check if the CIA has an active interrupt.
    #[must_use]
    pub fn irq_active(&self) -> bool {
        (self.icr_status & self.icr_mask & 0x1F) != 0
    }

    /// Read a CIA register.
    #[must_use]
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => (self.port_a & self.ddr_a) | (!self.ddr_a),
            0x01 => (self.port_b & self.ddr_b) | (!self.ddr_b),
            0x02 => self.ddr_a,
            0x03 => self.ddr_b,
            0x04 => self.timer_a as u8,
            0x05 => (self.timer_a >> 8) as u8,
            0x06 => self.timer_b as u8,
            0x07 => (self.timer_b >> 8) as u8,
            // TOD registers: stubbed
            0x08 => {
                let val = self.tod_10ths;
                self.tod_10ths = self.tod_10ths.wrapping_add(1);
                val
            }
            0x09..=0x0B => 0,
            // Serial data register
            0x0C => self.sdr,
            // ICR read: returns status with bit 7 = any active
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
            // TOD: ignored
            0x08..=0x0B => {}
            // Serial data register
            0x0C => self.sdr = value,
            0x0D => {
                // ICR write: bit 7 = set(1) or clear(0) the mask bits
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

    /// Get port A output value.
    #[must_use]
    pub fn port_a_output(&self) -> u8 {
        (self.port_a & self.ddr_a) | (!self.ddr_a)
    }

    /// Get port B output value.
    #[must_use]
    pub fn port_b_output(&self) -> u8 {
        (self.port_b & self.ddr_b) | (!self.ddr_b)
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
        cia.write(0x0E, 0x01); // Start, continuous

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
        cia.write(0x0E, 0x09); // Start + one-shot

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
        cia.write(0x0D, 0x83); // Set bits 0,1
        assert_eq!(cia.icr_mask, 0x03);

        cia.write(0x0D, 0x01); // Clear bit 0
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
    fn port_a_overlay_bit() {
        let mut cia = Cia::new();
        cia.write(0x02, 0x03); // DDR: bits 0,1 output
        cia.write(0x00, 0x01); // OVL = 1
        assert_eq!(cia.port_a_output() & 0x01, 0x01);

        cia.write(0x00, 0x00); // OVL = 0
        assert_eq!(cia.port_a_output() & 0x01, 0x00);
    }
}
