//! MBC5 — used by Donkey Kong Country, Pokémon Crystal (CGB), every
//! Color-only release. Up to 8 MiB ROM and 128 KiB RAM with no
//! quirks: bank 0 is always selectable, no banking-mode toggle.
//!
//! | Range            | Effect                                      |
//! |------------------|---------------------------------------------|
//! | `$0000..=$1FFF`  | RAM enable: low nibble == `$A` enables      |
//! | `$2000..=$2FFF`  | ROM bank low 8 bits                         |
//! | `$3000..=$3FFF`  | ROM bank bit 9                              |
//! | `$4000..=$5FFF`  | RAM bank (4 bits)                           |

use serde::{Deserialize, Serialize};

const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mbc5 {
    pub ram_enabled: bool,
    pub rom_bank: u16,
    pub ram_bank: u8,
}

impl Mbc5 {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
        }
    }

    pub(crate) fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank_count = rom.len() / ROM_BANK_SIZE;
        if bank_count == 0 {
            return 0xFF;
        }

        let bank = if addr < 0x4000 {
            0
        } else {
            usize::from(self.rom_bank)
        } % bank_count;
        let offset = bank * ROM_BANK_SIZE + usize::from(addr & 0x3FFF);
        rom.get(offset).copied().unwrap_or(0xFF)
    }

    pub(crate) fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (value & 0x0F) == 0x0A,
            0x2000..=0x2FFF => {
                self.rom_bank = (self.rom_bank & 0x0100) | u16::from(value);
            }
            0x3000..=0x3FFF => {
                self.rom_bank = (self.rom_bank & 0x00FF) | (u16::from(value & 1) << 8);
            }
            0x4000..=0x5FFF => self.ram_bank = value & 0x0F,
            _ => {}
        }
    }

    pub(crate) fn read_ram(&self, ram: &[u8], addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        let Some(offset) = self.ram_offset(ram, addr) else {
            return 0xFF;
        };
        ram[offset]
    }

    pub(crate) fn write_ram(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if !self.ram_enabled {
            return;
        }
        if let Some(offset) = self.ram_offset(ram, addr) {
            ram[offset] = value;
        }
    }

    fn ram_offset(&self, ram: &[u8], addr: u16) -> Option<usize> {
        let bank_count = ram.len() / RAM_BANK_SIZE;
        if bank_count == 0 {
            return None;
        }

        let bank = usize::from(self.ram_bank & 0x0F) % bank_count;
        let local = usize::from(addr.wrapping_sub(0xA000) & 0x1FFF);
        Some(bank * RAM_BANK_SIZE + local)
    }
}
