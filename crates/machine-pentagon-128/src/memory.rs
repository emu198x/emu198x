use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::snapshot::Paged128kMemory;
use serde_big_array::BigArray;
use std::path::Path;

/// A single 16 KB bank — newtype wrapping the flat array so `BigArray`
/// can handle the >32-element serde limit inside a nested `[Bank; N]`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Bank16K(#[serde(with = "BigArray")] [u8; 16384]);

impl Bank16K {
    const fn zeroed() -> Self {
        Self([0; 16384])
    }
}

impl std::ops::Deref for Bank16K {
    type Target = [u8; 16384];
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Bank16K {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Pentagon 128 memory: 2 × 16K ROM + 8 × 16K RAM banks + TR-DOS ROM.
///
/// Same paging as the Sinclair 128K via port $7FFD. The critical
/// difference: **no contention**. The Pentagon ULA never competes
/// with the CPU for the memory bus.
///
/// The TR-DOS ROM lives in a separate slot and is paged in by the
/// Beta disk interface (not by $7FFD), so it can replace either of
/// the two normal ROMs transparently.
/// Banks live behind `Vec<Bank16K>` so that `serde`'s deserializer
/// processes one 16 KB chunk at a time into heap memory rather than
/// materialising the whole 176 KB of inline arrays on the caller's
/// stack.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MemoryPentagon {
    rom: Vec<Bank16K>,
    #[serde(with = "BigArray")]
    trdos_rom: [u8; 16384],
    ram: Vec<Bank16K>,
    paging: u8,
    locked: bool,
}

impl MemoryPentagon {
    pub fn new() -> Self {
        Self {
            rom: vec![Bank16K::zeroed(); 2],
            trdos_rom: [0; 16384],
            ram: vec![Bank16K::zeroed(); 8],
            paging: 0,
            locked: false,
        }
    }

    pub fn load_roms(&mut self, rom0: &[u8], rom1: &[u8]) {
        let len0 = rom0.len().min(16384);
        self.rom[0][..len0].copy_from_slice(&rom0[..len0]);
        let len1 = rom1.len().min(16384);
        self.rom[1][..len1].copy_from_slice(&rom1[..len1]);
    }

    pub fn load_rom0(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ROM should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.rom[0].copy_from_slice(&data);
        Ok(())
    }

    pub fn load_rom1(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ROM should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.rom[1].copy_from_slice(&data);
        Ok(())
    }

    pub fn load_trdos_rom(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("TR-DOS ROM should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.trdos_rom.copy_from_slice(&data);
        Ok(())
    }

    /// Read from the TR-DOS ROM — used when Beta disk is paged in.
    pub fn read_trdos_rom(&self, addr: u16) -> u8 {
        self.trdos_rom[addr as usize & 0x3FFF]
    }

    pub fn write_7ffd(&mut self, val: u8) {
        if self.locked { return; }
        self.paging = val;
        if val & 0x20 != 0 {
            self.locked = true;
        }
    }

    pub fn current_bank(&self) -> u8 {
        self.paging & 0x07
    }

    pub fn current_rom(&self) -> u8 {
        (self.paging >> 4) & 0x01
    }

    pub fn screen_bank(&self) -> u8 {
        if self.paging & 0x08 != 0 { 7 } else { 5 }
    }
}

impl Default for MemoryPentagon {
    fn default() -> Self {
        Self::new()
    }
}

impl Paged128kMemory for MemoryPentagon {
    fn write_7ffd(&mut self, val: u8) {
        MemoryPentagon::write_7ffd(self, val)
    }
}

impl MemoryBus for MemoryPentagon {
    #[inline]
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.rom[self.current_rom() as usize][addr as usize],
            0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize],
            0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize],
            0xC000..=0xFFFF => self.ram[self.current_bank() as usize][(addr - 0xC000) as usize],
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x3FFF => {} // ROM
            0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize] = val,
            0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize] = val,
            0xC000..=0xFFFF => {
                let bank = self.current_bank() as usize;
                self.ram[bank][(addr - 0xC000) as usize] = val;
            }
        }
    }

    #[inline]
    fn is_contended(&self, _addr: u16) -> bool {
        false // Pentagon has no contention
    }

    #[inline]
    fn read_screen(&self, addr: u16) -> u8 {
        let bank = self.screen_bank() as usize;
        self.ram[bank][(addr & 0x3FFF) as usize]
    }
}
