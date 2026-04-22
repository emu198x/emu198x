use serde_big_array::BigArray;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use std::path::Path;

/// Timex TC2048 memory: 16K ROM + 48K RAM.
///
/// Same layout as the standard 48K Spectrum. The TC2048 has an
/// additional 8K of "EXROM" space but doesn't use DOCK/EXROM paging
/// (that's TC2068/TS2068).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MemoryTC2048 {
    #[serde(with = "BigArray")]
    rom: [u8; 16384],
    #[serde(with = "BigArray")]
    ram: [u8; 49152],
}

impl MemoryTC2048 {
    pub fn new() -> Self {
        Self {
            rom: [0; 16384],
            ram: [0; 49152],
        }
    }

    pub fn load_rom(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ROM should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.rom.copy_from_slice(&data);
        Ok(())
    }

    pub fn load_rom_data(&mut self, data: &[u8]) {
        let len = data.len().min(16384);
        self.rom[..len].copy_from_slice(&data[..len]);
    }
}

impl Default for MemoryTC2048 {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBus for MemoryTC2048 {
    #[inline]
    fn read(&self, addr: u16) -> u8 {
        if addr < 0x4000 {
            self.rom[addr as usize]
        } else {
            self.ram[(addr - 0x4000) as usize]
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        if addr >= 0x4000 {
            self.ram[(addr - 0x4000) as usize] = val;
        }
    }

    #[inline]
    fn is_contended(&self, addr: u16) -> bool {
        addr >= 0x4000 && addr < 0x8000
    }
}
