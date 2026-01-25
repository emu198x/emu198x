//! NES cartridge and iNES ROM format.

use crate::mapper::{self, Mapper};

/// Nametable mirroring mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mirroring {
    /// Horizontal mirroring (vertical scrolling games).
    Horizontal,
    /// Vertical mirroring (horizontal scrolling games).
    Vertical,
    /// Single-screen, lower bank.
    SingleLower,
    /// Single-screen, upper bank.
    SingleUpper,
    /// Four-screen (cartridge provides extra VRAM).
    FourScreen,
}

/// NES cartridge.
pub struct Cartridge {
    /// PRG ROM data.
    prg_rom: Vec<u8>,
    /// CHR ROM/RAM data.
    chr_rom: Vec<u8>,
    /// PRG RAM (battery-backed save RAM).
    prg_ram: Vec<u8>,
    /// Mapper implementation.
    mapper: Box<dyn Mapper>,
    /// Base mirroring mode (from iNES header).
    mirroring: Mirroring,
    /// Has battery-backed RAM.
    has_battery: bool,
}

impl Cartridge {
    /// Load cartridge from iNES ROM data.
    pub fn from_ines(data: &[u8]) -> Result<Self, String> {
        // Check header
        if data.len() < 16 {
            return Err("ROM too small for iNES header".to_string());
        }

        if &data[0..4] != b"NES\x1A" {
            return Err("Invalid iNES header magic".to_string());
        }

        let prg_rom_size = data[4] as usize * 16384; // 16KB units
        let chr_rom_size = data[5] as usize * 8192; // 8KB units
        let flags6 = data[6];
        let flags7 = data[7];

        let mirroring = if flags6 & 0x08 != 0 {
            Mirroring::FourScreen
        } else if flags6 & 0x01 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        let has_battery = flags6 & 0x02 != 0;
        let has_trainer = flags6 & 0x04 != 0;

        let mapper_num = (flags6 >> 4) | (flags7 & 0xF0);

        // Skip header (and trainer if present)
        let prg_start = 16 + if has_trainer { 512 } else { 0 };
        let chr_start = prg_start + prg_rom_size;

        if data.len() < chr_start + chr_rom_size {
            return Err("ROM file truncated".to_string());
        }

        let prg_rom = data[prg_start..prg_start + prg_rom_size].to_vec();
        let chr_rom = if chr_rom_size > 0 {
            data[chr_start..chr_start + chr_rom_size].to_vec()
        } else {
            // CHR RAM (8KB default)
            vec![0; 8192]
        };

        let mapper = mapper::create(mapper_num, prg_rom.len(), chr_rom.len(), mirroring)?;

        Ok(Self {
            prg_rom,
            chr_rom,
            prg_ram: vec![0; 8192], // 8KB PRG RAM
            mapper,
            mirroring,
            has_battery,
        })
    }

    /// Read from PRG space ($4020-$FFFF).
    pub fn prg_read(&self, addr: u16) -> u8 {
        if addr >= 0x6000 && addr < 0x8000 {
            // PRG RAM
            self.prg_ram[(addr - 0x6000) as usize & 0x1FFF]
        } else if addr >= 0x8000 {
            // PRG ROM
            let mapped = self.mapper.map_prg_read(addr);
            self.prg_rom[mapped % self.prg_rom.len()]
        } else {
            0
        }
    }

    /// Write to PRG space.
    pub fn prg_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x6000 && addr < 0x8000 {
            // PRG RAM
            self.prg_ram[(addr - 0x6000) as usize & 0x1FFF] = value;
        } else if addr >= 0x8000 {
            // Mapper register write
            self.mapper.write(addr, value);
        }
    }

    /// Read from CHR space ($0000-$1FFF).
    pub fn chr_read(&self, addr: u16) -> u8 {
        let mapped = self.mapper.map_chr_read(addr);
        self.chr_rom[mapped % self.chr_rom.len()]
    }

    /// Write to CHR space (CHR RAM only).
    pub fn chr_write(&mut self, addr: u16, value: u8) {
        let mapped = self.mapper.map_chr_read(addr);
        if mapped < self.chr_rom.len() {
            self.chr_rom[mapped] = value;
        }
    }

    /// Mirror nametable address based on current mirroring mode.
    pub fn mirror_nametable(&self, addr: u16) -> u16 {
        let mirroring = self.mapper.mirroring().unwrap_or(self.mirroring);
        let addr = addr & 0x0FFF;

        match mirroring {
            Mirroring::Horizontal => {
                // $2000 = $2400, $2800 = $2C00
                ((addr >> 1) & 0x400) | (addr & 0x3FF)
            }
            Mirroring::Vertical => {
                // $2000 = $2800, $2400 = $2C00
                addr & 0x7FF
            }
            Mirroring::SingleLower => addr & 0x3FF,
            Mirroring::SingleUpper => 0x400 | (addr & 0x3FF),
            Mirroring::FourScreen => addr & 0xFFF, // Needs extra VRAM
        }
    }

    /// Check if cartridge has battery-backed RAM.
    pub fn has_battery(&self) -> bool {
        self.has_battery
    }

    /// Get save RAM for persistence.
    pub fn save_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    /// Load save RAM.
    pub fn load_save_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.prg_ram.len());
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_rom(mapper: u8, prg_banks: u8, chr_banks: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 16 + (prg_banks as usize * 16384) + (chr_banks as usize * 8192)];
        rom[0..4].copy_from_slice(b"NES\x1A");
        rom[4] = prg_banks;
        rom[5] = chr_banks;
        rom[6] = mapper << 4;
        rom
    }

    #[test]
    fn test_load_nrom() {
        let rom = make_test_rom(0, 2, 1);
        let cart = Cartridge::from_ines(&rom).unwrap();
        assert!(!cart.has_battery());
    }

    #[test]
    fn test_invalid_header() {
        let rom = vec![0u8; 16];
        assert!(Cartridge::from_ines(&rom).is_err());
    }
}
