//! Spectrum memory subsystem.
//!
//! Memory layout varies by model. The trait abstracts banking/contention
//! differences so the bus doesn't need to know which model is active.

#![allow(clippy::cast_possible_truncation)] // Intentional: u16 addresses index into arrays.
#![allow(clippy::large_stack_arrays)] // Intentional: 48K RAM is the full usable address space.

/// Memory interface for all Spectrum variants.
///
/// Implementations handle ROM/RAM layout, banking (128K+), and contention
/// page identification. The ULA uses `peek()` to read VRAM without triggering
/// side effects or contention.
pub trait SpectrumMemory {
    /// Read a byte from the given address (may have side effects in banked models).
    fn read(&self, addr: u16) -> u8;

    /// Write a byte to the given address. ROM writes are silently ignored.
    fn write(&mut self, addr: u16, val: u8);

    /// Read a byte without side effects (used by ULA for screen fetches).
    fn peek(&self, addr: u16) -> u8;

    /// Is this address in contended RAM?
    ///
    /// For 48K: $4000-$7FFF is contended.
    /// For 128K: banks 1, 3, 5, 7 are contended ($4000 always maps to bank 5).
    fn contended_page(&self, addr: u16) -> bool;
}

/// 48K Spectrum memory: 16K ROM + 48K RAM.
///
/// Layout:
/// - $0000-$3FFF: ROM (writes ignored)
/// - $4000-$7FFF: Contended RAM (shared with ULA)
/// - $8000-$FFFF: Uncontended RAM
pub struct Memory48K {
    rom: [u8; 0x4000],
    ram: [u8; 0xC000],
}

impl Memory48K {
    /// Create a new 48K memory with the given ROM data.
    ///
    /// # Panics
    ///
    /// Panics if `rom` is not exactly 16,384 bytes.
    #[must_use]
    pub fn new(rom: &[u8]) -> Self {
        assert!(
            rom.len() == 0x4000,
            "48K ROM must be exactly 16384 bytes, got {}",
            rom.len()
        );
        let mut memory = Self {
            rom: [0; 0x4000],
            ram: [0; 0xC000],
        };
        memory.rom.copy_from_slice(rom);
        memory
    }

    /// Direct RAM access for snapshot loading. Offset 0 = address $4000.
    pub fn load_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.ram.len());
        self.ram[..len].copy_from_slice(&data[..len]);
    }

    /// Direct RAM read for observation. Offset 0 = address $4000.
    #[must_use]
    pub fn ram_slice(&self, offset: usize, len: usize) -> &[u8] {
        &self.ram[offset..offset + len]
    }
}

impl SpectrumMemory for Memory48K {
    fn read(&self, addr: u16) -> u8 {
        let addr = addr as usize;
        if addr < 0x4000 {
            self.rom[addr]
        } else {
            self.ram[addr - 0x4000]
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        let addr = addr as usize;
        if addr >= 0x4000 {
            self.ram[addr - 0x4000] = val;
        }
        // ROM writes silently ignored
    }

    fn peek(&self, addr: u16) -> u8 {
        self.read(addr)
    }

    fn contended_page(&self, addr: u16) -> bool {
        // $4000-$7FFF is contended (the ULA shares this bus)
        (0x4000..0x8000).contains(&addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x4000];
        rom[0] = 0xF3; // DI
        rom[1] = 0xAF; // XOR A
        rom[0x3FFF] = 0x42;
        rom
    }

    #[test]
    fn read_rom() {
        let mem = Memory48K::new(&make_rom());
        assert_eq!(mem.read(0x0000), 0xF3);
        assert_eq!(mem.read(0x0001), 0xAF);
        assert_eq!(mem.read(0x3FFF), 0x42);
    }

    #[test]
    fn rom_writes_ignored() {
        let mut mem = Memory48K::new(&make_rom());
        mem.write(0x0000, 0x00);
        assert_eq!(mem.read(0x0000), 0xF3);
    }

    #[test]
    fn ram_read_write() {
        let mut mem = Memory48K::new(&make_rom());
        mem.write(0x4000, 0xAB);
        assert_eq!(mem.read(0x4000), 0xAB);
        mem.write(0xFFFF, 0xCD);
        assert_eq!(mem.read(0xFFFF), 0xCD);
    }

    #[test]
    fn contended_page_48k() {
        let mem = Memory48K::new(&make_rom());
        assert!(!mem.contended_page(0x0000)); // ROM
        assert!(!mem.contended_page(0x3FFF)); // ROM
        assert!(mem.contended_page(0x4000)); // Contended RAM start
        assert!(mem.contended_page(0x7FFF)); // Contended RAM end
        assert!(!mem.contended_page(0x8000)); // Uncontended RAM
        assert!(!mem.contended_page(0xFFFF)); // Uncontended RAM
    }

    #[test]
    fn peek_same_as_read() {
        let mut mem = Memory48K::new(&make_rom());
        mem.write(0x5000, 0x77);
        assert_eq!(mem.peek(0x5000), mem.read(0x5000));
    }

    #[test]
    fn load_ram() {
        let mut mem = Memory48K::new(&make_rom());
        let data = [0x11, 0x22, 0x33];
        mem.load_ram(&data);
        assert_eq!(mem.read(0x4000), 0x11);
        assert_eq!(mem.read(0x4001), 0x22);
        assert_eq!(mem.read(0x4002), 0x33);
    }

    #[test]
    #[should_panic(expected = "48K ROM must be exactly 16384 bytes")]
    fn wrong_rom_size_panics() {
        let _ = Memory48K::new(&[0; 1024]);
    }
}
