//! GxROM (mapper 66): discrete GNROM/MHROM program and character banking.
//!
//! Evidence: umbrella reference/by-system/nintendo-nes/gxrom-evidence.md.
//! Nestopia's GxRom/GetBusData models AND bus conflicts; Mesen/ares default
//! classes omit them. Keep the old-bank ROM byte in the latch calculation.
use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring, PpuCartridge};
use crate::snapshot::MapperSnapshot;

/// Standard GxROM: 32 KiB PRG and 8 KiB CHR windows, fixed mirroring.
#[derive(Clone, Serialize, Deserialize)]
pub struct GxRom {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl GxRom {
    /// Construct a standard GNROM/MHROM board, starting deterministically in bank 0.
    ///
    /// # Errors
    /// Rejects extents outside the standard board's connected ROM address lines.
    /// Oversized homebrew boards and CHR-RAM variants need independent modelling.
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Result<Self, String> {
        if !matches!(prg_rom.len(), 32768 | 65536 | 131072)
            || !matches!(chr_rom.len(), 8192 | 16384 | 32768)
        {
            return Err(format!(
                "unsupported mapper 66 ROM layout: PRG {} bytes, CHR {} bytes; standard GxROM requires 32/64/128 KiB PRG and 8/16/32 KiB CHR ROM",
                prg_rom.len(),
                chr_rom.len()
            ));
        }
        Ok(Self {
            prg_rom,
            chr_rom,
            mirroring,
            prg_bank: 0,
            chr_bank: 0,
        })
    }
}

impl PpuCartridge for GxRom {
    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank = usize::from(self.chr_bank) % (self.chr_rom.len() / 8192);
        self.chr_rom[bank * 8192 + (usize::from(addr) & 0x1FFF)]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Mapper for GxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        if addr < 0x8000 {
            return 0;
        }
        let bank = usize::from(self.prg_bank) % (self.prg_rom.len() / 32768);
        self.prg_rom[bank * 32768 + usize::from(addr - 0x8000)]
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let effective = value & self.cpu_read(addr);
            self.prg_bank = (effective >> 4) & 3;
            self.chr_bank = effective & 3;
        }
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::GxRom(self.clone())
    }
}
