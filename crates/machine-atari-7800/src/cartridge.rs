//! Atari 7800 cartridge handling.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-7800/src/cartridge.rs`
//! (RULES.md rule 27).
//!
//! Supports flat ROM mapping and SuperGame banking (up to 128 KB).
//!
//! Flat mapping places the ROM at the top of `$4000-$FFFF`:
//!
//! - 16 KB: `$C000-$FFFF`
//! - 32 KB: `$8000-$FFFF`
//! - 48 KB: `$4000-$FFFF`
//!
//! SuperGame banking (ROM > 48 KB, up to 128 KB):
//!
//! - Bank 7 permanently mapped at `$C000-$FFFF`
//! - Writes to `$8000-$BFFF` select the bank visible in that window
//! - 8 banks of 16 KB each
//!
//! The A78 header (128 bytes starting `01 49 87 01`) is detected and
//! stripped automatically.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BankingScheme {
    Flat { base: u16 },
    SuperGame,
}

pub struct Cartridge {
    rom: Vec<u8>,
    banking: BankingScheme,
    bank: usize,
}

impl Cartridge {
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let rom_data = Self::strip_a78_header(data);
        let (banking, rom) = match rom_data.len() {
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
            49153..=131_072 => {
                let mut padded = vec![0xFF; 131_072];
                padded[..rom_data.len()].copy_from_slice(rom_data);
                (BankingScheme::SuperGame, padded)
            }
            other => return Err(format!("Unsupported cartridge size: {other} bytes")),
        };
        Ok(Self { rom, banking, bank: 0 })
    }

    fn strip_a78_header(data: &[u8]) -> &[u8] {
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
                    self.rom
                        .get(7 * 0x4000 + (addr as usize - 0xC000))
                        .copied()
                        .unwrap_or(0xFF)
                } else if addr >= 0x8000 {
                    self.rom
                        .get(self.bank * 0x4000 + (addr as usize - 0x8000))
                        .copied()
                        .unwrap_or(0xFF)
                } else if addr >= 0x4000 {
                    self.rom
                        .get(6 * 0x4000 + (addr as usize - 0x4000))
                        .copied()
                        .unwrap_or(0xFF)
                } else {
                    0xFF
                }
            }
        }
    }

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
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("16K");
        assert_eq!(cart.banking, BankingScheme::Flat { base: 0xC000 });
    }

    #[test]
    fn detect_32k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 32768]).expect("32K");
        assert_eq!(cart.banking, BankingScheme::Flat { base: 0x8000 });
    }

    #[test]
    fn detect_48k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 49152]).expect("48K");
        assert_eq!(cart.banking, BankingScheme::Flat { base: 0x4000 });
    }

    #[test]
    fn detect_128k_supergame() {
        let cart = Cartridge::from_rom(&vec![0xEA; 131_072]).expect("128K");
        assert_eq!(cart.banking, BankingScheme::SuperGame);
    }

    #[test]
    fn reject_oversized_rom() {
        assert!(Cartridge::from_rom(&vec![0u8; 256_000]).is_err());
    }

    #[test]
    fn flat_32k_read() {
        let mut rom = vec![0xFF; 32768];
        rom[0] = 0x42;
        rom[0x7FFC] = 0x00;
        rom[0x7FFD] = 0x80;
        let cart = Cartridge::from_rom(&rom).expect("32K");
        assert_eq!(cart.read(0x8000), 0x42);
        assert_eq!(cart.read(0xFFFC), 0x00);
        assert_eq!(cart.read(0xFFFD), 0x80);
        assert_eq!(cart.read(0x4000), 0xFF);
    }

    #[test]
    fn supergame_bank_switching() {
        let mut rom = vec![0; 131_072];
        rom[0x0000] = 0xAA;
        rom[0xC000] = 0xBB;
        rom[7 * 0x4000] = 0xCC;
        let mut cart = Cartridge::from_rom(&rom).expect("128K");
        assert_eq!(cart.read(0x8000), 0xAA);
        assert_eq!(cart.read(0xC000), 0xCC);
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
        data[128] = 0x42;
        let cart = Cartridge::from_rom(&data).expect("A78");
        assert_eq!(cart.read(0x8000), 0x42);
    }
}
