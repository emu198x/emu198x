//! Atari 2600 cartridge handling.
//!
//! Supports 2KB and 4KB (no banking) ROMs, plus F8 (8KB / 2 banks),
//! F6 (16KB / 4 banks), F4 (32KB / 8 banks) bank-switching via
//! hotspot detection. Reads or writes to specific addresses in the
//! `$1000-$1FFF` range trigger bank switches.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-2600/src/cartridge.rs`
//! (2026-06-01).

/// Cartridge banking scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankingScheme {
    /// 2KB or 4KB, no banking.
    None,
    /// F8: 8KB, 2 banks. Hotspots `$1FF8`/`$1FF9`.
    F8,
    /// F6: 16KB, 4 banks. Hotspots `$1FF6-$1FF9`.
    F6,
    /// F4: 32KB, 8 banks. Hotspots `$1FF4-$1FFB`.
    F4,
}

pub struct Cartridge {
    rom: Vec<u8>,
    scheme: BankingScheme,
    bank: usize,
    bank_size: usize,
}

impl Cartridge {
    /// Parse a ROM and detect the banking scheme from its size.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let (scheme, bank_size) = match data.len() {
            0..=2048 => (BankingScheme::None, data.len()),
            2049..=4096 => (BankingScheme::None, data.len()),
            8192 => (BankingScheme::F8, 4096),
            16384 => (BankingScheme::F6, 4096),
            32768 => (BankingScheme::F4, 4096),
            other => return Err(format!("Unsupported ROM size: {other} bytes")),
        };
        let num_banks = data.len().checked_div(bank_size).unwrap_or(1);
        let bank = num_banks.saturating_sub(1);
        Ok(Self {
            rom: data.to_vec(),
            scheme,
            bank,
            bank_size,
        })
    }

    /// Read a byte from the cart at `$1000-$1FFF` (also fires hotspot
    /// detection for bank switching).
    pub fn read(&mut self, addr: u16) -> u8 {
        self.check_hotspot(addr);
        let offset = (addr & 0x0FFF) as usize;
        if self.bank_size <= 2048 {
            self.rom[offset % self.rom.len()]
        } else {
            let idx = self.bank * self.bank_size + offset;
            self.rom.get(idx).copied().unwrap_or(0)
        }
    }

    /// Write to cart space — used purely for hotspot detection.
    pub fn write(&mut self, addr: u16, _value: u8) {
        self.check_hotspot(addr);
    }

    /// Current bank.
    #[must_use]
    pub fn bank(&self) -> usize {
        self.bank
    }

    /// Banking scheme.
    #[must_use]
    pub fn scheme(&self) -> BankingScheme {
        self.scheme
    }

    fn check_hotspot(&mut self, addr: u16) {
        match self.scheme {
            BankingScheme::None => {}
            BankingScheme::F8 => match addr {
                0x1FF8 => self.bank = 0,
                0x1FF9 => self.bank = 1,
                _ => {}
            },
            BankingScheme::F6 => match addr {
                0x1FF6 => self.bank = 0,
                0x1FF7 => self.bank = 1,
                0x1FF8 => self.bank = 2,
                0x1FF9 => self.bank = 3,
                _ => {}
            },
            BankingScheme::F4 => match addr {
                0x1FF4 => self.bank = 0,
                0x1FF5 => self.bank = 1,
                0x1FF6 => self.bank = 2,
                0x1FF7 => self.bank = 3,
                0x1FF8 => self.bank = 4,
                0x1FF9 => self.bank = 5,
                0x1FFA => self.bank = 6,
                0x1FFB => self.bank = 7,
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_2k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 2048]).expect("2K");
        assert_eq!(cart.scheme(), BankingScheme::None);
    }

    #[test]
    fn detect_4k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 4096]).expect("4K");
        assert_eq!(cart.scheme(), BankingScheme::None);
    }

    #[test]
    fn detect_f8_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 8192]).expect("F8");
        assert_eq!(cart.scheme(), BankingScheme::F8);
        assert_eq!(cart.bank(), 1);
    }

    #[test]
    fn detect_f6_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("F6");
        assert_eq!(cart.scheme(), BankingScheme::F6);
        assert_eq!(cart.bank(), 3);
    }

    #[test]
    fn detect_f4_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 32768]).expect("F4");
        assert_eq!(cart.scheme(), BankingScheme::F4);
        assert_eq!(cart.bank(), 7);
    }

    #[test]
    fn reject_invalid_size() {
        assert!(Cartridge::from_rom(&vec![0u8; 5000]).is_err());
    }

    #[test]
    fn f8_bank_switching() {
        let mut rom = vec![0u8; 8192];
        rom[..4096].fill(0xAA);
        rom[4096..].fill(0xBB);
        let mut cart = Cartridge::from_rom(&rom).expect("F8");
        assert_eq!(cart.read(0x1000), 0xBB);
        cart.read(0x1FF8);
        assert_eq!(cart.read(0x1000), 0xAA);
        cart.read(0x1FF9);
        assert_eq!(cart.read(0x1000), 0xBB);
    }

    #[test]
    fn two_kb_rom_mirrors() {
        let mut rom = vec![0u8; 2048];
        rom[0] = 0x42;
        let mut cart = Cartridge::from_rom(&rom).expect("2K");
        assert_eq!(cart.read(0x1000), 0x42);
        assert_eq!(cart.read(0x1800), 0x42);
    }
}
