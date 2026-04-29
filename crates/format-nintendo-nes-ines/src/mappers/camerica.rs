//! Camerica / Codemasters (Mapper 71): switchable 16 KiB low PRG
//! bank with a fixed final high bank and CHR RAM.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// Camerica / Codemasters (Mapper 71): switchable 16 KiB low PRG
/// bank with a fixed final high bank and CHR RAM.
#[derive(Clone, Serialize, Deserialize)]
pub struct Camerica {
    prg_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    chr_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
}

impl Camerica {
    /// Construct mapper 71 from parsed PRG ROM and fixed header mirroring.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            mirroring,
            prg_bank: 0,
        }
    }

    fn prg_16k_count(&self) -> usize {
        (self.prg_rom.len() / 16384).max(1)
    }
}

impl Mapper for Camerica {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = usize::from(self.prg_bank) % self.prg_16k_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 16384 + offset]
            }
            0xC000..=0xFFFF => {
                let bank = self.prg_16k_count() - 1;
                let offset = usize::from(addr - 0xC000);
                self.prg_rom[bank * 16384 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x9000..=0x9FFF => {
                self.mirroring = if value & 0x10 != 0 {
                    Mirroring::SingleScreenUpper
                } else {
                    Mirroring::SingleScreenLower
                };
            }
            0xC000..=0xFFFF => self.prg_bank = value,
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_ram[usize::from(addr) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        self.chr_ram[usize::from(addr) & 0x1FFF] = value;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Camerica(self.clone())
    }
}
