//! ZX Spectrum 48K memory map.
//!
//! Source references:
//! - `wiki/systems/spectrum/overview.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/memory.rs`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/machine-sinclair-zx-spectrum-48k/src/memory.rs`
//!
//! The fresh implementation keeps only the 48K memory map and removes all file
//! I/O from the component boundary.

use crate::error::RomImageError;
use crate::timing::is_contended_address_48k;

const ROM_BYTES_48K: usize = 16 * 1024;
const RAM_BYTES_48K: usize = 48 * 1024;
const ROM_END: u16 = 0x3fff;
const RAM_BASE: u16 = 0x4000;

/// Spectrum-family memory surface used by the machine and ULA layers.
pub trait MemoryBus {
    /// Reads one byte from the machine address space.
    fn read(&self, addr: u16) -> u8;

    /// Writes one byte to the machine address space.
    ///
    /// ROM writes are silently ignored.
    fn write(&mut self, addr: u16, value: u8);

    /// Returns `true` if the address lies in contended RAM.
    fn is_contended(&self, addr: u16) -> bool;

    /// Reads one byte from the ULA-visible screen bank.
    ///
    /// On the 48K machine this is identical to `read`.
    fn read_screen(&self, addr: u16) -> u8 {
        self.read(addr)
    }
}

/// 48K Spectrum memory: 16 KiB ROM plus 48 KiB RAM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spectrum48kMemory {
    rom: [u8; ROM_BYTES_48K],
    ram: [u8; RAM_BYTES_48K],
}

impl Spectrum48kMemory {
    /// Creates a zero-initialized 48K memory map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rom: [0; ROM_BYTES_48K],
            ram: [0; RAM_BYTES_48K],
        }
    }

    /// Creates memory from a full 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(rom: [u8; ROM_BYTES_48K]) -> Self {
        Self {
            rom,
            ram: [0; RAM_BYTES_48K],
        }
    }

    /// Loads a ROM image from a byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error when the ROM image is not exactly 16 KiB.
    pub fn load_rom_bytes(&mut self, bytes: &[u8]) -> Result<(), RomImageError> {
        if bytes.len() != ROM_BYTES_48K {
            return Err(RomImageError::WrongSize {
                actual: bytes.len(),
            });
        }

        self.rom.copy_from_slice(bytes);
        Ok(())
    }

    /// Returns the ROM bytes.
    #[must_use]
    pub fn rom(&self) -> &[u8; ROM_BYTES_48K] {
        &self.rom
    }

    /// Returns the RAM bytes.
    #[must_use]
    pub fn ram(&self) -> &[u8; RAM_BYTES_48K] {
        &self.ram
    }

    /// Returns mutable RAM bytes.
    #[must_use]
    pub fn ram_mut(&mut self) -> &mut [u8; RAM_BYTES_48K] {
        &mut self.ram
    }
}

impl Default for Spectrum48kMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBus for Spectrum48kMemory {
    fn read(&self, addr: u16) -> u8 {
        if addr <= ROM_END {
            self.rom[usize::from(addr)]
        } else {
            self.ram[usize::from(addr - RAM_BASE)]
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        if addr >= RAM_BASE {
            self.ram[usize::from(addr - RAM_BASE)] = value;
        }
    }

    fn is_contended(&self, addr: u16) -> bool {
        is_contended_address_48k(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rom_reads_through_lower_16k() {
        let mut rom = [0u8; ROM_BYTES_48K];
        rom[0x0000] = 0x3e;
        rom[0x1234] = 0xa5;
        rom[0x3fff] = 0x76;

        let memory = Spectrum48kMemory::with_rom(rom);
        assert_eq!(memory.read(0x0000), 0x3e);
        assert_eq!(memory.read(0x1234), 0xa5);
        assert_eq!(memory.read(0x3fff), 0x76);
    }

    #[test]
    fn rom_writes_are_ignored() {
        let mut memory = Spectrum48kMemory::new();
        memory.write(0x0001, 0xaa);
        assert_eq!(memory.read(0x0001), 0x00);
    }

    #[test]
    fn ram_reads_and_writes_cover_full_upper_48k() {
        let mut memory = Spectrum48kMemory::new();

        memory.write(0x4000, 0x11);
        memory.write(0x8000, 0x22);
        memory.write(0xffff, 0x33);

        assert_eq!(memory.read(0x4000), 0x11);
        assert_eq!(memory.read(0x8000), 0x22);
        assert_eq!(memory.read(0xffff), 0x33);
    }

    #[test]
    fn screen_reads_match_normal_reads_on_48k() {
        let mut memory = Spectrum48kMemory::new();
        memory.write(0x4000, 0x5a);
        memory.write(0x57ff, 0xc3);

        assert_eq!(memory.read_screen(0x4000), 0x5a);
        assert_eq!(memory.read_screen(0x57ff), 0xc3);
    }

    #[test]
    fn contention_is_only_in_lower_ram_bank() {
        let memory = Spectrum48kMemory::new();
        assert!(!memory.is_contended(0x3fff));
        assert!(memory.is_contended(0x4000));
        assert!(memory.is_contended(0x7fff));
        assert!(!memory.is_contended(0x8000));
        assert!(!memory.is_contended(0xffff));
    }

    #[test]
    fn rom_loader_requires_exact_16k_image() {
        let mut memory = Spectrum48kMemory::new();
        let error = memory
            .load_rom_bytes(&[0u8; 42])
            .expect_err("42-byte image should be rejected");

        assert_eq!(error, RomImageError::WrongSize { actual: 42 });
    }

    #[test]
    fn rom_loader_accepts_full_image() {
        let mut memory = Spectrum48kMemory::new();
        let rom = [0x7e; ROM_BYTES_48K];

        memory
            .load_rom_bytes(&rom)
            .expect("16 KiB image should load");

        assert_eq!(memory.read(0x0000), 0x7e);
        assert_eq!(memory.read(0x3fff), 0x7e);
    }
}
