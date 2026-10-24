//! Color Dreams (Mapper 11): switchable 32 KiB PRG plus 8 KiB CHR ROM.

use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// Color Dreams (Mapper 11): switchable 32 KiB PRG plus 8 KiB CHR ROM.
///
/// The latch format is `CCCC LLPP`: bits 0-1 select the 32 KiB PRG
/// bank, bits 4-7 select the 8 KiB CHR bank, and bits 2-3 are lockout
/// defeat lines with no emulation-visible effect. The board has bus
/// conflicts, so writes latch `CPU value & ROM byte`.
#[derive(Clone, Serialize, Deserialize)]
pub struct ColorDreams {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl ColorDreams {
    /// Construct mapper 11 from parsed PRG/CHR ROM and fixed mirroring.
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
            mirroring,
            prg_bank: 0,
            chr_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 32768).max(1)
    }

    fn chr_bank_count(&self) -> usize {
        (self.chr_rom.len() / 8192).max(1)
    }
}

impl Mapper for ColorDreams {
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
            let effective = value & self.cpu_read(addr);
            self.prg_bank = effective & 0x03;
            self.chr_bank = (effective >> 4) & 0x0F;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank = usize::from(self.chr_bank) % self.chr_bank_count();
        let offset = usize::from(addr) & 0x1FFF;
        self.chr_rom[bank * 8192 + offset]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::ColorDreams(self.clone())
    }
}
