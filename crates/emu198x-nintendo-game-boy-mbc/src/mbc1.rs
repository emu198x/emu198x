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
const MBC1M_REGION_SIZE: usize = 0x40000;
const NINTENDO_LOGO_OFFSET: usize = 0x0104;
const NINTENDO_LOGO_SIZE: usize = 0x30;
const NINTENDO_LOGO: [u8; NINTENDO_LOGO_SIZE] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mbc1 {
    pub ram_enabled: bool,
    /// Lower 5 bits of the ROM bank ($00 reads as $01).
    pub bank_low: u8,
    /// 2-bit "secondary" — ROM bits 5-6 in mode 0, RAM bank in mode 1.
    pub bank_high: u8,
    /// Banking mode: `false` = ROM mode, `true` = RAM mode.
    pub mode_advanced: bool,
    /// MBC1M multicarts wire the secondary bank bits one bit lower
    /// to select 256 KiB sub-ROMs.
    #[serde(default)]
    pub multicart: bool,
}

impl Mbc1 {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ram_enabled: false,
            bank_low: 1,
            bank_high: 0,
            mode_advanced: false,
            multicart: false,
        }
    }

    #[must_use]
    pub fn new_for_rom(rom: &[u8]) -> Self {
        Self {
            multicart: looks_like_mbc1m(rom),
            ..Self::new()
        }
    }

    pub(crate) fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank_count = rom.len() / ROM_BANK_SIZE;
        if bank_count == 0 {
            return 0xFF;
        }

        let bank = if addr < 0x4000 {
            // $0000..=$3FFF: bank 0 in ROM mode; in RAM mode the
            // secondary bits act as ROM bits 5-6 (used by carts ≥1 MiB).
            if self.mode_advanced {
                self.high_bank_bits()
            } else {
                0
            }
        } else {
            // $4000..=$7FFF: lower 5 bits + secondary as bits 5-6.
            self.high_bank_bits()
                | if self.multicart {
                    usize::from(self.bank_low & 0x0F)
                } else {
                    usize::from(self.bank_low & 0x1F)
                }
        } % bank_count;
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

        let local = usize::from(addr.wrapping_sub(0xA000) & 0x1FFF);
        let bank = if self.mode_advanced {
            usize::from(self.bank_high & 0b11)
        } else {
            0
        } % bank_count;
        Some(bank * RAM_BANK_SIZE + local)
    }

    fn high_bank_bits(&self) -> usize {
        let shift = if self.multicart { 4 } else { 5 };
        usize::from(self.bank_high & 0b11) << shift
    }
}

fn looks_like_mbc1m(rom: &[u8]) -> bool {
    let Some(logo) = rom.get(NINTENDO_LOGO_OFFSET..NINTENDO_LOGO_OFFSET + NINTENDO_LOGO_SIZE)
    else {
        return false;
    };
    if logo != NINTENDO_LOGO {
        return false;
    }

    (1..=3).any(|region| {
        let offset = region * MBC1M_REGION_SIZE + NINTENDO_LOGO_OFFSET;
        rom.get(offset..offset + NINTENDO_LOGO_SIZE) == Some(logo)
    })
}
