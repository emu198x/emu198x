//! Atari 2600 cartridge handling.
//!
//! Supports 2KB and 4KB (no banking) ROMs, plus F8 (8KB, 2 banks),
//! F6 (16KB, 4 banks), and F4 (32KB, 8 banks) bank-switching.
//!
//! Bank switching uses hotspot detection: reads or writes to specific
//! addresses in the $1000-$1FFF range trigger bank switches.

/// Cartridge banking scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankingScheme {
    /// 2KB or 4KB, no banking.
    None,
    /// F8: 8KB, 2 banks. Hotspots $1FF8/$1FF9.
    F8,
    /// F6: 16KB, 4 banks. Hotspots $1FF6-$1FF9.
    F6,
    /// F4: 32KB, 8 banks. Hotspots $1FF4-$1FFB.
    F4,
}

/// An Atari 2600 cartridge.
pub struct Cartridge {
    /// Full ROM data.
    rom: Vec<u8>,
    /// Banking scheme.
    scheme: BankingScheme,
    /// Current bank number.
    bank: usize,
    /// Bank size in bytes (4096 for all banked schemes, up to 4096 for None).
    bank_size: usize,
}

impl Cartridge {
    /// Parse a ROM file and detect the banking scheme.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not a valid Atari 2600 size.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let (scheme, bank_size) = match data.len() {
            0..=2048 => (BankingScheme::None, data.len()),
            2049..=4096 => (BankingScheme::None, data.len()),
            8192 => (BankingScheme::F8, 4096),
            16384 => (BankingScheme::F6, 4096),
            32768 => (BankingScheme::F4, 4096),
            other => return Err(format!("Unsupported ROM size: {other} bytes")),
        };

        // Start at last bank (where reset vector lives).
        let num_banks = if bank_size > 0 { data.len() / bank_size } else { 1 };
        let bank = num_banks.saturating_sub(1);

        Ok(Self {
            rom: data.to_vec(),
            scheme,
            bank,
            bank_size,
        })
    }

    /// Read a byte from the cartridge ROM address space ($1000-$1FFF).
    ///
    /// Also performs hotspot detection for bank switching.
    pub fn read(&mut self, addr: u16) -> u8 {
        self.check_hotspot(addr);

        let offset = (addr & 0x0FFF) as usize;
        let bank_offset = self.bank * self.bank_size;

        if self.bank_size <= 2048 {
            // 2KB ROM: mirror within 4KB window.
            self.rom[offset % self.rom.len()]
        } else {
            let idx = bank_offset + offset;
            if idx < self.rom.len() {
                self.rom[idx]
            } else {
                0
            }
        }
    }

    /// Write to cartridge address space (for hotspot detection only).
    pub fn write(&mut self, addr: u16, _value: u8) {
        self.check_hotspot(addr);
    }

    /// Check for bank-switching hotspots.
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
        let rom = vec![0xEA; 2048];
        let cart = Cartridge::from_rom(&rom).expect("2K ROM");
        assert_eq!(cart.scheme, BankingScheme::None);
    }

    #[test]
    fn detect_4k_rom() {
        let rom = vec![0xEA; 4096];
        let cart = Cartridge::from_rom(&rom).expect("4K ROM");
        assert_eq!(cart.scheme, BankingScheme::None);
    }

    #[test]
    fn detect_f8_rom() {
        let rom = vec![0xEA; 8192];
        let cart = Cartridge::from_rom(&rom).expect("F8 ROM");
        assert_eq!(cart.scheme, BankingScheme::F8);
        assert_eq!(cart.bank, 1); // Starts at last bank
    }

    #[test]
    fn detect_f6_rom() {
        let rom = vec![0xEA; 16384];
        let cart = Cartridge::from_rom(&rom).expect("F6 ROM");
        assert_eq!(cart.scheme, BankingScheme::F6);
        assert_eq!(cart.bank, 3);
    }

    #[test]
    fn detect_f4_rom() {
        let rom = vec![0xEA; 32768];
        let cart = Cartridge::from_rom(&rom).expect("F4 ROM");
        assert_eq!(cart.scheme, BankingScheme::F4);
        assert_eq!(cart.bank, 7);
    }

    #[test]
    fn reject_invalid_size() {
        let rom = vec![0; 5000];
        assert!(Cartridge::from_rom(&rom).is_err());
    }

    #[test]
    fn f8_bank_switching() {
        let mut rom = vec![0; 8192];
        // Bank 0: fill with 0xAA
        rom[..4096].fill(0xAA);
        // Bank 1: fill with 0xBB
        rom[4096..].fill(0xBB);

        let mut cart = Cartridge::from_rom(&rom).expect("F8 ROM");
        // Starts at bank 1
        assert_eq!(cart.read(0x1000), 0xBB);

        // Switch to bank 0
        cart.read(0x1FF8);
        assert_eq!(cart.read(0x1000), 0xAA);

        // Switch back to bank 1
        cart.read(0x1FF9);
        assert_eq!(cart.read(0x1000), 0xBB);
    }

    #[test]
    fn two_kb_rom_mirrors() {
        let mut rom = vec![0; 2048];
        rom[0] = 0x42;
        let mut cart = Cartridge::from_rom(&rom).expect("2K ROM");

        // $1000 and $1800 should both read 0x42 (mirrored)
        assert_eq!(cart.read(0x1000), 0x42);
        assert_eq!(cart.read(0x1800), 0x42);
    }
}
