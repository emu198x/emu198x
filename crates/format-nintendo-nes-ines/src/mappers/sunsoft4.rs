//! Sunsoft-4 (Mapper 68): 16 KiB PRG banking, 2 KiB CHR banking,
//! and optional CHR-ROM nametable banking.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// Sunsoft-4 (Mapper 68): 16 KiB PRG banking, 2 KiB CHR banking,
/// and optional CHR-ROM nametable banking.
///
/// Used by *After Burner*. When CHR-ROM nametable mode is enabled,
/// the mapper supplies reads for `$2000-$2FFF`; writes are consumed
/// and ignored because the backing store is ROM.
#[derive(Clone, Serialize, Deserialize)]
pub struct Sunsoft4 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    prg_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
    chr_banks: [u8; 4],
    nt_banks: [u8; 2],
    nt_rom_mode: bool,
    prg_ram_enabled: bool,
}

impl Sunsoft4 {
    /// Construct mapper 68 from parsed ROM data and header mirroring.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_rom = if chr_data.is_empty() {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr_rom,
            prg_ram: [0; 8192],
            mirroring,
            prg_bank: 0,
            chr_banks: [0; 4],
            nt_banks: [0x80; 2],
            nt_rom_mode: false,
            prg_ram_enabled: false,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 16384).max(1)
    }

    fn read_prg_16k(&self, bank: usize, offset: u16) -> u8 {
        let bank = bank % self.prg_bank_count();
        self.prg_rom[bank * 16384 + usize::from(offset)]
    }

    fn chr_2k_bank_count(&self) -> usize {
        (self.chr_rom.len() / 2048).max(1)
    }

    fn chr_1k_bank_count(&self) -> usize {
        (self.chr_rom.len() / 1024).max(1)
    }

    fn nametable_slot(&self, addr: u16) -> usize {
        let page = ((addr - 0x2000) & 0x0FFF) / 0x0400;
        match self.mirroring {
            Mirroring::Vertical => usize::from(page & 1),
            Mirroring::Horizontal => usize::from(page / 2),
            Mirroring::SingleScreenLower => 0,
            Mirroring::SingleScreenUpper => 1,
            Mirroring::FourScreen => usize::from(page & 1),
        }
    }
}

impl Mapper for Sunsoft4 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xBFFF => self.read_prg_16k(usize::from(self.prg_bank), addr - 0x8000),
            0xC000..=0xFFFF => self.read_prg_16k(self.prg_bank_count() - 1, addr - 0xC000),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled => {
                self.prg_ram[usize::from(addr - 0x6000)] = value;
            }
            0x8000..=0x8FFF => self.chr_banks[0] = value,
            0x9000..=0x9FFF => self.chr_banks[1] = value,
            0xA000..=0xAFFF => self.chr_banks[2] = value,
            0xB000..=0xBFFF => self.chr_banks[3] = value,
            0xC000..=0xCFFF => self.nt_banks[0] = 0x80 | (value & 0x7F),
            0xD000..=0xDFFF => self.nt_banks[1] = 0x80 | (value & 0x7F),
            0xE000..=0xEFFF => {
                self.nt_rom_mode = value & 0x10 != 0;
                self.mirroring = match value & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            0xF000..=0xFFFF => {
                self.prg_bank = value & 0x0F;
                self.prg_ram_enabled = value & 0x10 != 0;
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let slot = usize::from((addr & 0x1FFF) / 0x0800);
        let bank = usize::from(self.chr_banks[slot]) % self.chr_2k_bank_count();
        let offset = usize::from(addr & 0x07FF);
        self.chr_rom[bank * 2048 + offset]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        if !self.nt_rom_mode {
            return None;
        }
        let slot = self.nametable_slot(addr);
        let bank = usize::from(self.nt_banks[slot]) % self.chr_1k_bank_count();
        let offset = usize::from(addr & 0x03FF);
        Some(self.chr_rom[bank * 1024 + offset])
    }

    fn nametable_write(&mut self, _addr: u16, _value: u8) -> bool {
        self.nt_rom_mode
    }

    fn save_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    fn restore_save_ram(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.prg_ram.len());
        self.prg_ram[..n].copy_from_slice(&bytes[..n]);
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Sunsoft4(self.clone())
    }
}
