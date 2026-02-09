//! NES APU stub.
//!
//! Accepts register writes to $4000-$4013, $4015, $4017.
//! Returns appropriate values for $4015 (status).
//! No audio output in v1.

/// APU stub — stores register values, produces no audio.
pub struct Apu {
    regs: [u8; 24],
    status: u8,
}

impl Apu {
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: [0; 24],
            status: 0,
        }
    }

    /// Read an APU register.
    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x4015 => self.status,
            _ => 0,
        }
    }

    /// Write an APU register.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x4000..=0x4013 => {
                self.regs[(addr - 0x4000) as usize] = value;
            }
            0x4015 => self.status = value,
            0x4017 => {
                // Frame counter — stubbed
                self.regs[23] = value;
            }
            _ => {}
        }
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}
