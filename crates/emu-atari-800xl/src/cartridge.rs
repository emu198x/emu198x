//! Atari 800XL cartridge handling.
//!
//! Supports flat ROM mapping (no banking) for 8KB and 16KB cartridges:
//! - 8KB: $A000-$BFFF (replaces BASIC ROM area)
//! - 16KB: $8000-$BFFF

/// An Atari 800XL cartridge.
pub struct Cartridge {
    /// Full ROM data.
    rom: Vec<u8>,
    /// Base address where the ROM starts.
    base: u16,
}

impl Cartridge {
    /// Create a cartridge from raw ROM data.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not 8KB or 16KB.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let base = match data.len() {
            1..=8192 => 0xA000,
            8193..=16384 => 0x8000,
            other => return Err(format!("Unsupported cartridge size: {other} bytes")),
        };

        Ok(Self {
            rom: data.to_vec(),
            base,
        })
    }

    /// Base address of this cartridge in the memory map.
    #[must_use]
    pub fn base(&self) -> u16 {
        self.base
    }

    /// Read a byte from the cartridge address space.
    ///
    /// Returns `0xFF` if the address falls outside the ROM data.
    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        let offset = addr.wrapping_sub(self.base) as usize;
        if offset < self.rom.len() {
            self.rom[offset]
        } else {
            0xFF
        }
    }

    /// Whether this cartridge covers the given address.
    #[must_use]
    pub fn covers(&self, addr: u16) -> bool {
        addr >= self.base && (addr as usize - self.base as usize) < self.rom.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_8k_rom() {
        let rom = vec![0xEA; 8192];
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");
        assert_eq!(cart.base(), 0xA000);
    }

    #[test]
    fn detect_16k_rom() {
        let rom = vec![0xEA; 16384];
        let cart = Cartridge::from_rom(&rom).expect("16K ROM");
        assert_eq!(cart.base(), 0x8000);
    }

    #[test]
    fn reject_oversize() {
        let rom = vec![0; 32769];
        assert!(Cartridge::from_rom(&rom).is_err());
    }

    #[test]
    fn read_within_range() {
        let mut rom = vec![0; 8192];
        rom[0] = 0x42;
        rom[0x1FFF] = 0x99;
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");

        assert_eq!(cart.read(0xA000), 0x42);
        assert_eq!(cart.read(0xBFFF), 0x99);
    }

    #[test]
    fn read_outside_range_returns_ff() {
        let rom = vec![0; 8192];
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");
        // Below cart range
        assert_eq!(cart.read(0x8000), 0xFF);
    }

    #[test]
    fn covers_reports_correctly() {
        let rom = vec![0; 8192];
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");
        assert!(cart.covers(0xA000));
        assert!(cart.covers(0xBFFF));
        assert!(!cart.covers(0x9FFF));
        assert!(!cart.covers(0xC000));
    }
}
