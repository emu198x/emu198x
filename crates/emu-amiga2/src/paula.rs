//! Paula interrupt controller.
//!
//! Paula manages interrupt priority for the Amiga. INTENA controls which
//! interrupts are enabled, INTREQ tracks pending requests. The highest
//! active level drives the 68000's IPL lines.
//!
//! | IPL | INTREQ bits | Sources              |
//! |-----|-------------|----------------------|
//! | 6   | 12-14       | EXTER (CIA-B)        |
//! | 5   | 10-11       | RBF, DSKSYN          |
//! | 4   | 7-9         | AUD0-3               |
//! | 3   | 4-5         | COPER, VERTB         |
//! | 2   | 3           | PORTS (CIA-A)        |
//! | 1   | 0-2         | TBE, DSKBLK, SOFT    |
//!
//! Master enable: INTENA bit 14.

#![allow(clippy::cast_possible_truncation)]

use crate::custom_regs;

/// Paula interrupt controller state.
pub struct Paula {
    /// Interrupt enable register (bits 0-14, bit 14 = master enable).
    pub intena: u16,
    /// Interrupt request register (bits 0-14).
    pub intreq: u16,
    /// Serial receive buffer (SERDATR).
    serial_rx: Option<u8>,
    /// Remaining reads before the serial buffer is cleared.
    serial_rx_reads_left: u8,
    /// Serial overrun flag (SERDATR bit 14).
    serial_overrun: bool,
}

impl Paula {
    #[must_use]
    pub fn new() -> Self {
        Self {
            intena: 0,
            intreq: 0,
            serial_rx: None,
            serial_overrun: false,
            serial_rx_reads_left: 0,
        }
    }

    /// Write to INTENA using SET/CLR logic.
    pub fn write_intena(&mut self, val: u16) {
        custom_regs::set_clr_write(&mut self.intena, val);
    }

    /// Write to INTREQ using SET/CLR logic.
    pub fn write_intreq(&mut self, val: u16) {
        custom_regs::set_clr_write(&mut self.intreq, val);
    }

    /// Set a specific interrupt request bit.
    pub fn request_interrupt(&mut self, bit: u16) {
        self.intreq |= 1 << bit;
    }

    /// Queue a serial byte into the receive buffer.
    ///
    /// Sets the RBF interrupt (bit 11) if the buffer becomes full.
    pub fn queue_serial_rx(&mut self, value: u8) {
        if self.serial_rx.is_some() {
            self.serial_overrun = true;
            return;
        }
        self.serial_rx = Some(value);
        self.serial_rx_reads_left = 2;
        self.intreq |= 1 << 11;
    }

    /// Read SERDATR (serial data + status).
    ///
    /// Bit 14: RBF (receive buffer full)
    /// Bit 15: OVR (overrun)
    /// Bit 13: TBE (transmit buffer empty)
    pub fn read_serdatr(&mut self) -> u16 {
        let mut word = 0x2000; // TBE always set
        if let Some(value) = self.serial_rx {
            word |= 0x4000; // RBF
            if self.serial_overrun {
                word |= 0x8000; // OVR
            }
            word |= u16::from(value);
            if self.serial_rx_reads_left > 0 {
                self.serial_rx_reads_left -= 1;
            }
            if self.serial_rx_reads_left == 0 {
                self.serial_rx = None;
                self.intreq &= !(1 << 11);
                self.serial_overrun = false;
            }
        } else {
            word |= 0x007F; // Idle line
        }
        word
    }

    /// Is the serial receive buffer empty?
    #[must_use]
    pub fn serial_rx_empty(&self) -> bool {
        self.serial_rx.is_none()
    }

    /// Compute the active IPL level (0-6).
    ///
    /// Returns 0 if no interrupts are pending or master enable is off.
    #[must_use]
    pub fn compute_ipl(&self) -> u8 {
        if self.intena & (1 << 14) == 0 {
            return 0;
        }

        let active = self.intena & self.intreq & 0x3FFF;
        if active == 0 {
            return 0;
        }

        if active & 0x7000 != 0 { return 6; }
        if active & 0x0C00 != 0 { return 5; }
        if active & 0x0380 != 0 { return 4; }
        if active & 0x0030 != 0 { return 3; }
        if active & 0x0008 != 0 { return 2; }
        if active & 0x0007 != 0 { return 1; }

        0
    }
}

impl Default for Paula {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipl_zero_when_master_disabled() {
        let mut paula = Paula::new();
        paula.intreq = 0x3FFF;
        paula.intena = 0x3FFF;
        assert_eq!(paula.compute_ipl(), 0);
    }

    #[test]
    fn ipl_zero_when_no_active() {
        let mut paula = Paula::new();
        paula.intena = 1 << 14;
        assert_eq!(paula.compute_ipl(), 0);
    }

    #[test]
    fn ipl_3_for_vertb() {
        let mut paula = Paula::new();
        paula.intena = (1 << 14) | (1 << 5);
        paula.intreq = 1 << 5;
        assert_eq!(paula.compute_ipl(), 3);
    }

    #[test]
    fn ipl_6_for_exter() {
        let mut paula = Paula::new();
        paula.intena = (1 << 14) | (1 << 13);
        paula.intreq = 1 << 13;
        assert_eq!(paula.compute_ipl(), 6);
    }

    #[test]
    fn highest_priority_wins() {
        let mut paula = Paula::new();
        paula.intena = (1 << 14) | (1 << 3) | (1 << 13);
        paula.intreq = (1 << 3) | (1 << 13);
        assert_eq!(paula.compute_ipl(), 6);
    }

    #[test]
    fn set_clr_intena() {
        let mut paula = Paula::new();
        paula.write_intena(0xC020);
        assert_eq!(paula.intena & (1 << 5), 1 << 5);
        assert_eq!(paula.intena & (1 << 14), 1 << 14);
    }

    #[test]
    fn request_interrupt_bit() {
        let mut paula = Paula::new();
        paula.request_interrupt(5);
        assert_eq!(paula.intreq & (1 << 5), 1 << 5);
    }
}
