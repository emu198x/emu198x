use common_sinclair_zx_spectrum::memory::MemoryBus;
use serde_big_array::BigArray;
use std::path::Path;

/// Timex TC2068/TS2068 memory with DOCK/EXROM paging.
///
/// The 64K address space is divided into 8 × 8K chunks. Port $F4
/// selects whether each chunk maps to HOME or the external source chosen
/// by port $FF bit 7: DOCK (0) or EXROM (1). Empty DOCK reads return $FF.
/// Source: TS2068 Technical Manual §§2.1.8.1 and 2.1.13.5.
///
/// Port $F4 (DOCK bank select):
///   Bit N = 0: chunk N maps to HOME
///   Bit N = 1: chunk N maps to the selected external source
///
/// HOME memory map (standard Spectrum):
///   Chunks 0-1 ($0000-$3FFF): ROM
///   Chunks 2-7 ($4000-$FFFF): RAM
///
/// The built-in 8K EXROM is mirrored in each selected external chunk.
/// Port $FF bit 7 alone does not change any HOME mapping. ULA screen
/// fetches continue reading physical HOME RAM independently of CPU paging.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MemoryTimex {
    #[serde(with = "BigArray")]
    rom: [u8; 16384],
    #[serde(with = "BigArray")]
    exrom: [u8; 8192],
    #[serde(with = "BigArray")]
    ram: [u8; 49152],
    /// Port $F4: external chunk select (bit per 8K chunk).
    dock_select: u8,
    /// Port $FF bit 7: external-source selection (1 = EXROM, 0 = DOCK).
    exrom_enabled: bool,
}

/// Source selected by the current Timex memory implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemorySource {
    /// The internal HOME ROM.
    HomeRom,
    /// The internal HOME RAM.
    HomeRam,
    /// The extension ROM in the low 8 KiB window.
    Exrom,
    /// A selected DOCK window with no cartridge backing.
    EmptyDock,
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

    /// Load the 8K EXROM from a byte slice (firmware-bundle path).
    pub fn load_exrom_data(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.exrom[..len].copy_from_slice(&data[..len]);
    }

    /// Select the external source from port $FF bit 7.
    pub fn set_exrom_enabled(&mut self, enabled: bool) {
        self.exrom_enabled = enabled;
    }

    /// Write to port $F4 (external chunk select).
    pub fn write_f4(&mut self, val: u8) {
        self.dock_select = val;
    }

    /// Read port $F4.
    pub fn read_f4(&self) -> u8 {
        self.dock_select
    }

    /// Observe the source of a read without accessing data or changing paging.
    #[must_use]
    pub fn mapped_source(&self, address: u16) -> MemorySource {
        if self.is_dock(usize::from(address >> 13)) {
            if self.exrom_enabled {
                MemorySource::Exrom
            } else {
                MemorySource::EmptyDock
            }
        } else if address < 0x4000 {
            MemorySource::HomeRom
        } else {
            MemorySource::HomeRam
        }
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
        match self.mapped_source(addr) {
            MemorySource::HomeRom => self.rom[usize::from(addr)],
            MemorySource::HomeRam => self.ram[usize::from(addr - 0x4000)],
            MemorySource::Exrom => self.exrom[usize::from(addr & 0x1fff)],
            MemorySource::EmptyDock => 0xff,
        }
    }

    #[inline]
    fn read_screen(&self, addr: u16) -> u8 {
        // SCLD display DMA accesses HOME RAM, not the CPU's external bank.
        if addr >= 0x4000 {
            self.ram[usize::from(addr - 0x4000)]
        } else {
            self.rom[usize::from(addr)]
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        let chunk = (addr >> 13) as usize;
        if self.is_dock(chunk) {
            return; // EXROM is read-only; no DOCK cartridge is fitted.
        }
        if addr >= 0x4000 {
            self.ram[(addr - 0x4000) as usize] = val;
        }
    }

    #[inline]
    fn is_contended(&self, addr: u16) -> bool {
        (0x4000..0x8000).contains(&addr)
    }
}
