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
}

impl Paula {
    #[must_use]
    pub fn new() -> Self {
        Self {
            intena: 0,
            intreq: 0,
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

    /// Compute the active IPL level (0-6).
    ///
    /// Returns 0 if no interrupts are pending or master enable is off.
    #[must_use]
    pub fn compute_ipl(&self) -> u8 {
        // Master enable: bit 14
        if self.intena & (1 << 14) == 0 {
            return 0;
        }

        let active = self.intena & self.intreq & 0x3FFF;
        if active == 0 {
            return 0;
        }

        // Check from highest priority down
        // IPL 6: bits 12-14 (EXTER)
        if active & 0x7000 != 0 {
            return 6;
        }
        // IPL 5: bits 10-11 (RBF, DSKSYN)
        if active & 0x0C00 != 0 {
            return 5;
        }
        // IPL 4: bits 7-9 (AUD0-3)
        if active & 0x0380 != 0 {
            return 4;
        }
        // IPL 3: bits 4-5 (COPER, VERTB)
        if active & 0x0030 != 0 {
            return 3;
        }
        // IPL 2: bit 3 (PORTS / CIA-A)
        if active & 0x0008 != 0 {
            return 2;
        }
        // IPL 1: bits 0-2 (TBE, DSKBLK, SOFT)
        if active & 0x0007 != 0 {
            return 1;
        }

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
        paula.intena = 0x3FFF; // All enabled except master
        assert_eq!(paula.compute_ipl(), 0);
    }

    #[test]
    fn ipl_zero_when_no_active() {
        let mut paula = Paula::new();
        paula.intena = 1 << 14; // Master only
        assert_eq!(paula.compute_ipl(), 0);
    }

    #[test]
    fn ipl_3_for_vertb() {
        let mut paula = Paula::new();
        paula.intena = (1 << 14) | (1 << 5); // Master + VERTB
        paula.intreq = 1 << 5; // VERTB pending
        assert_eq!(paula.compute_ipl(), 3);
    }

    #[test]
    fn ipl_6_for_exter() {
        let mut paula = Paula::new();
        paula.intena = (1 << 14) | (1 << 13); // Master + EXTER
        paula.intreq = 1 << 13;
        assert_eq!(paula.compute_ipl(), 6);
    }

    #[test]
    fn ipl_2_for_ports() {
        let mut paula = Paula::new();
        paula.intena = (1 << 14) | (1 << 3); // Master + PORTS
        paula.intreq = 1 << 3;
        assert_eq!(paula.compute_ipl(), 2);
    }

    #[test]
    fn highest_priority_wins() {
        let mut paula = Paula::new();
        paula.intena = (1 << 14) | (1 << 3) | (1 << 13); // PORTS + EXTER
        paula.intreq = (1 << 3) | (1 << 13);
        assert_eq!(paula.compute_ipl(), 6); // EXTER wins
    }

    #[test]
    fn set_clr_intena() {
        let mut paula = Paula::new();
        paula.write_intena(0xC020); // Set bit 5 (VERTB) + master (bit 14)
        assert_eq!(paula.intena & (1 << 5), 1 << 5);
        assert_eq!(paula.intena & (1 << 14), 1 << 14);
    }

    #[test]
    fn request_interrupt_bit() {
        let mut paula = Paula::new();
        paula.request_interrupt(5); // VERTB
        assert_eq!(paula.intreq & (1 << 5), 1 << 5);
    }
}
