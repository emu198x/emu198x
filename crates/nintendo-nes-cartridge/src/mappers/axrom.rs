//! AxROM (Mapper 7): switchable 32 KiB PRG bank with single-screen
//! mirroring.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// AxROM (Mapper 7): switchable 32 KiB PRG bank with single-screen
/// mirroring.
///
/// AxROM boards use CHR RAM and switch the whole CPU `$8000-$FFFF`
/// PRG window at once. Bit 4 of the latched bank register selects
/// lower vs upper single-screen nametable mirroring.
#[derive(Clone, Serialize, Deserialize)]
pub struct AxRom {
    prg_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    chr_ram: [u8; 8192],
    bank: u8,
    mirroring: Mirroring,
}

impl AxRom {
    /// Construct AxROM from parsed PRG ROM. CHR is always 8 KiB RAM.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>) -> Self {
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            bank: 0,
            mirroring: Mirroring::SingleScreenLower,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 32768).max(1)
    }
}

impl Mapper for AxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = (usize::from(self.bank) & 0x07) % self.prg_bank_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[(bank * 32768 + offset) % self.prg_rom.len()]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let effective = value & self.cpu_read(addr);
            self.bank = effective & 0x07;
            self.mirroring = if effective & 0x10 != 0 {
                Mirroring::SingleScreenUpper
            } else {
                Mirroring::SingleScreenLower
            };
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
        MapperSnapshot::AxRom(self.clone())
    }
}
