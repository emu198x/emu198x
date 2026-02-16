//! Paula interrupt controller.

pub struct Paula {
    pub intena: u16,
    pub intreq: u16,
}

impl Paula {
    pub fn new() -> Self {
        Self {
            intena: 0,
            intreq: 0,
        }
    }

    pub fn write_intena(&mut self, val: u16) {
        if val & 0x8000 != 0 {
            self.intena |= val & 0x7FFF;
        } else {
            self.intena &= !(val & 0x7FFF);
        }
    }

    pub fn write_intreq(&mut self, val: u16) {
        if val & 0x8000 != 0 {
            self.intreq |= val & 0x7FFF;
        } else {
            self.intreq &= !(val & 0x7FFF);
        }
    }

    pub fn request_interrupt(&mut self, bit: u8) {
        self.intreq |= 1 << bit;
    }

    pub fn compute_ipl(&self) -> u8 {
        // Master enable: bit 14
        if self.intena & 0x4000 == 0 {
            return 0;
        }

        let active = self.intena & self.intreq & 0x3FFF;
        if active == 0 {
            return 0;
        }

        // Check from highest priority down
        if active & 0x7000 != 0 { return 6; } // EXTER (CIA-B)
        if active & 0x0C00 != 0 { return 5; } // RBF, DSKSYN
        if active & 0x0380 != 0 { return 4; } // AUD0-3
        if active & 0x0030 != 0 { return 3; } // COPER, VERTB
        if active & 0x0008 != 0 { return 2; } // PORTS (CIA-A)
        if active & 0x0007 != 0 { return 1; } // TBE, DSKBLK, SOFT

        0
    }
}
