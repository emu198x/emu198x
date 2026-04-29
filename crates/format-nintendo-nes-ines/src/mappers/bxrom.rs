//! BxROM / BNROM (Mapper 34): switchable 32 KiB PRG bank with CHR RAM.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// BxROM / BNROM (Mapper 34): switchable 32 KiB PRG bank with CHR RAM.
///
/// The iNES mapper 34 assignment is historically ambiguous. The parser
/// chooses this variant for CHR-RAM images (`CHR=0`) and
/// [`Nina001`](crate::Nina001) for CHR-ROM images.
#[derive(Clone, Serialize, Deserialize)]
pub struct BxRom {
    prg_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    chr_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
}

impl BxRom {
    /// Construct BxROM from parsed PRG ROM and fixed header mirroring.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            mirroring,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 32768).max(1)
    }
}

impl Mapper for BxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = usize::from(self.prg_bank) % self.prg_bank_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 32768 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = value & self.cpu_read(addr);
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
        MapperSnapshot::BxRom(self.clone())
    }
}
