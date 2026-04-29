//! Konami VRC2a (Mapper 22): two switchable 8 KiB PRG banks, fixed
//! final 16 KiB PRG window, 1 KiB CHR banking, and H/V mirroring.

use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// Konami VRC2a (Mapper 22): two switchable 8 KiB PRG banks, fixed
/// final 16 KiB PRG window, 1 KiB CHR banking, and H/V mirroring.
///
/// Mapper 22 is the VRC2a board wiring, where the two low register
/// address bits are wired as A1/A0 rather than A0/A1. VRC2a also
/// ignores the low CHR bank bit, so the effective 1 KiB CHR bank is
/// the 8-bit register value shifted right by one.
#[derive(Clone, Serialize, Deserialize)]
pub struct Vrc2a {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_banks: [u8; 2],
    chr_regs: [u8; 8],
    mirroring: Mirroring,
    latch_6000: u8,
}

impl Vrc2a {
    /// Construct mapper 22 from parsed PRG/CHR payloads.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_banks: [0, 1],
            chr_regs: [0; 8],
            mirroring: Mirroring::Vertical,
            latch_6000: 0,
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn read_prg_8k(&self, bank: usize, offset: u16) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + usize::from(offset)]
    }

    fn register_index(addr: u16) -> u16 {
        ((addr >> 1) & 0x01) | ((addr & 0x01) << 1)
    }

    fn chr_slot(addr: u16) -> Option<usize> {
        let region = match addr & 0xF000 {
            0xB000 => 0,
            0xC000 => 2,
            0xD000 => 4,
            0xE000 => 6,
            _ => return None,
        };
        Some(region + usize::from(Self::register_index(addr) / 2))
    }

    fn chr_bank(&self, slot: usize) -> usize {
        usize::from(self.chr_regs[slot] >> 1)
    }
}

impl Mapper for Vrc2a {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x6FFF => (addr >> 8) as u8 & 0xFE | (self.latch_6000 & 1),
            0x8000..=0x9FFF => self.read_prg_8k(usize::from(self.prg_banks[0]), addr - 0x8000),
            0xA000..=0xBFFF => self.read_prg_8k(usize::from(self.prg_banks[1]), addr - 0xA000),
            0xC000..=0xDFFF => {
                self.read_prg_8k(self.prg_8k_count().saturating_sub(2), addr - 0xC000)
            }
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, addr - 0xE000),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x6FFF => self.latch_6000 = value & 1,
            0x8000..=0x8FFF => self.prg_banks[0] = value & 0x1F,
            0x9000..=0x9FFF => {
                self.mirroring = if value & 1 == 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            }
            0xA000..=0xAFFF => self.prg_banks[1] = value & 0x1F,
            0xB000..=0xEFFF => {
                if let Some(slot) = Self::chr_slot(addr) {
                    if Self::register_index(addr) & 1 == 0 {
                        self.chr_regs[slot] = (self.chr_regs[slot] & 0xF0) | (value & 0x0F);
                    } else {
                        self.chr_regs[slot] = (self.chr_regs[slot] & 0x0F) | ((value & 0x0F) << 4);
                    }
                }
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let slot = usize::from((addr & 0x1FFF) / 0x0400);
        let offset = usize::from(addr & 0x03FF);
        let index = self.chr_bank(slot) * 1024 + offset;
        self.chr[index % self.chr.len()]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            let slot = usize::from((addr & 0x1FFF) / 0x0400);
            let offset = usize::from(addr & 0x03FF);
            let index = (self.chr_bank(slot) * 1024 + offset) % self.chr.len();
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Vrc2a(self.clone())
    }
}
