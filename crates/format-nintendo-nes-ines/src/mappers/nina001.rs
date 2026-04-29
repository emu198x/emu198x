//! NINA-001 / NINA-002 (Mapper 34): 32 KiB PRG banking plus two
//! switchable 4 KiB CHR-ROM windows.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// NINA-001 / NINA-002 (Mapper 34): 32 KiB PRG banking plus two
/// switchable 4 KiB CHR-ROM windows.
///
/// The bank registers live at `$7FFD-$7FFF`, overlaid on the
/// cartridge's 8 KiB PRG RAM window. Reads return the RAM byte, while
/// writes update both RAM and the corresponding register.
#[derive(Clone, Serialize, Deserialize)]
pub struct Nina001 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    prg_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
}

impl Nina001 {
    /// Construct NINA-001 from parsed PRG/CHR ROM and fixed header
    /// mirroring.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            prg_ram: [0; 8192],
            mirroring,
            prg_bank: 0,
            chr_bank_0: 0,
            chr_bank_1: 1,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 32768).max(1)
    }

    fn chr_4k_count(&self) -> usize {
        (self.chr_rom.len() / 4096).max(1)
    }
}

impl Mapper for Nina001 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xFFFF => {
                let bank = usize::from(self.prg_bank & 0x03) % self.prg_bank_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 32768 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.prg_ram[usize::from(addr - 0x6000)] = value;
        }

        match addr {
            0x7FFD => self.prg_bank = value & 0x03,
            0x7FFE => self.chr_bank_0 = value & 0x0F,
            0x7FFF => self.chr_bank_1 = value & 0x0F,
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr = usize::from(addr) & 0x1FFF;
        let bank = if addr < 0x1000 {
            self.chr_bank_0
        } else {
            self.chr_bank_1
        };
        let offset = addr & 0x0FFF;
        let bank = usize::from(bank) % self.chr_4k_count();
        self.chr_rom[bank * 4096 + offset]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {
        // NINA-001 has CHR ROM, not CHR RAM.
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Nina001(self.clone())
    }
}
