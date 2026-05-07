//! ZX Spectrum 128K memory map.
//!
//! Source references:
//! - `wiki/systems/spectrum/variants.md`
//! - `wiki/systems/spectrum/contention.md`
//!
//! Lives in the 128K-class layer crate because the Sinclair 128K and the
//! Sinclair-branded Amstrad-built +2 share the same memory layout
//! exactly. The 48K-class layer's split into a generic-over-`M` core
//! doesn't apply here — both variants take the same `Memory128K`.

use common_sinclair_zx_spectrum::memory::{Bank16K, MemoryBus};
use common_sinclair_zx_spectrum::snapshot::Paged128kMemory;
use std::path::Path;

/// ZX Spectrum 128K memory: 2 × 16K ROM + 8 × 16K RAM banks.
///
/// Address map (default paging):
///   $0000-$3FFF: ROM 0 (128K editor) or ROM 1 (48K BASIC)
///   $4000-$7FFF: RAM bank 5 (screen, always contended)
///   $8000-$BFFF: RAM bank 2 (never contended)
///   $C000-$FFFF: Switchable RAM bank (0-7, contended when odd: 1,3,5,7)
///
/// Port $7FFD controls paging:
///   Bits 0-2: RAM bank at $C000 (0-7)
///   Bit 3:    Screen bank (0 = bank 5, 1 = bank 7)
///   Bit 4:    ROM select (0 = 128K editor, 1 = 48K BASIC)
///   Bit 5:    Paging lock (1 = locked until reset)
///
/// Banks live behind `Vec<Bank16K>` so that `serde`'s deserializer
/// processes one 16 KB chunk at a time into heap memory rather than
/// materialising the whole 160 KB inline-array on the caller's stack.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Memory128K {
    rom: Vec<Bank16K>,
    ram: Vec<Bank16K>,
    /// Current port $7FFD value.
    paging: u8,
    /// True when paging is locked (bit 5 of $7FFD).
    locked: bool,
}

impl Memory128K {
    pub fn new() -> Self {
        Self {
            rom: vec![Bank16K::zeroed(); 2],
            ram: vec![Bank16K::zeroed(); 8],
            paging: 0,
            locked: false,
        }
    }

    /// Load ROM 0 (128K editor) from file.
    pub fn load_rom0(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ROM 0 should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.rom[0].copy_from_slice(&data);
        Ok(())
    }

    /// Load ROM 1 (48K BASIC) from file.
    pub fn load_rom1(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ROM 1 should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.rom[1].copy_from_slice(&data);
        Ok(())
    }

    /// Load ROM from byte slices (for testing / embedded ROMs).
    pub fn load_roms(&mut self, rom0: &[u8], rom1: &[u8]) {
        let len0 = rom0.len().min(16384);
        self.rom[0][..len0].copy_from_slice(&rom0[..len0]);
        let len1 = rom1.len().min(16384);
        self.rom[1][..len1].copy_from_slice(&rom1[..len1]);
    }

    /// Write to port $7FFD (memory paging control).
    pub fn write_7ffd(&mut self, val: u8) {
        if self.locked {
            return;
        }
        self.paging = val;
        if val & 0x20 != 0 {
            self.locked = true;
        }
    }

    /// Currently selected RAM bank at $C000 (bits 0-2 of $7FFD).
    pub fn current_bank(&self) -> u8 {
        self.paging & 0x07
    }

    /// Currently selected ROM (bit 4 of $7FFD): 0 = 128K editor, 1 = 48K BASIC.
    pub fn current_rom(&self) -> u8 {
        (self.paging >> 4) & 0x01
    }

    /// Screen bank: bit 3 of $7FFD. 0 = bank 5 ($4000), 1 = bank 7.
    pub fn screen_bank(&self) -> u8 {
        if self.paging & 0x08 != 0 { 7 } else { 5 }
    }

    /// Direct access to a RAM bank.
    pub fn ram_bank(&self, bank: usize) -> &[u8; 16384] {
        &self.ram[bank]
    }

    /// Mutable access to a RAM bank.
    pub fn ram_bank_mut(&mut self, bank: usize) -> &mut [u8; 16384] {
        &mut self.ram[bank]
    }

    /// Reads one byte from a specific ROM bank, ignoring the current
    /// `$7FFD` paging. Used by the runtime's screen-text decoder so
    /// it can reach the standard glyph table at `$3D00` of ROM 1
    /// (48 BASIC) even when ROM 0 (the 128 BASIC editor) is mapped
    /// at `$0000-$3FFF`. Returns `0` for out-of-range bank indices
    /// or addresses past the 16 KiB ROM bank.
    #[must_use]
    pub fn read_rom_byte(&self, bank: usize, addr: u16) -> u8 {
        self.rom
            .get(bank)
            .and_then(|rom| rom.get(addr as usize))
            .copied()
            .unwrap_or(0)
    }
}

impl Default for Memory128K {
    fn default() -> Self {
        Self::new()
    }
}

impl Paged128kMemory for Memory128K {
    fn write_7ffd(&mut self, val: u8) {
        Memory128K::write_7ffd(self, val)
    }
}

impl MemoryBus for Memory128K {
    #[inline]
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                let rom = self.current_rom() as usize;
                self.rom[rom][addr as usize]
            }
            0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize],
            0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize],
            0xC000..=0xFFFF => {
                let bank = self.current_bank() as usize;
                self.ram[bank][(addr - 0xC000) as usize]
            }
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x3FFF => {} // ROM — writes ignored
            0x4000..=0x7FFF => {
                self.ram[5][(addr - 0x4000) as usize] = val;
            }
            0x8000..=0xBFFF => {
                self.ram[2][(addr - 0x8000) as usize] = val;
            }
            0xC000..=0xFFFF => {
                let bank = self.current_bank() as usize;
                self.ram[bank][(addr - 0xC000) as usize] = val;
            }
        }
    }

    #[inline]
    fn is_contended(&self, addr: u16) -> bool {
        match addr {
            0x4000..=0x7FFF => true,
            0xC000..=0xFFFF => self.current_bank() & 1 != 0,
            _ => false,
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
    fn default_paging() {
        let mem = Memory128K::new();
        assert_eq!(mem.current_bank(), 0);
        assert_eq!(mem.current_rom(), 0);
        assert_eq!(mem.screen_bank(), 5);
    }

    #[test]
    fn bank_switching() {
        let mut mem = Memory128K::new();
        // Write distinct values to banks 0 and 3
        mem.ram[0][0] = 0xAA;
        mem.ram[3][0] = 0xBB;

        // Default: bank 0 at $C000
        assert_eq!(mem.read(0xC000), 0xAA);

        // Switch to bank 3
        mem.write_7ffd(0x03);
        assert_eq!(mem.read(0xC000), 0xBB);
        assert_eq!(mem.current_bank(), 3);
    }

    #[test]
    fn rom_switching() {
        let mut mem = Memory128K::new();
        mem.rom[0][0] = 0x11;
        mem.rom[1][0] = 0x22;

        // Default: ROM 0
        assert_eq!(mem.read(0x0000), 0x11);

        // Switch to ROM 1 (bit 4)
        mem.write_7ffd(0x10);
        assert_eq!(mem.read(0x0000), 0x22);
    }

    #[test]
    fn paging_lock() {
        let mut mem = Memory128K::new();
        mem.write_7ffd(0x23); // bank 3, lock bit set
        assert!(mem.locked);
        assert_eq!(mem.current_bank(), 3);

        // Further writes should be ignored
        mem.write_7ffd(0x00);
        assert_eq!(mem.current_bank(), 3); // Still 3, locked
    }

    #[test]
    fn contention_128k() {
        let mut mem = Memory128K::new();
        // Bank 5 at $4000 always contended
        assert!(mem.is_contended(0x4000));

        // $C000: bank 0 (even) = not contended
        assert!(!mem.is_contended(0xC000));

        // Switch to bank 1 (odd) = contended
        mem.write_7ffd(0x01);
        assert!(mem.is_contended(0xC000));

        // Switch to bank 4 (even) = not contended
        mem.write_7ffd(0x04);
        assert!(!mem.is_contended(0xC000));
    }

    #[test]
    fn bank5_always_at_4000() {
        let mut mem = Memory128K::new();
        mem.ram[5][0] = 0x42;
        assert_eq!(mem.read(0x4000), 0x42);

        // Even after paging changes, bank 5 stays at $4000
        mem.write_7ffd(0x07);
        assert_eq!(mem.read(0x4000), 0x42);
    }
}
