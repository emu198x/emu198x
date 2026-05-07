//! ZX Spectrum +2A/+2B/+3 memory map.
//!
//! Source references:
//! - `wiki/systems/spectrum/variants.md`
//! - `wiki/systems/spectrum/contention.md`
//!
//! Lives in the Amstrad-class layer crate because the +2A, +2B, and +3
//! all share the same memory layout exactly. Lifted verbatim from the
//! pre-D6 `machine-sinclair-zx-spectrum-plus/src/memory.rs`.

use common_sinclair_zx_spectrum::memory::{Bank16K, MemoryBus};
use common_sinclair_zx_spectrum::snapshot::Paged128kMemory;
use std::path::Path;

/// ZX Spectrum +2A/+2B/+3 memory: 4 × 16K ROM + 8 × 16K RAM banks.
///
/// Uses two paging registers:
///
/// Port $7FFD (same as 128K):
///   Bits 0-2: RAM bank at $C000 (0-7)
///   Bit 3:    Screen bank (0 = bank 5, 1 = bank 7)
///   Bit 4:    ROM select low bit
///   Bit 5:    Paging lock
///
/// Port $1FFD (+2A/+3 extension):
///   Bit 0:    Paging mode (0 = normal, 1 = special)
///   Bit 1:    Special mode config low bit
///   Bit 2:    ROM select high bit
///   Bit 3:    Disk motor (ignored)
///   Bit 4:    Printer strobe (ignored)
///
/// Normal mode: same as 128K but with 4 ROMs (selected by $7FFD bit 4 + $1FFD bit 2).
///
/// Special paging modes (bit 0 of $1FFD = 1):
///   Mode 0 ($1FFD bits 2-1 = 00): banks 0,1,2,3
///   Mode 1 ($1FFD bits 2-1 = 01): banks 4,5,6,7
///   Mode 2 ($1FFD bits 2-1 = 10): banks 4,5,6,3
///   Mode 3 ($1FFD bits 2-1 = 11): banks 4,7,6,3
/// Banks live behind `Vec<Bank16K>` so that `serde`'s deserializer
/// processes one 16 KB chunk at a time into heap memory rather than
/// materialising the whole 192 KB inline-array on the caller's stack.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MemoryPlus {
    rom: Vec<Bank16K>,
    ram: Vec<Bank16K>,
    /// Port $7FFD value.
    paging_7ffd: u8,
    /// Port $1FFD value.
    paging_1ffd: u8,
    /// Paging locked.
    locked: bool,
}

/// The 4 special paging configurations: [bank at $0000, $4000, $8000, $C000].
const SPECIAL_MODES: [[u8; 4]; 4] = [[0, 1, 2, 3], [4, 5, 6, 7], [4, 5, 6, 3], [4, 7, 6, 3]];

impl MemoryPlus {
    pub fn new() -> Self {
        Self {
            rom: vec![Bank16K::zeroed(); 4],
            ram: vec![Bank16K::zeroed(); 8],
            paging_7ffd: 0,
            paging_1ffd: 0,
            locked: false,
        }
    }

    /// Load all 4 ROMs from byte slices.
    pub fn load_roms(&mut self, rom0: &[u8], rom1: &[u8], rom2: &[u8], rom3: &[u8]) {
        for (i, data) in [rom0, rom1, rom2, rom3].iter().enumerate() {
            let len = data.len().min(16384);
            self.rom[i][..len].copy_from_slice(&data[..len]);
        }
    }

    /// Load a single ROM from file.
    pub fn load_rom(&mut self, index: usize, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ROM should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.rom[index].copy_from_slice(&data);
        Ok(())
    }

    /// Direct access to a RAM bank (used by snapshot loading and tests).
    pub fn ram_bank(&self, bank: usize) -> &[u8; 16384] {
        &self.ram[bank]
    }

    /// Mutable access to a RAM bank (used by snapshot loading and tests).
    pub fn ram_bank_mut(&mut self, bank: usize) -> &mut [u8; 16384] {
        &mut self.ram[bank]
    }

    pub fn write_7ffd(&mut self, val: u8) {
        if self.locked {
            return;
        }
        self.paging_7ffd = val;
        if val & 0x20 != 0 {
            self.locked = true;
        }
    }

    pub fn write_1ffd(&mut self, val: u8) {
        if self.locked {
            return;
        }
        self.paging_1ffd = val;
    }

    /// Is special paging mode active?
    fn special_mode(&self) -> bool {
        self.paging_1ffd & 0x01 != 0
    }

    /// Which special mode (0-3)?
    fn special_config(&self) -> usize {
        ((self.paging_1ffd >> 1) & 0x03) as usize
    }

    /// RAM bank at $C000 in normal mode.
    fn normal_bank(&self) -> usize {
        (self.paging_7ffd & 0x07) as usize
    }

    /// ROM index in normal mode: bit 4 of $7FFD (low) + bit 2 of $1FFD (high).
    fn normal_rom(&self) -> usize {
        let low = (self.paging_7ffd >> 4) & 0x01;
        let high = (self.paging_1ffd >> 2) & 0x01;
        ((high << 1) | low) as usize
    }

    pub fn screen_bank(&self) -> u8 {
        if self.paging_7ffd & 0x08 != 0 { 7 } else { 5 }
    }

    /// Reads one byte from a specific ROM bank, ignoring the current
    /// `$7FFD`/`$1FFD` paging. Used by the runtime's screen-text
    /// decoder to reach the standard glyph table at `$3D00` of ROM 3
    /// (48 BASIC sub-ROM) regardless of which of the four +3 ROMs is
    /// currently mapped at `$0000-$3FFF`. Returns `0` for
    /// out-of-range bank indices.
    #[must_use]
    pub fn read_rom_byte(&self, bank: usize, addr: u16) -> u8 {
        self.rom
            .get(bank)
            .and_then(|rom| rom.get(addr as usize))
            .copied()
            .unwrap_or(0)
    }
}

impl Default for MemoryPlus {
    fn default() -> Self {
        Self::new()
    }
}

impl Paged128kMemory for MemoryPlus {
    fn write_7ffd(&mut self, val: u8) {
        MemoryPlus::write_7ffd(self, val)
    }
}

impl MemoryBus for MemoryPlus {
    #[inline]
    fn read(&self, addr: u16) -> u8 {
        if self.special_mode() {
            let banks = &SPECIAL_MODES[self.special_config()];
            let slot = (addr >> 14) as usize;
            let offset = (addr & 0x3FFF) as usize;
            self.ram[banks[slot] as usize][offset]
        } else {
            match addr {
                0x0000..=0x3FFF => self.rom[self.normal_rom()][addr as usize],
                0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize],
                0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize],
                0xC000..=0xFFFF => self.ram[self.normal_bank()][(addr - 0xC000) as usize],
            }
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        if self.special_mode() {
            let banks = &SPECIAL_MODES[self.special_config()];
            let slot = (addr >> 14) as usize;
            let offset = (addr & 0x3FFF) as usize;
            self.ram[banks[slot] as usize][offset] = val;
        } else {
            match addr {
                0x0000..=0x3FFF => {} // ROM
                0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize] = val,
                0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize] = val,
                0xC000..=0xFFFF => {
                    let bank = self.normal_bank();
                    self.ram[bank][(addr - 0xC000) as usize] = val;
                }
            }
        }
    }

    #[inline]
    fn is_contended(&self, addr: u16) -> bool {
        if self.special_mode() {
            // In special mode, slots with banks 4-7 are contended
            let banks = &SPECIAL_MODES[self.special_config()];
            let slot = (addr >> 14) as usize;
            banks[slot] >= 4
        } else {
            match addr {
                0x4000..=0x7FFF => true, // Bank 5 always contended
                // $C000: contended when bank 4,5,6,7 is paged (NOT odd banks — Amstrad difference!)
                0xC000..=0xFFFF => self.normal_bank() >= 4,
                _ => false,
            }
        }
    }

    #[inline]
    fn read_screen(&self, addr: u16) -> u8 {
        let bank = self.screen_bank() as usize;
        self.ram[bank][(addr & 0x3FFF) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_paging() {
        let mut mem = MemoryPlus::new();
        mem.ram[0][0] = 0xAA;
        mem.ram[7][0] = 0xBB;

        assert_eq!(mem.read(0xC000), 0xAA); // bank 0 default
        mem.write_7ffd(0x07);
        assert_eq!(mem.read(0xC000), 0xBB); // bank 7
    }

    #[test]
    fn four_rom_select() {
        let mut mem = MemoryPlus::new();
        mem.rom[0][0] = 0x00;
        mem.rom[1][0] = 0x11;
        mem.rom[2][0] = 0x22;
        mem.rom[3][0] = 0x33;

        // ROM 0: $7FFD bit4=0, $1FFD bit2=0
        assert_eq!(mem.read(0x0000), 0x00);

        // ROM 1: $7FFD bit4=1
        mem.write_7ffd(0x10);
        assert_eq!(mem.read(0x0000), 0x11);

        // ROM 2: $7FFD bit4=0, $1FFD bit2=1
        mem.write_7ffd(0x00);
        mem.write_1ffd(0x04);
        assert_eq!(mem.read(0x0000), 0x22);

        // ROM 3: both bits set
        mem.write_7ffd(0x10);
        assert_eq!(mem.read(0x0000), 0x33);
    }

    #[test]
    fn special_paging_mode0() {
        let mut mem = MemoryPlus::new();
        mem.ram[0][0] = 0x00;
        mem.ram[1][0] = 0x11;
        mem.ram[2][0] = 0x22;
        mem.ram[3][0] = 0x33;

        // Special mode 0: banks 0,1,2,3
        mem.write_1ffd(0x01);
        assert_eq!(mem.read(0x0000), 0x00);
        assert_eq!(mem.read(0x4000), 0x11);
        assert_eq!(mem.read(0x8000), 0x22);
        assert_eq!(mem.read(0xC000), 0x33);
    }

    #[test]
    fn special_paging_mode1() {
        let mut mem = MemoryPlus::new();
        mem.ram[4][0] = 0x44;
        mem.ram[5][0] = 0x55;
        mem.ram[6][0] = 0x66;
        mem.ram[7][0] = 0x77;

        // Special mode 1: banks 4,5,6,7
        mem.write_1ffd(0x03); // bit 0=1 (special), bit 1=1 (config 1)
        assert_eq!(mem.read(0x0000), 0x44);
        assert_eq!(mem.read(0x4000), 0x55);
        assert_eq!(mem.read(0x8000), 0x66);
        assert_eq!(mem.read(0xC000), 0x77);
    }

    #[test]
    fn contention_plus2a() {
        let mut mem = MemoryPlus::new();
        // $4000 always contended (bank 5)
        assert!(mem.is_contended(0x4000));

        // Bank 0 at $C000 = not contended (< 4)
        assert!(!mem.is_contended(0xC000));

        // Bank 4 at $C000 = contended (>= 4)
        mem.write_7ffd(0x04);
        assert!(mem.is_contended(0xC000));

        // Bank 1 = NOT contended on +2A (unlike 128K where odd banks are contended)
        mem.write_7ffd(0x01);
        assert!(!mem.is_contended(0xC000));
    }
}
