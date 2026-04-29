//! CNROM (Mapper 3): fixed PRG ROM with switchable 8 KiB CHR ROM.

use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// CNROM (Mapper 3): fixed PRG ROM with switchable 8 KiB CHR ROM.
///
/// CNROM keeps PRG ROM unbanked at `$8000-$FFFF` and uses writes to
/// `$8000-$FFFF` to select the 8 KiB CHR bank visible to the PPU at
/// `$0000-$1FFF`. Most boards have bus conflicts, so the latched bank
/// value is the CPU value AND the ROM byte driving the bus.
#[derive(Clone, Serialize, Deserialize)]
pub struct CnRom {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    chr_bank: u8,
}

impl CnRom {
    /// Construct CNROM from parsed iNES payloads.
    ///
    /// CNROM is a CHR-ROM board. If a malformed image declares no CHR
    /// ROM, this allocates a zeroed 8 KiB bank so reads remain defined
    /// rather than panicking.
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
            chr_bank: 0,
        }
    }
}

impl Mapper for CnRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[offset % self.prg_rom.len()]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let rom_byte = self.cpu_read(addr);
            self.chr_bank = value & rom_byte;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank_offset = usize::from(self.chr_bank) * 8192;
        let offset = usize::from(addr) & 0x1FFF;
        self.chr_rom[(bank_offset + offset) % self.chr_rom.len()]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {
        // CNROM has CHR ROM, not CHR RAM.
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::CnRom(self.clone())
    }
}
