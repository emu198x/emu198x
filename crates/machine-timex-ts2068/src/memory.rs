use serde_big_array::BigArray;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use std::path::Path;

/// Timex TC2068/TS2068 memory with DOCK/EXROM paging.
///
/// The 64K address space is divided into 8 × 8K chunks. Port $F4
/// selects whether each chunk maps to HOME (normal Spectrum memory)
/// or DOCK (cartridge). When DOCK is selected but no cartridge is
/// present, reads return $FF.
///
/// Port $F4 (DOCK bank select):
///   Bit N = 0: chunk N maps to HOME
///   Bit N = 1: chunk N maps to DOCK cartridge
///
/// HOME memory map (standard Spectrum):
///   Chunks 0-1 ($0000-$3FFF): ROM
///   Chunks 2-7 ($4000-$FFFF): RAM
///
/// The TS2068 also has a built-in EXROM (8K extension ROM) that maps
/// at $0000-$1FFF when port $FF bit 7 is set. See `set_exrom_enabled`
/// and the read path below.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MemoryTimex {
    #[serde(with = "BigArray")]
    rom: [u8; 16384],
    #[serde(with = "BigArray")]
    exrom: [u8; 8192],
    #[serde(with = "BigArray")]
    ram: [u8; 49152],
    /// Port $F4: DOCK bank select (bit per 8K chunk).
    dock_select: u8,
    /// Port $FF bit 7: EXROM enable.
    exrom_enabled: bool,
}

impl MemoryTimex {
    pub fn new() -> Self {
        Self {
            rom: [0; 16384],
            exrom: [0; 8192],
            ram: [0; 49152],
            dock_select: 0,
            exrom_enabled: false,
        }
    }

    pub fn load_rom(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        let len = data.len().min(16384);
        self.rom[..len].copy_from_slice(&data[..len]);
        Ok(())
    }

    pub fn load_rom_data(&mut self, data: &[u8]) {
        let len = data.len().min(16384);
        self.rom[..len].copy_from_slice(&data[..len]);
    }

    /// Load the 8K EXROM.
    pub fn load_exrom(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        let len = data.len().min(8192);
        self.exrom[..len].copy_from_slice(&data[..len]);
        Ok(())
    }

    /// Set EXROM enable from port $FF bit 7.
    pub fn set_exrom_enabled(&mut self, enabled: bool) {
        self.exrom_enabled = enabled;
    }

    /// Write to port $F4 (DOCK bank select).
    pub fn write_f4(&mut self, val: u8) {
        self.dock_select = val;
    }

    /// Read port $F4.
    pub fn read_f4(&self) -> u8 {
        self.dock_select
    }

    /// Is the given 8K chunk mapped to DOCK?
    fn is_dock(&self, chunk: usize) -> bool {
        self.dock_select & (1 << chunk) != 0
    }
}

impl Default for MemoryTimex {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBus for MemoryTimex {
    #[inline]
    fn read(&self, addr: u16) -> u8 {
        let chunk = (addr >> 13) as usize; // 0-7
        if self.is_dock(chunk) {
            return 0xFF; // No cartridge — DOCK reads return $FF
        }
        // HOME memory
        if addr < 0x2000 && self.exrom_enabled {
            // EXROM mapped at $0000-$1FFF when enabled
            self.exrom[addr as usize]
        } else if addr < 0x4000 {
            self.rom[addr as usize]
        } else {
            self.ram[(addr - 0x4000) as usize]
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        let chunk = (addr >> 13) as usize;
        if self.is_dock(chunk) {
            return; // DOCK writes ignored (no cartridge)
        }
        if addr >= 0x4000 {
            self.ram[(addr - 0x4000) as usize] = val;
        }
    }

    #[inline]
    fn is_contended(&self, addr: u16) -> bool {
        addr >= 0x4000 && addr < 0x8000
    }
}
