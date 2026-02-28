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

    /// Read VRAM for ULA screen fetches. Defaults to `peek()`.
    ///
    /// On 128K models, the shadow screen (bank 7) can be selected via bit 3
    /// of the $7FFD register. This method reads from the active screen bank.
    fn vram_peek(&self, addr: u16) -> u8 {
        self.peek(addr)
    }

    /// Is this address in contended RAM?
    ///
    /// For 48K: $4000-$7FFF is contended.
    /// For 128K: banks 1, 3, 5, 7 are contended ($4000 always maps to bank 5).
    fn contended_page(&self, addr: u16) -> bool;

    /// Write the bank register ($7FFD). No-op on 48K.
    fn write_bank_register(&mut self, _value: u8) {}

    /// Write the +3 banking register ($1FFD). No-op on non-+3 models.
    fn write_plus3_register(&mut self, _value: u8) {}

    /// Whether the disk motor is on (controlled by $1FFD bit 3). Always false on non-+3.
    fn disk_motor(&self) -> bool {
        false
    }

    /// Which RAM bank holds the current screen? Always 5 on 48K.
    fn screen_bank(&self) -> u8 {
        5
    }
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

/// 128K Spectrum memory: 2×16K ROM + 8×16K RAM with bank switching.
///
/// Layout:
/// - $0000-$3FFF: ROM bank 0 or 1 (bit 4 of $7FFD)
/// - $4000-$7FFF: Always RAM bank 5 (contended)
/// - $8000-$BFFF: Always RAM bank 2
/// - $C000-$FFFF: Switchable RAM bank 0-7 (bits 0-2 of $7FFD)
///
/// Bit 3 of $7FFD selects the shadow screen (bank 7 instead of bank 5).
/// Bit 5 of $7FFD locks the bank register (cannot be changed until reset).
pub struct Memory128K {
    rom: [[u8; 0x4000]; 2],
    ram: [Box<[u8; 0x4000]>; 8],
    /// $7FFD register value.
    bank_reg: u8,
    /// Once bit 5 is set, further writes to $7FFD are ignored.
    locked: bool,
}

impl Memory128K {
    /// Create a new 128K memory with the given 32K ROM data.
    ///
    /// The ROM is split into two 16K banks: ROM 0 (128K editor) at the
    /// start and ROM 1 (48K BASIC) at offset $4000.
    ///
    /// # Panics
    ///
    /// Panics if `rom` is not exactly 32,768 bytes.
    #[must_use]
    pub fn new(rom: &[u8]) -> Self {
        assert!(
            rom.len() == 0x8000,
            "128K ROM must be exactly 32768 bytes, got {}",
            rom.len()
        );
        let mut memory = Self {
            rom: [[0; 0x4000]; 2],
            ram: std::array::from_fn(|_| Box::new([0u8; 0x4000])),
            bank_reg: 0,
            locked: false,
        };
        memory.rom[0].copy_from_slice(&rom[..0x4000]);
        memory.rom[1].copy_from_slice(&rom[0x4000..]);
        memory
    }

    /// Selected ROM bank (0 or 1).
    fn rom_bank(&self) -> usize {
        ((self.bank_reg >> 4) & 1) as usize
    }

    /// Selected RAM bank at $C000 (0-7).
    fn page_bank(&self) -> usize {
        (self.bank_reg & 0x07) as usize
    }

    /// Direct RAM access for snapshot loading.
    pub fn load_ram_bank(&mut self, bank: usize, data: &[u8]) {
        let len = data.len().min(0x4000);
        self.ram[bank][..len].copy_from_slice(&data[..len]);
    }

    /// Direct RAM read for observation.
    #[must_use]
    pub fn ram_bank_slice(&self, bank: usize, offset: usize, len: usize) -> &[u8] {
        &self.ram[bank][offset..offset + len]
    }
}

impl SpectrumMemory for Memory128K {
    fn read(&self, addr: u16) -> u8 {
        let a = addr as usize;
        match a {
            0x0000..0x4000 => self.rom[self.rom_bank()][a],
            0x4000..0x8000 => self.ram[5][a - 0x4000],
            0x8000..0xC000 => self.ram[2][a - 0x8000],
            _ => self.ram[self.page_bank()][a - 0xC000],
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        let a = addr as usize;
        match a {
            0x0000..0x4000 => {} // ROM writes ignored
            0x4000..0x8000 => self.ram[5][a - 0x4000] = val,
            0x8000..0xC000 => self.ram[2][a - 0x8000] = val,
            _ => {
                let bank = self.page_bank();
                self.ram[bank][a - 0xC000] = val;
            }
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        self.read(addr)
    }

    fn vram_peek(&self, addr: u16) -> u8 {
        let a = addr as usize;
        if (0x4000..0x5B00).contains(&a) {
            let screen = self.screen_bank() as usize;
            self.ram[screen][a - 0x4000]
        } else {
            self.read(addr)
        }
    }

    fn contended_page(&self, addr: u16) -> bool {
        let a = addr as usize;
        match a {
            // $4000-$7FFF: always bank 5 (contended)
            0x4000..0x8000 => true,
            // $C000-$FFFF: contended if odd bank (1, 3, 5, 7)
            0xC000..=0xFFFF => self.page_bank() & 1 != 0,
            _ => false,
        }
    }

    fn write_bank_register(&mut self, value: u8) {
        if !self.locked {
            self.bank_reg = value;
            self.locked = value & 0x20 != 0;
        }
    }

    fn screen_bank(&self) -> u8 {
        if self.bank_reg & 0x08 != 0 { 7 } else { 5 }
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

    // --- 128K Memory tests ---

    fn make_128k_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0] = 0xAA; // ROM 0 first byte
        rom[0x4000] = 0xBB; // ROM 1 first byte
        rom
    }

    #[test]
    fn memory_128k_rom_switching() {
        let mut mem = Memory128K::new(&make_128k_rom());
        // Default: ROM 0 selected (bit 4 = 0)
        assert_eq!(mem.read(0x0000), 0xAA);

        // Switch to ROM 1 (bit 4 = 1)
        mem.write_bank_register(0x10);
        assert_eq!(mem.read(0x0000), 0xBB);

        // Back to ROM 0
        mem.write_bank_register(0x00);
        assert_eq!(mem.read(0x0000), 0xAA);
    }

    #[test]
    fn memory_128k_bank_switching() {
        let mut mem = Memory128K::new(&make_128k_rom());

        // Write to bank 0 (default at $C000)
        mem.write(0xC000, 0x11);
        assert_eq!(mem.read(0xC000), 0x11);

        // Switch to bank 3
        mem.write_bank_register(0x03);
        assert_eq!(mem.read(0xC000), 0x00); // Bank 3 is fresh
        mem.write(0xC000, 0x33);

        // Switch back to bank 0
        mem.write_bank_register(0x00);
        assert_eq!(mem.read(0xC000), 0x11); // Bank 0 data preserved
    }

    #[test]
    fn memory_128k_fixed_banks() {
        let mut mem = Memory128K::new(&make_128k_rom());

        // $4000-$7FFF is always bank 5
        mem.write(0x4000, 0x55);
        assert_eq!(mem.read(0x4000), 0x55);

        // $8000-$BFFF is always bank 2
        mem.write(0x8000, 0x22);
        assert_eq!(mem.read(0x8000), 0x22);

        // Bank switching doesn't affect $4000 or $8000
        mem.write_bank_register(0x07);
        assert_eq!(mem.read(0x4000), 0x55); // Still bank 5
        assert_eq!(mem.read(0x8000), 0x22); // Still bank 2
    }

    #[test]
    fn memory_128k_lock_bit() {
        let mut mem = Memory128K::new(&make_128k_rom());

        // Switch to bank 3
        mem.write_bank_register(0x03);
        mem.write(0xC000, 0x33);

        // Lock (bit 5 set)
        mem.write_bank_register(0x23);

        // Try to switch to bank 0 — should be ignored
        mem.write_bank_register(0x00);
        assert_eq!(mem.read(0xC000), 0x33, "Bank should still be 3 (locked)");
    }

    #[test]
    fn memory_128k_contended_pages() {
        let mut mem = Memory128K::new(&make_128k_rom());

        // $4000-$7FFF always contended (bank 5)
        assert!(mem.contended_page(0x4000));
        assert!(mem.contended_page(0x7FFF));

        // ROM not contended
        assert!(!mem.contended_page(0x0000));

        // $8000-$BFFF (bank 2) not contended (even bank)
        assert!(!mem.contended_page(0x8000));

        // Bank 0 at $C000: even, not contended
        mem.write_bank_register(0x00);
        assert!(!mem.contended_page(0xC000));

        // Bank 1 at $C000: odd, contended
        mem.write_bank_register(0x01);
        assert!(mem.contended_page(0xC000));

        // Bank 7 at $C000: odd, contended
        mem.write_bank_register(0x07);
        assert!(mem.contended_page(0xC000));
    }

    #[test]
    fn memory_128k_shadow_screen() {
        let mut mem = Memory128K::new(&make_128k_rom());

        // Write to bank 5 screen area (via $4000)
        mem.write(0x4000, 0x55);

        // Write to bank 7 (page it in at $C000, write, page it out)
        mem.write_bank_register(0x07);
        mem.ram[7][0] = 0x77; // Direct write to bank 7 offset 0 (= vram $4000)
        mem.write_bank_register(0x00);

        // Default screen: bank 5
        assert_eq!(mem.screen_bank(), 5);
        assert_eq!(mem.vram_peek(0x4000), 0x55);

        // Shadow screen: bank 7 (bit 3 set)
        mem.write_bank_register(0x08);
        assert_eq!(mem.screen_bank(), 7);
        assert_eq!(mem.vram_peek(0x4000), 0x77);
    }

    #[test]
    #[should_panic(expected = "128K ROM must be exactly 32768 bytes")]
    fn wrong_128k_rom_size_panics() {
        let _ = Memory128K::new(&[0; 1024]);
    }
}

/// +3 Spectrum memory: 4×16K ROM + 8×16K RAM with dual bank registers.
///
/// The +3 extends the 128K banking with an additional register at $1FFD
/// that controls ROM paging, special all-RAM modes, and the disk motor.
///
/// ## Normal mode ($1FFD bit 0 = 0)
///
/// ```text
/// $0000-$3FFF: ROM page (2-bit select: $1FFD bit 2 << 1 | $7FFD bit 4)
/// $4000-$7FFF: Always RAM bank 5 (contended)
/// $8000-$BFFF: Always RAM bank 2
/// $C000-$FFFF: Switchable RAM bank 0-7 ($7FFD bits 0-2)
/// ```
///
/// ## Special mode ($1FFD bit 0 = 1, all-RAM)
///
/// Four configurations selected by $1FFD bits 1-2:
/// ```text
/// Config 0: banks 0, 1, 2, 3
/// Config 1: banks 4, 5, 6, 7
/// Config 2: banks 4, 5, 6, 3
/// Config 3: banks 4, 7, 6, 3
/// ```
pub struct MemoryPlus3 {
    rom: [[u8; 0x4000]; 4],
    ram: [Box<[u8; 0x4000]>; 8],
    /// $7FFD register value.
    bank_7ffd: u8,
    /// $1FFD register value.
    bank_1ffd: u8,
    /// Once $7FFD bit 5 is set, both $7FFD and $1FFD writes are ignored.
    locked: bool,
}

/// Special mode RAM bank configurations, indexed by $1FFD bits 1-2.
/// Each entry is [slot0, slot1, slot2, slot3] = banks at $0000/$4000/$8000/$C000.
const SPECIAL_CONFIGS: [[usize; 4]; 4] = [
    [0, 1, 2, 3], // Config 0
    [4, 5, 6, 7], // Config 1
    [4, 5, 6, 3], // Config 2
    [4, 7, 6, 3], // Config 3
];

impl MemoryPlus3 {
    /// Create a new +3 memory with the given 64K ROM data.
    ///
    /// The ROM is split into four 16K pages:
    /// - Page 0 ($0000): Editor
    /// - Page 1 ($4000): Syntax
    /// - Page 2 ($8000): +3DOS
    /// - Page 3 ($C000): 48K BASIC
    ///
    /// # Panics
    ///
    /// Panics if `rom` is not exactly 65,536 bytes.
    #[must_use]
    pub fn new(rom: &[u8]) -> Self {
        assert!(
            rom.len() == 0x10000,
            "+3 ROM must be exactly 65536 bytes, got {}",
            rom.len()
        );
        let mut memory = Self {
            rom: [[0; 0x4000]; 4],
            ram: std::array::from_fn(|_| Box::new([0u8; 0x4000])),
            bank_7ffd: 0,
            bank_1ffd: 0,
            locked: false,
        };
        for i in 0..4 {
            memory.rom[i].copy_from_slice(&rom[i * 0x4000..(i + 1) * 0x4000]);
        }
        memory
    }

    /// Whether special mode (all-RAM) is active ($1FFD bit 0).
    fn special_mode(&self) -> bool {
        self.bank_1ffd & 0x01 != 0
    }

    /// Special mode configuration index ($1FFD bits 1-2).
    fn special_config(&self) -> usize {
        ((self.bank_1ffd >> 1) & 0x03) as usize
    }

    /// Selected ROM page in normal mode (2-bit: $1FFD bit 2, $7FFD bit 4).
    fn rom_page(&self) -> usize {
        let bit_high = (self.bank_1ffd >> 2) & 1;
        let bit_low = (self.bank_7ffd >> 4) & 1;
        ((bit_high << 1) | bit_low) as usize
    }

    /// Selected RAM bank at $C000 in normal mode ($7FFD bits 0-2).
    fn page_bank(&self) -> usize {
        (self.bank_7ffd & 0x07) as usize
    }

    /// Direct RAM access for snapshot loading.
    pub fn load_ram_bank(&mut self, bank: usize, data: &[u8]) {
        let len = data.len().min(0x4000);
        self.ram[bank][..len].copy_from_slice(&data[..len]);
    }

    /// Direct RAM read for observation.
    #[must_use]
    pub fn ram_bank_slice(&self, bank: usize, offset: usize, len: usize) -> &[u8] {
        &self.ram[bank][offset..offset + len]
    }
}

impl SpectrumMemory for MemoryPlus3 {
    fn read(&self, addr: u16) -> u8 {
        let a = addr as usize;
        if self.special_mode() {
            let cfg = SPECIAL_CONFIGS[self.special_config()];
            let slot = a >> 14;
            self.ram[cfg[slot]][a & 0x3FFF]
        } else {
            match a {
                0x0000..0x4000 => self.rom[self.rom_page()][a],
                0x4000..0x8000 => self.ram[5][a - 0x4000],
                0x8000..0xC000 => self.ram[2][a - 0x8000],
                _ => self.ram[self.page_bank()][a - 0xC000],
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        let a = addr as usize;
        if self.special_mode() {
            let cfg = SPECIAL_CONFIGS[self.special_config()];
            let slot = a >> 14;
            self.ram[cfg[slot]][a & 0x3FFF] = val;
        } else {
            match a {
                0x0000..0x4000 => {} // ROM writes ignored
                0x4000..0x8000 => self.ram[5][a - 0x4000] = val,
                0x8000..0xC000 => self.ram[2][a - 0x8000] = val,
                _ => {
                    let bank = self.page_bank();
                    self.ram[bank][a - 0xC000] = val;
                }
            }
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        self.read(addr)
    }

    fn vram_peek(&self, addr: u16) -> u8 {
        let a = addr as usize;
        if (0x4000..0x5B00).contains(&a) {
            let screen = self.screen_bank() as usize;
            self.ram[screen][a - 0x4000]
        } else {
            self.read(addr)
        }
    }

    fn contended_page(&self, addr: u16) -> bool {
        if self.special_mode() {
            // In special mode, contention applies to slots holding odd banks (1,3,5,7)
            let cfg = SPECIAL_CONFIGS[self.special_config()];
            let slot = (addr as usize) >> 14;
            cfg[slot] & 1 != 0
        } else {
            let a = addr as usize;
            match a {
                0x4000..0x8000 => true, // Always bank 5 (contended)
                0xC000..=0xFFFF => self.page_bank() & 1 != 0,
                _ => false,
            }
        }
    }

    fn write_bank_register(&mut self, value: u8) {
        if !self.locked {
            self.bank_7ffd = value;
            self.locked = value & 0x20 != 0;
        }
    }

    fn write_plus3_register(&mut self, value: u8) {
        if !self.locked {
            self.bank_1ffd = value;
        }
    }

    fn disk_motor(&self) -> bool {
        self.bank_1ffd & 0x08 != 0
    }

    fn screen_bank(&self) -> u8 {
        if self.bank_7ffd & 0x08 != 0 { 7 } else { 5 }
    }
}

#[cfg(test)]
mod tests_plus3 {
    use super::*;

    fn make_plus3_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        rom[0] = 0xAA;          // ROM page 0 first byte
        rom[0x4000] = 0xBB;     // ROM page 1 first byte
        rom[0x8000] = 0xCC;     // ROM page 2 first byte
        rom[0xC000] = 0xDD;     // ROM page 3 first byte
        rom
    }

    #[test]
    fn rom_paging_all_four_pages() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());

        // Default: ROM page 0 ($7FFD bit 4 = 0, $1FFD bit 2 = 0)
        assert_eq!(mem.read(0x0000), 0xAA);

        // ROM page 1: $7FFD bit 4 = 1, $1FFD bit 2 = 0
        mem.write_bank_register(0x10);
        assert_eq!(mem.read(0x0000), 0xBB);

        // ROM page 2: $7FFD bit 4 = 0, $1FFD bit 2 = 1
        mem.write_bank_register(0x00);
        mem.write_plus3_register(0x04);
        assert_eq!(mem.read(0x0000), 0xCC);

        // ROM page 3: $7FFD bit 4 = 1, $1FFD bit 2 = 1
        mem.write_bank_register(0x10);
        assert_eq!(mem.read(0x0000), 0xDD);
    }

    #[test]
    fn normal_mode_ram_banking() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());

        // Write to bank 0 (default at $C000)
        mem.write(0xC000, 0x11);
        assert_eq!(mem.read(0xC000), 0x11);

        // Switch to bank 3
        mem.write_bank_register(0x03);
        assert_eq!(mem.read(0xC000), 0x00);
        mem.write(0xC000, 0x33);

        // Back to bank 0
        mem.write_bank_register(0x00);
        assert_eq!(mem.read(0xC000), 0x11);
    }

    #[test]
    fn normal_mode_fixed_banks() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());
        mem.write(0x4000, 0x55);
        mem.write(0x8000, 0x22);

        // Bank switching doesn't affect $4000 or $8000
        mem.write_bank_register(0x07);
        assert_eq!(mem.read(0x4000), 0x55);
        assert_eq!(mem.read(0x8000), 0x22);
    }

    #[test]
    fn special_mode_config_0() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());

        // Pre-load banks with identifiable data
        mem.ram[0][0] = 0x00;
        mem.ram[1][0] = 0x11;
        mem.ram[2][0] = 0x22;
        mem.ram[3][0] = 0x33;

        // Enter special mode, config 0: banks 0,1,2,3
        mem.write_plus3_register(0x01); // bit 0 = 1, bits 1-2 = 00
        assert!(mem.special_mode());

        assert_eq!(mem.read(0x0000), 0x00);
        assert_eq!(mem.read(0x4000), 0x11);
        assert_eq!(mem.read(0x8000), 0x22);
        assert_eq!(mem.read(0xC000), 0x33);
    }

    #[test]
    fn special_mode_config_1() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());
        mem.ram[4][0] = 0x44;
        mem.ram[5][0] = 0x55;
        mem.ram[6][0] = 0x66;
        mem.ram[7][0] = 0x77;

        // Config 1: banks 4,5,6,7 — bits 1-2 = 01
        mem.write_plus3_register(0x03);
        assert_eq!(mem.read(0x0000), 0x44);
        assert_eq!(mem.read(0x4000), 0x55);
        assert_eq!(mem.read(0x8000), 0x66);
        assert_eq!(mem.read(0xC000), 0x77);
    }

    #[test]
    fn special_mode_config_3() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());
        mem.ram[3][0] = 0x33;
        mem.ram[4][0] = 0x44;
        mem.ram[6][0] = 0x66;
        mem.ram[7][0] = 0x77;

        // Config 3: banks 4,7,6,3 — bits 1-2 = 11
        mem.write_plus3_register(0x07);
        assert_eq!(mem.read(0x0000), 0x44);
        assert_eq!(mem.read(0x4000), 0x77);
        assert_eq!(mem.read(0x8000), 0x66);
        assert_eq!(mem.read(0xC000), 0x33);
    }

    #[test]
    fn special_mode_writes_to_ram() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());

        // In special mode, all addresses are RAM (writable)
        mem.write_plus3_register(0x01); // Config 0: 0,1,2,3
        mem.write(0x0000, 0xEE); // Writes to bank 0 (would be ROM in normal mode)
        assert_eq!(mem.ram[0][0], 0xEE);
    }

    #[test]
    fn lock_bit_prevents_both_registers() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());

        // Switch to bank 3 and ROM page 1
        mem.write_bank_register(0x13); // bank 3, ROM 1
        mem.write_plus3_register(0x04); // ROM page bit

        // Lock
        mem.write_bank_register(0x23); // bit 5 set

        // Attempt to change — should be ignored
        mem.write_bank_register(0x00);
        assert_eq!(mem.read(0xC000), 0x00); // Still bank 3
        mem.write(0xC000, 0x33);
        mem.write_bank_register(0x00); // Try again
        assert_eq!(mem.read(0xC000), 0x33); // Still bank 3

        // $1FFD also locked
        mem.write_plus3_register(0x00);
        assert_eq!(mem.bank_1ffd, 0x04); // Unchanged
    }

    #[test]
    fn disk_motor_bit() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());
        assert!(!mem.disk_motor());

        mem.write_plus3_register(0x08); // bit 3
        assert!(mem.disk_motor());

        mem.write_plus3_register(0x00);
        assert!(!mem.disk_motor());
    }

    #[test]
    fn contention_special_mode() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());

        // Config 1: banks 4,5,6,7
        mem.write_plus3_register(0x03);
        assert!(!mem.contended_page(0x0000)); // Bank 4 (even)
        assert!(mem.contended_page(0x4000));  // Bank 5 (odd)
        assert!(!mem.contended_page(0x8000)); // Bank 6 (even)
        assert!(mem.contended_page(0xC000));  // Bank 7 (odd)
    }

    #[test]
    fn shadow_screen() {
        let mut mem = MemoryPlus3::new(&make_plus3_rom());
        mem.write(0x4000, 0x55); // Bank 5

        mem.write_bank_register(0x07); // Page bank 7 at $C000
        mem.ram[7][0] = 0x77;
        mem.write_bank_register(0x00);

        assert_eq!(mem.screen_bank(), 5);
        assert_eq!(mem.vram_peek(0x4000), 0x55);

        mem.write_bank_register(0x08); // Shadow screen = bank 7
        assert_eq!(mem.screen_bank(), 7);
        assert_eq!(mem.vram_peek(0x4000), 0x77);
    }

    #[test]
    #[should_panic(expected = "+3 ROM must be exactly 65536 bytes")]
    fn wrong_plus3_rom_size_panics() {
        let _ = MemoryPlus3::new(&[0; 1024]);
    }
}
