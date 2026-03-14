//! Atari 7800 cartridge handling.
//!
//! Supports flat ROM mapping and `SuperGame` banking (128KB).
//!
//! Flat mapping places the ROM at the top of $4000-$FFFF:
//! - 16KB: $C000-$FFFF
//! - 32KB: $8000-$FFFF
//! - 48KB: $4000-$FFFF
//!
//! `SuperGame` banking (ROM > 48KB, up to 128KB):
//! - Bank 7 permanently mapped at $C000-$FFFF
//! - Writes to $8000-$BFFF select the bank visible in that window
//! - 8 banks of 16KB each
//!
//! The A78 header (128 bytes starting with bytes 1/49/87/01) is
//! detected and stripped automatically.

/// Banking scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BankingScheme {
    /// ROM placed at a fixed base address, no banking.
    Flat { base: u16 },
    /// `SuperGame`: 8 x 16KB banks, bank 7 fixed at $C000.
    SuperGame,
}

/// An Atari 7800 cartridge.
pub struct Cartridge {
    rom: Vec<u8>,
    banking: BankingScheme,
    /// Current bank for the $8000-$BFFF window (`SuperGame` only).
    bank: usize,
}

impl Cartridge {
    /// Create a cartridge from raw ROM data (with or without A78 header).
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not supported.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let rom_data = Self::strip_a78_header(data);

        let (banking, rom) = match rom_data.len() {
            // Standard flat sizes.
            0..=16384 => {
                let mut padded = vec![0xFF; 16384];
                let start = 16384 - rom_data.len();
                padded[start..].copy_from_slice(rom_data);
                (BankingScheme::Flat { base: 0xC000 }, padded)
            }
            16385..=32768 => {
                let mut padded = vec![0xFF; 32768];
                let start = 32768 - rom_data.len();
                padded[start..].copy_from_slice(rom_data);
                (BankingScheme::Flat { base: 0x8000 }, padded)
            }
            32769..=49152 => {
                let mut padded = vec![0xFF; 49152];
                let start = 49152 - rom_data.len();
                padded[start..].copy_from_slice(rom_data);
                (BankingScheme::Flat { base: 0x4000 }, padded)
            }
            // Larger ROMs use SuperGame banking.
            49153..=131_072 => {
                let mut padded = vec![0xFF; 131_072];
                padded[..rom_data.len()].copy_from_slice(rom_data);
                (BankingScheme::SuperGame, padded)
            }
            other => return Err(format!("Unsupported cartridge size: {other} bytes")),
        };

        Ok(Self {
            rom,
            banking,
            bank: 0,
        })
    }

    /// Detect and strip a 128-byte A78 header if present.
    fn strip_a78_header(data: &[u8]) -> &[u8] {
        // A78 header signature: bytes 1, 49, 87, 01 at offset 0.
        if data.len() > 128
            && data[0] == 0x01
            && data[1] == 0x49
            && data[2] == 0x87
            && data[3] == 0x01
        {
            &data[128..]
        } else {
            data
        }
    }

    /// Read a byte from cartridge space ($4000-$FFFF).
    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        match self.banking {
            BankingScheme::Flat { base } => {
                if addr < base {
                    return 0xFF;
                }
                let offset = (addr - base) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            BankingScheme::SuperGame => {
                if addr >= 0xC000 {
                    // Bank 7 is permanently mapped here.
                    let offset = 7 * 0x4000 + (addr as usize - 0xC000);
                    self.rom.get(offset).copied().unwrap_or(0xFF)
                } else if addr >= 0x8000 {
                    // Switchable bank window.
                    let offset = self.bank * 0x4000 + (addr as usize - 0x8000);
                    self.rom.get(offset).copied().unwrap_or(0xFF)
                } else if addr >= 0x4000 {
                    // $4000-$7FFF: bank 6 for SuperGame.
                    let offset = 6 * 0x4000 + (addr as usize - 0x4000);
                    self.rom.get(offset).copied().unwrap_or(0xFF)
                } else {
                    0xFF
                }
            }
        }
    }

    /// Non-mutating read for MARIA DMA (doesn't trigger bank switching).
    #[must_use]
    pub fn read_pure(&self, addr: u16) -> u8 {
        self.read(addr)
    }

    /// Write to cartridge space -- only meaningful for `SuperGame` bank switching.
    pub fn write(&mut self, addr: u16, value: u8) {
        if self.banking == BankingScheme::SuperGame && (0x8000..0xC000).contains(&addr) {
            self.bank = (value as usize) & 0x07;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_16k_rom() {
        let rom = vec![0xEA; 16384];
        let cart = Cartridge::from_rom(&rom).expect("16K ROM");
        assert_eq!(cart.banking, BankingScheme::Flat { base: 0xC000 });
    }

    #[test]
    fn detect_32k_rom() {
        let rom = vec![0xEA; 32768];
        let cart = Cartridge::from_rom(&rom).expect("32K ROM");
        assert_eq!(cart.banking, BankingScheme::Flat { base: 0x8000 });
    }

    #[test]
    fn detect_48k_rom() {
        let rom = vec![0xEA; 49152];
        let cart = Cartridge::from_rom(&rom).expect("48K ROM");
        assert_eq!(cart.banking, BankingScheme::Flat { base: 0x4000 });
    }

    #[test]
    fn detect_128k_supergame() {
        let rom = vec![0xEA; 131_072];
        let cart = Cartridge::from_rom(&rom).expect("128K ROM");
        assert_eq!(cart.banking, BankingScheme::SuperGame);
    }

    #[test]
    fn reject_oversized_rom() {
        let rom = vec![0; 256_000];
        assert!(Cartridge::from_rom(&rom).is_err());
    }

    #[test]
    fn flat_32k_read() {
        let mut rom = vec![0xFF; 32768];
        rom[0] = 0x42; // $8000
        rom[0x7FFC] = 0x00; // Reset vector low
        rom[0x7FFD] = 0x80; // Reset vector high
        let cart = Cartridge::from_rom(&rom).expect("32K ROM");

        assert_eq!(cart.read(0x8000), 0x42);
        assert_eq!(cart.read(0xFFFC), 0x00);
        assert_eq!(cart.read(0xFFFD), 0x80);
        assert_eq!(cart.read(0x4000), 0xFF); // Below base
    }

    #[test]
    fn supergame_bank_switching() {
        let mut rom = vec![0; 131_072];
        // Put marker bytes in bank 0 and bank 3.
        rom[0x0000] = 0xAA; // Bank 0, offset 0 ($8000 when selected)
        rom[0xC000] = 0xBB; // Bank 3, offset 0 ($8000 when selected)
        // Bank 7 at $C000 (fixed).
        rom[7 * 0x4000] = 0xCC;

        let mut cart = Cartridge::from_rom(&rom).expect("128K ROM");

        // Default bank 0.
        assert_eq!(cart.read(0x8000), 0xAA);
        // Fixed bank 7.
        assert_eq!(cart.read(0xC000), 0xCC);

        // Switch to bank 3.
        cart.write(0x8000, 3);
        assert_eq!(cart.read(0x8000), 0xBB);
    }

    #[test]
    fn strip_a78_header() {
        let mut data = vec![0; 128 + 32768];
        data[0] = 0x01;
        data[1] = 0x49;
        data[2] = 0x87;
        data[3] = 0x01;
        // ROM data after header.
        data[128] = 0x42;
        let cart = Cartridge::from_rom(&data).expect("A78 ROM");
        assert_eq!(cart.read(0x8000), 0x42);
    }
}
