//! Atari 800XL cartridge handling.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-800xl/src/cartridge.rs`
//! (RULES.md rule 27).
//!
//! Supports flat 8 KB and 16 KB cartridges:
//!
//! - 8 KB: `$A000-$BFFF` (replaces the BASIC ROM window)
//! - 16 KB: `$8000-$BFFF`

pub struct Cartridge {
    rom: Vec<u8>,
    base: u16,
}

impl Cartridge {
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

    #[must_use]
    pub fn base(&self) -> u16 {
        self.base
    }

    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        let offset = addr.wrapping_sub(self.base) as usize;
        self.rom.get(offset).copied().unwrap_or(0xFF)
    }

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
        let cart = Cartridge::from_rom(&vec![0xEA; 8192]).expect("8K");
        assert_eq!(cart.base(), 0xA000);
    }

    #[test]
    fn detect_16k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("16K");
        assert_eq!(cart.base(), 0x8000);
    }

    #[test]
    fn reject_oversize() {
        assert!(Cartridge::from_rom(&vec![0u8; 32769]).is_err());
    }

    #[test]
    fn read_within_range() {
        let mut rom = vec![0u8; 8192];
        rom[0] = 0x42;
        rom[0x1FFF] = 0x99;
        let cart = Cartridge::from_rom(&rom).expect("8K");
        assert_eq!(cart.read(0xA000), 0x42);
        assert_eq!(cart.read(0xBFFF), 0x99);
    }

    #[test]
    fn covers_reports_correctly() {
        let cart = Cartridge::from_rom(&vec![0u8; 8192]).expect("8K");
        assert!(cart.covers(0xA000));
        assert!(cart.covers(0xBFFF));
        assert!(!cart.covers(0x9FFF));
        assert!(!cart.covers(0xC000));
    }
}
