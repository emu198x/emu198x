//! Atari 5200 cartridge handling.
//!
//! ROM mapping (no bank switching) for 4 KB, 8 KB, 16 KB, and 32 KB
//! cartridges across the `$4000-$BFFF` window.
//!
//! 4 KB, 8 KB, and 32 KB carts sit at the top of the window and mirror
//! downward to fill it. **16 KB carts are "two chip" (EE_16)** — the
//! standard 5200 16 KB layout (Pac-Man, Galaxian, Defender, Star
//! Raiders, …). Two 8 KB ROM chips are decoded by CPU A15: the lower
//! 8 KB answers `$4000-$7FFF`, the upper 8 KB answers `$8000-$BFFF`,
//! and A13/A14 are don't-care so each chip mirrors twice within its
//! 16 KB half. The cart's entry vector at `$BFFE` therefore lands in
//! the upper chip and points at the upper chip's code — a plain linear
//! `$8000-$BFFF` map (the donor's behaviour) leaves the entry pointing
//! into the lower chip's empty space and the machine executes padding.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-5200/src/cartridge.rs`
//! (port 2026-06-01); 16 KB two-chip decode added 2026-06-04.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
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
        let offset = if self.rom.len() == 16384 {
            // Two-chip 16 KB decode: A15 selects the 8 KB chip, A0-A12
            // address within it, A13/A14 mirror. $8000-$BFFF → upper 8 KB
            // (ROM $2000-$3FFF), $4000-$7FFF → lower 8 KB (ROM $0000-$1FFF).
            (addr as usize & 0x1FFF) | usize::from(addr & 0x8000 != 0) << 13
        } else {
            addr.wrapping_sub(self.base_addr) as usize % self.rom.len()
        };
        self.rom[offset]
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

    #[test]
    fn sixteen_kb_two_chip_decode() {
        // Lay a unique marker in each 8 KB chip so the decode is
        // unambiguous: lower chip = ROM $0000-$1FFF, upper = $2000-$3FFF.
        let mut rom = vec![0u8; 16384];
        rom[0x0000] = 0xA1; // lower chip, first byte
        rom[0x1FFF] = 0xA2; // lower chip, last byte
        rom[0x2000] = 0xB1; // upper chip, first byte
        rom[0x3FFF] = 0xB2; // upper chip, last byte
        rom[0x2386] = 0x78; // entry-point byte (cf. Pac-Man's $8386 = SEI)
        let cart = Cartridge::from_rom(&rom).expect("16K");

        // Lower 8 KB answers $4000-$7FFF; upper 8 KB answers $8000-$BFFF.
        assert_eq!(cart.read(0x4000), 0xA1);
        assert_eq!(cart.read(0x8000), 0xB1);
        assert_eq!(cart.read(0xBFFF), 0xB2);
        // The cart entry vector ($BFFE) and its target both live in the
        // upper chip — the bug this guards against put $8386 in the lower
        // chip's empty space and the machine executed padding.
        assert_eq!(cart.read(0x8386), 0x78);

        // A13/A14 are don't-care, so each chip mirrors twice within its
        // 16 KB half: $6000-$7FFF repeats the lower chip, $A000-$BFFF the
        // upper.
        assert_eq!(cart.read(0x6000), 0xA1);
        assert_eq!(cart.read(0xA000), 0xB1);
        assert_eq!(cart.read(0x7FFF), 0xA2);
    }
}
