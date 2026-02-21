//! Paula interrupt controller.

pub struct Paula {
    pub intena: u16,
    pub intreq: u16,
    pub adkcon: u16,
    pub dsklen: u16,
    pub dsklen_prev: u16,
    pub dsksync: u16,
}

impl Paula {
    pub fn new() -> Self {
        Self {
            intena: 0,
            intreq: 0,
            adkcon: 0,
            dsklen: 0,
            dsklen_prev: 0,
            dsksync: 0,
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

    pub fn write_adkcon(&mut self, val: u16) {
        if val & 0x8000 != 0 {
            self.adkcon |= val & 0x7FFF;
        } else {
            self.adkcon &= !(val & 0x7FFF);
        }
    }

    /// Double-write protocol: DMA starts only when DSKLEN is written
    /// twice in a row with bit 15 set. With no disk media, the DMA
    /// "completes" immediately — the buffer keeps whatever data was
    /// there (zeros) and DSKBLK fires so the waiting task unblocks.
    pub fn write_dsklen(&mut self, val: u16) {
        let prev = self.dsklen;
        self.dsklen = val;
        self.dsklen_prev = prev;

        // Detect double-write with DMA enable (bit 15 set on both writes).
        // Bit 14 clear = read direction (disk → memory).
        if val & 0x8000 != 0 && prev & 0x8000 != 0 {
            // DMA "complete" — fire DSKBLK interrupt (INTREQ bit 1).
            // The ROM's L1 interrupt handler and disk.resource will
            // signal the trackdisk task, unblocking its Wait($0400).
            self.request_interrupt(1);
        }
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

        // Amiga Hardware Reference Manual interrupt priority mapping:
        //   L6: bit 13 EXTER (CIA-B)
        //   L5: bit 12 DSKSYN, bit 11 RBF
        //   L4: bit 10 AUD3, bit 9 AUD2, bit 8 AUD1, bit 7 AUD0
        //   L3: bit 6 BLIT, bit 5 VERTB, bit 4 COPER
        //   L2: bit 3 PORTS (CIA-A)
        //   L1: bit 2 SOFT, bit 1 DSKBLK, bit 0 TBE
        if active & 0x2000 != 0 { return 6; } // EXTER
        if active & 0x1800 != 0 { return 5; } // DSKSYN, RBF
        if active & 0x0780 != 0 { return 4; } // AUD3-0
        if active & 0x0070 != 0 { return 3; } // BLIT, VERTB, COPER
        if active & 0x0008 != 0 { return 2; } // PORTS
        if active & 0x0007 != 0 { return 1; } // SOFT, DSKBLK, TBE

        0
    }
}
