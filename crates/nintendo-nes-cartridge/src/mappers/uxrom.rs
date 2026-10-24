//! UxROM (Mapper 2): one switchable 16 KiB PRG bank and one fixed
//! 16 KiB PRG bank.

use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// UxROM (Mapper 2): one switchable 16 KiB PRG bank and one fixed
/// 16 KiB PRG bank.
///
/// This common discrete-logic board family maps `$8000-$BFFF` to a
/// CPU-selected PRG bank and `$C000-$FFFF` to the final PRG bank.
/// Most UxROM cartridges use 8 KiB of CHR RAM; CHR ROM is also accepted
/// because the mapper trait can serve either layout.
#[derive(Clone, Serialize, Deserialize)]
pub struct UxRom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    prg_bank: u8,
}

impl UxRom {
    /// Construct UxROM from parsed iNES payloads.
    ///
    /// `chr_data` is empty for CHR-RAM cartridges; in that case this
    /// allocates the standard 8 KiB CHR RAM window.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
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
            mirroring,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 16384).max(1)
    }
}

impl Mapper for UxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = usize::from(self.prg_bank) % self.prg_bank_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 16384 + offset]
            }
            0xC000..=0xFFFF => {
                let bank = self.prg_bank_count() - 1;
                let offset = usize::from(addr - 0xC000);
                self.prg_rom[bank * 16384 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            // Discrete UxROM boards have bus conflicts: the value
            // latched by the bank-select register is the CPU value
            // AND the ROM byte simultaneously driving the bus.
            let rom_byte = self.cpu_read(addr);
            self.prg_bank = value & rom_byte;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[usize::from(addr) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr[usize::from(addr) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::UxRom(self.clone())
    }
}
