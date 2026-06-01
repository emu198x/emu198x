//! Atari 5200 cartridge handling.
//!
//! Flat ROM mapping (no banking) for 4 KB, 8 KB, 16 KB, and 32 KB
//! cartridges. The ROM is placed at the top of the `$4000-$BFFF`
//! window and mirrored downward to fill the entire region.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-5200/src/cartridge.rs`
//! (port 2026-06-01).

pub struct Cartridge {
    rom: Vec<u8>,
    base_addr: u16,
}

impl Cartridge {
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

    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        if self.rom.is_empty() {
            return 0xFF;
        }
        let offset = addr.wrapping_sub(self.base_addr) as usize;
        self.rom[offset % self.rom.len()]
    }

    #[must_use]
    pub fn base_addr(&self) -> u16 {
        self.base_addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_4k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 4096]).expect("4K");
        assert_eq!(cart.base_addr(), 0xB000);
    }

    #[test]
    fn detect_8k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 8192]).expect("8K");
        assert_eq!(cart.base_addr(), 0xA000);
    }

    #[test]
    fn detect_16k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("16K");
        assert_eq!(cart.base_addr(), 0x8000);
    }

    #[test]
    fn detect_32k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 32768]).expect("32K");
        assert_eq!(cart.base_addr(), 0x4000);
    }

    #[test]
    fn reject_invalid_size() {
        assert!(Cartridge::from_rom(&vec![0u8; 5000]).is_err());
    }

    #[test]
    fn reset_vector_at_bffc_for_8k() {
        let mut rom = vec![0u8; 8192];
        rom[0x1FFC] = 0x00;
        rom[0x1FFD] = 0xA0;
        let cart = Cartridge::from_rom(&rom).expect("8K");
        assert_eq!(cart.read(0xBFFC), 0x00);
        assert_eq!(cart.read(0xBFFD), 0xA0);
    }
}
