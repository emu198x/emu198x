//! MBC1 — the most common DMG mapper.
//!
//! Bank-switch register layout (writes to ROM space):
//!
//! | Range            | Effect                                              |
//! |------------------|-----------------------------------------------------|
//! | `$0000..=$1FFF`  | RAM enable: low nibble == `$A` enables, anything else disables |
//! | `$2000..=$3FFF`  | ROM bank lower 5 bits (writing 0 reads as 1)        |
//! | `$4000..=$5FFF`  | 2-bit "secondary" — either ROM bank bits 6-5 or RAM bank |
//! | `$6000..=$7FFF`  | Banking mode: 0 = ROM mode, 1 = RAM mode            |
//!
//! In ROM mode (default), the secondary bits become bits 5-6 of the
//! ROM bank for the `$4000..=$7FFF` window; the `$0000..=$3FFF`
//! window is fixed to bank 0. In RAM mode, the secondary bits select
//! the RAM bank, and on large ROMs (≥ 1 MiB) they also pick which
//! 16 KiB bank the `$0000..=$3FFF` window points at (bank 0 / 32 /
//! 64 / 96).

use serde::{Deserialize, Serialize};

const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mbc1 {
    pub ram_enabled: bool,
    /// Lower 5 bits of the ROM bank ($00 reads as $01).
    pub bank_low: u8,
    /// 2-bit "secondary" — ROM bits 5-6 in mode 0, RAM bank in mode 1.
    pub bank_high: u8,
    /// Banking mode: `false` = ROM mode, `true` = RAM mode.
    pub mode_advanced: bool,
}

impl Mbc1 {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ram_enabled: false,
            bank_low: 1,
            bank_high: 0,
            mode_advanced: false,
        }
    }

    pub(crate) fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank = if addr < 0x4000 {
            // $0000..=$3FFF: bank 0 in ROM mode; in RAM mode the
            // secondary bits act as ROM bits 5-6 (used by carts ≥1 MiB).
            if self.mode_advanced {
                usize::from(self.bank_high & 0b11) << 5
            } else {
                0
            }
        } else {
            // $4000..=$7FFF: lower 5 bits + (in ROM mode) secondary as bits 5-6.
            let mut bank = usize::from(self.bank_low & 0x1F);
            if !self.mode_advanced {
                bank |= usize::from(self.bank_high & 0b11) << 5;
            }
            bank
        };
        let offset = bank * ROM_BANK_SIZE + usize::from(addr & 0x3FFF);
        rom.get(offset).copied().unwrap_or(0xFF)
    }

    pub(crate) fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (value & 0x0F) == 0x0A,
            0x2000..=0x3FFF => {
                let mut low = value & 0x1F;
                if low == 0 {
                    low = 1; // 0 → 1 quirk applies on write
                }
                self.bank_low = low;
            }
            0x4000..=0x5FFF => self.bank_high = value & 0b11,
            0x6000..=0x7FFF => self.mode_advanced = (value & 1) != 0,
            _ => {}
        }
    }

    pub(crate) fn read_ram(&self, ram: &[u8], addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        let offset = self.ram_offset(addr);
        ram.get(offset).copied().unwrap_or(0xFF)
    }

    pub(crate) fn write_ram(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if !self.ram_enabled {
            return;
        }
        let offset = self.ram_offset(addr);
        if let Some(slot) = ram.get_mut(offset) {
            *slot = value;
        }
    }

    fn ram_offset(&self, addr: u16) -> usize {
        let local = usize::from(addr.wrapping_sub(0xA000) & 0x1FFF);
        if self.mode_advanced {
            usize::from(self.bank_high & 0b11) * RAM_BANK_SIZE + local
        } else {
            local
        }
    }
}
