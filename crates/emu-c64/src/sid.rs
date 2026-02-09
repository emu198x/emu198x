//! SID 6581/8580 stub.
//!
//! Accepts register writes, returns 0 for reads. No audio output in v1.

/// SID chip stub.
///
/// The real SID has 29 registers controlling 3 oscillators, filters, and
/// an ADSR envelope per voice. For v1, we just store register writes and
/// produce no audio.
pub struct Sid {
    regs: [u8; 29],
}

impl Sid {
    #[must_use]
    pub fn new() -> Self {
        Self { regs: [0; 29] }
    }

    /// Read a SID register. Most SID registers are write-only;
    /// readable registers ($19-$1C) return 0 in this stub.
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        let reg = (addr & 0x1F) as usize;
        if reg < self.regs.len() {
            // In real hardware, only $19-$1C are readable (paddle/osc/env).
            // Return 0 for the stub.
            0
        } else {
            0
        }
    }

    /// Write a SID register.
    pub fn write(&mut self, addr: u8, value: u8) {
        let reg = (addr & 0x1F) as usize;
        if reg < self.regs.len() {
            self.regs[reg] = value;
        }
    }
}

impl Default for Sid {
    fn default() -> Self {
        Self::new()
    }
}
