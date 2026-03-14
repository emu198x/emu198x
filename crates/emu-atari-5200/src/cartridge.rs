//! Atari 5200 cartridge handling.
//!
//! Supports flat ROM mapping (no banking) for 4KB, 8KB, 16KB, and 32KB
//! cartridges. The ROM is placed at the top of the $4000-$BFFF address
//! range and mirrored downward to fill the entire window.

/// An Atari 5200 cartridge.
pub struct Cartridge {
    /// Full ROM data.
    rom: Vec<u8>,
    /// Base address where the ROM starts (e.g. $A000 for 8KB).
    base_addr: u16,
}

impl Cartridge {
    /// Create a cartridge from raw ROM data.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not a supported power-of-two
    /// between 4KB and 32KB.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let base_addr = match data.len() {
            4096 => 0xB000,
            8192 => 0xA000,
            16384 => 0x8000,
            32768 => 0x4000,
            other => return Err(format!("Unsupported cartridge size: {other} bytes")),
        };

        Ok(Self {
            rom: data.to_vec(),
            base_addr,
        })
    }

    /// Read a byte from the cartridge address space ($4000-$BFFF).
    ///
    /// Addresses below `base_addr` are mirrored.
    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        if self.rom.is_empty() {
            return 0xFF;
        }
        let offset = addr.wrapping_sub(self.base_addr) as usize;
        self.rom[offset % self.rom.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_4k_rom() {
        let rom = vec![0xEA; 4096];
        let cart = Cartridge::from_rom(&rom).expect("4K ROM");
        assert_eq!(cart.base_addr, 0xB000);
    }

    #[test]
    fn detect_8k_rom() {
        let rom = vec![0xEA; 8192];
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");
        assert_eq!(cart.base_addr, 0xA000);
    }

    #[test]
    fn detect_16k_rom() {
        let rom = vec![0xEA; 16384];
        let cart = Cartridge::from_rom(&rom).expect("16K ROM");
        assert_eq!(cart.base_addr, 0x8000);
    }

    #[test]
    fn detect_32k_rom() {
        let rom = vec![0xEA; 32768];
        let cart = Cartridge::from_rom(&rom).expect("32K ROM");
        assert_eq!(cart.base_addr, 0x4000);
    }

    #[test]
    fn reject_invalid_size() {
        let rom = vec![0; 5000];
        assert!(Cartridge::from_rom(&rom).is_err());
    }

    #[test]
    fn eight_kb_rom_mirrors_below_base() {
        let mut rom = vec![0; 8192];
        rom[0] = 0x42;
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");

        // Direct read at base address ($A000)
        assert_eq!(cart.read(0xA000), 0x42);
        // Mirror: $4000 wraps around (offset = $4000 - $A000 = wrapping sub)
        assert_eq!(cart.read(0x4000), 0x42);
    }

    #[test]
    fn reset_vector_location_8k() {
        let mut rom = vec![0; 8192];
        // Reset vector at $BFFC-$BFFD for 8KB ROM
        // Offset $1FFC, $1FFD within ROM
        rom[0x1FFC] = 0x00;
        rom[0x1FFD] = 0xA0;
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");

        assert_eq!(cart.read(0xBFFC), 0x00);
        assert_eq!(cart.read(0xBFFD), 0xA0);
    }
}
