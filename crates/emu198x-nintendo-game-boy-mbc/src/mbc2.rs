//! MBC2 — a ROM mapper with internal 512×4-bit RAM.
//!
//! Writes to `$0000..=$3FFF` are split by CPU address bit 8:
//!
//! | Range / bit      | Effect                                      |
//! |------------------|---------------------------------------------|
//! | `$0000..=$3FFF`, A8=0 | RAM enable: low nibble == `$A` enables |
//! | `$0000..=$3FFF`, A8=1 | ROM bank low 4 bits (`0` reads as `1`) |
//!
//! The RAM window only decodes 9 address bits (`$A000..=$A1FF`) and
//! stores the low nibble; reads return the high nibble set.

use serde::{Deserialize, Serialize};

const ROM_BANK_SIZE: usize = 0x4000;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mbc2 {
    pub ram_enabled: bool,
    /// 4-bit ROM bank. `0` reads as `1`.
    pub rom_bank: u8,
}

impl Mbc2 {
    pub const RAM_SIZE: usize = 0x200;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            ram_enabled: false,
            rom_bank: 1,
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
            usize::from(self.rom_bank.max(1)) % bank_count
        };
        let offset = bank * ROM_BANK_SIZE + usize::from(addr & 0x3FFF);
        rom.get(offset).copied().unwrap_or(0xFF)
    }

    pub(crate) fn write_rom(&mut self, addr: u16, value: u8) {
        if addr > 0x3FFF {
            return;
        }

        if (addr & 0x0100) == 0 {
            self.ram_enabled = (value & 0x0F) == 0x0A;
        } else {
            let bank = value & 0x0F;
            self.rom_bank = if bank == 0 { 1 } else { bank };
        }
    }

    pub(crate) fn read_ram(&self, ram: &[u8], addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }

        let offset = usize::from(addr.wrapping_sub(0xA000) & 0x01FF);
        ram.get(offset).copied().unwrap_or(0x0F) | 0xF0
    }

    pub(crate) fn write_ram(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if !self.ram_enabled {
            return;
        }

        let offset = usize::from(addr.wrapping_sub(0xA000) & 0x01FF);
        if let Some(slot) = ram.get_mut(offset) {
            *slot = value & 0x0F;
        }
    }
}
