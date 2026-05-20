//! ZX Spectrum 48K memory map.
//!
//! Source references:
//! - `wiki/systems/spectrum/overview.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/198x/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/memory.rs`
//! - Adapted from `/Users/stevehill/Projects/198x/Emu198x-Older/crates/machine-sinclair-zx-spectrum-48k/src/memory.rs`
//!
//! The fresh implementation keeps only the 48K memory map and removes all file
//! I/O from the component boundary.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::error::RomImageError;
use crate::timing::is_contended_address_48k;

const ROM_BYTES_48K: usize = 16 * 1024;
const RAM_BYTES_48K: usize = 48 * 1024;
const RAM_BYTES_16K: usize = 16 * 1024;
const ROM_END: u16 = 0x3fff;
const RAM_BASE: u16 = 0x4000;
const RAM_16K_END: u16 = 0x7fff;

/// One 16 KiB memory bank. Every paged Spectrum variant (128K, +2, +2A,
/// +2B, +3, Pentagon, Scorpion) lays its ROMs and RAM out as a sequence
/// of these. The newtype wraps the raw `[u8; 16384]` so `serde_big_array`
/// can handle the >32-element serde limit inside any enclosing array or
/// vector of banks.
#[derive(Clone, Serialize, Deserialize)]
pub struct Bank16K(#[serde(with = "BigArray")] [u8; 16 * 1024]);

impl Bank16K {
    /// Returns a freshly-zeroed bank.
    #[must_use]
    pub const fn zeroed() -> Self {
        Self([0; 16 * 1024])
    }
}

impl Default for Bank16K {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl std::ops::Deref for Bank16K {
    type Target = [u8; 16 * 1024];
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
///
/// The backing storage lives on the heap (behind `Vec<u8>`) rather than
/// inline on the stack — that keeps serde deserialization bounded to a
/// small stack footprint, which matters when an enclosing snapshot
/// struct is materialised from `postcard::from_bytes`. Lengths are
/// maintained as strict invariants via the public constructors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spectrum48kMemory {
    rom: Vec<u8>,
    ram: Vec<u8>,
}

impl Spectrum48kMemory {
    /// Creates a zero-initialized 48K memory map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rom: vec![0; ROM_BYTES_48K],
            ram: vec![0; RAM_BYTES_48K],
        }
    }

    /// Creates memory from a full 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(rom: [u8; ROM_BYTES_48K]) -> Self {
        Self {
            rom: rom.to_vec(),
            ram: vec![0; RAM_BYTES_48K],
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
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// Returns the RAM bytes.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Returns mutable RAM bytes.
    #[must_use]
    pub fn ram_mut(&mut self) -> &mut [u8] {
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

/// 16K Spectrum memory: 16 KiB ROM plus 16 KiB RAM.
///
/// Identical to the 48K layout below `$8000`. The upper 32 KiB of the
/// address space ($8000-$FFFF) is electrically disconnected — reads
/// return $FF and writes are silently dropped. The contention map is
/// the 48K map: only the single RAM bank at $4000-$7FFF is contended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spectrum16kMemory {
    rom: Vec<u8>,
    ram: Vec<u8>,
}

impl Spectrum16kMemory {
    /// Creates a zero-initialized 16K memory map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rom: vec![0; ROM_BYTES_48K],
            ram: vec![0; RAM_BYTES_16K],
        }
    }

    /// Creates memory from a full 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(rom: [u8; ROM_BYTES_48K]) -> Self {
        Self {
            rom: rom.to_vec(),
            ram: vec![0; RAM_BYTES_16K],
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
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// Returns the RAM bytes.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Returns mutable RAM bytes.
    #[must_use]
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }
}

impl Default for Spectrum16kMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBus for Spectrum16kMemory {
    fn read(&self, addr: u16) -> u8 {
        if addr <= ROM_END {
            self.rom[usize::from(addr)]
        } else if addr <= RAM_16K_END {
            self.ram[usize::from(addr - RAM_BASE)]
        } else {
            // $8000-$FFFF is electrically disconnected on the 16K — no
            // RAM, no floating bus from a paging chip. Returns $FF.
            0xFF
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        if (RAM_BASE..=RAM_16K_END).contains(&addr) {
            self.ram[usize::from(addr - RAM_BASE)] = value;
        }
        // ROM writes ignored; $8000-$FFFF writes silently dropped.
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

    // ---------------- 16K ----------------

    #[test]
    fn spectrum16k_new_is_zeroed() {
        let memory = Spectrum16kMemory::new();
        assert_eq!(memory.rom().len(), ROM_BYTES_48K);
        assert_eq!(memory.ram().len(), RAM_BYTES_16K);
        assert!(memory.rom().iter().all(|&b| b == 0));
        assert!(memory.ram().iter().all(|&b| b == 0));
    }

    #[test]
    fn spectrum16k_default_matches_new() {
        assert_eq!(Spectrum16kMemory::default(), Spectrum16kMemory::new());
    }

    #[test]
    fn spectrum16k_with_rom_initialises_rom_image() {
        let mut rom = [0u8; ROM_BYTES_48K];
        rom[0x0000] = 0x12;
        rom[0x1234] = 0x34;
        rom[0x3fff] = 0x56;

        let memory = Spectrum16kMemory::with_rom(rom);
        assert_eq!(memory.read(0x0000), 0x12);
        assert_eq!(memory.read(0x1234), 0x34);
        assert_eq!(memory.read(0x3fff), 0x56);
    }

    #[test]
    fn spectrum16k_rom_writes_are_ignored() {
        let mut memory = Spectrum16kMemory::new();
        memory.write(0x0001, 0xaa);
        assert_eq!(memory.read(0x0001), 0x00);
    }

    #[test]
    fn spectrum16k_ram_reads_and_writes_only_lower_16k() {
        let mut memory = Spectrum16kMemory::new();

        memory.write(0x4000, 0x11);
        memory.write(0x5fff, 0x22);
        memory.write(0x7fff, 0x33);

        assert_eq!(memory.read(0x4000), 0x11);
        assert_eq!(memory.read(0x5fff), 0x22);
        assert_eq!(memory.read(0x7fff), 0x33);
    }

    #[test]
    fn spectrum16k_upper_address_space_reads_return_ff() {
        let memory = Spectrum16kMemory::new();
        // $8000-$FFFF is electrically disconnected: no RAM, returns $FF.
        assert_eq!(memory.read(0x8000), 0xFF);
        assert_eq!(memory.read(0xC000), 0xFF);
        assert_eq!(memory.read(0xFFFF), 0xFF);
    }

    #[test]
    fn spectrum16k_upper_address_space_writes_silently_dropped() {
        let mut memory = Spectrum16kMemory::new();
        memory.write(0x8000, 0xAA);
        memory.write(0xC000, 0xBB);
        memory.write(0xFFFF, 0xCC);
        // Writes do not panic and do not affect lower RAM.
        assert_eq!(memory.read(0x8000), 0xFF);
        assert_eq!(memory.read(0xFFFF), 0xFF);
        assert_eq!(memory.read(0x4000), 0x00);
    }

    #[test]
    fn spectrum16k_ram_mut_allows_in_place_modification() {
        let mut memory = Spectrum16kMemory::new();
        memory.ram_mut()[0] = 0xDE;
        memory.ram_mut()[16 * 1024 - 1] = 0xAD;
        assert_eq!(memory.read(0x4000), 0xDE);
        assert_eq!(memory.read(0x7FFF), 0xAD);
    }

    #[test]
    fn spectrum16k_screen_reads_match_normal_reads() {
        let mut memory = Spectrum16kMemory::new();
        memory.write(0x4000, 0x5a);
        memory.write(0x57ff, 0xc3);

        assert_eq!(memory.read_screen(0x4000), 0x5a);
        assert_eq!(memory.read_screen(0x57ff), 0xc3);
    }

    #[test]
    fn spectrum16k_contention_only_in_ram_bank() {
        let memory = Spectrum16kMemory::new();
        assert!(!memory.is_contended(0x3fff));
        assert!(memory.is_contended(0x4000));
        assert!(memory.is_contended(0x7fff));
        assert!(!memory.is_contended(0x8000));
        assert!(!memory.is_contended(0xffff));
    }

    #[test]
    fn spectrum16k_rom_loader_requires_exact_16k_image() {
        let mut memory = Spectrum16kMemory::new();
        let error = memory
            .load_rom_bytes(&[0u8; 42])
            .expect_err("42-byte image should be rejected");

        assert_eq!(error, RomImageError::WrongSize { actual: 42 });
    }

    #[test]
    fn spectrum16k_rom_loader_accepts_full_image() {
        let mut memory = Spectrum16kMemory::new();
        let rom = [0x7e; ROM_BYTES_48K];

        memory
            .load_rom_bytes(&rom)
            .expect("16 KiB image should load");

        assert_eq!(memory.read(0x0000), 0x7e);
        assert_eq!(memory.read(0x3fff), 0x7e);
    }
}
