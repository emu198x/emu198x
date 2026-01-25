//! C128 MMU (Memory Management Unit) emulation.
//!
//! The MMU controls memory banking for the C128, providing:
//! - 128K RAM in two 64K banks
//! - ROM/RAM banking configuration
//! - Zero page and stack relocation
//! - Common RAM areas (shared between banks)
//! - CPU selection (6502/Z80)
//!
//! # Registers
//!
//! | Address | Register | Description |
//! |---------|----------|-------------|
//! | $D500   | CR       | Configuration register |
//! | $D501   | PCRA     | Pre-config A (loaded on /GAME low) |
//! | $D502   | PCRB     | Pre-config B (loaded on /EXROM low) |
//! | $D503   | PCRC     | Pre-config C (loaded on /GAME+/EXROM low) |
//! | $D504   | PCRD     | Pre-config D (loaded on MMU reset) |
//! | $D505   | MCR      | Mode configuration register |
//! | $D506   | RCR      | RAM configuration register |
//! | $D507   | P0L      | Page 0 pointer low |
//! | $D508   | P0H      | Page 0 pointer high |
//! | $D509   | P1L      | Page 1 pointer low |
//! | $D50A   | P1H      | Page 1 pointer high |
//! | $D50B   | VER      | MMU version |
//!
//! # Configuration Register (CR) bits
//!
//! | Bit | Name   | Description |
//! |-----|--------|-------------|
//! | 0   | IO     | I/O block select (0=I/O, 1=RAM/ROM) |
//! | 1   | LORAM  | Low RAM ($4000-$7FFF): 0=BASIC, 1=RAM |
//! | 2   | HIRAM  | High RAM ($8000-$BFFF): 0=BASIC/ML, 1=RAM |
//! | 3   | MID    | Mid RAM ($C000-$CFFF): 0=screen editor, 1=RAM |
//! | 4   | ROMSEL | ROM set select |
//! | 5   | BANK0  | RAM bank bit 0 |
//! | 6   | BANK1  | RAM bank bit 1 (always 0 on 128K) |
//! | 7   | CPU    | CPU select (0=8502, 1=Z80) |

/// C128 MMU state.
#[derive(Clone)]
pub struct Mmu {
    /// Configuration register ($D500)
    pub cr: u8,
    /// Pre-configuration register A ($D501)
    pub pcra: u8,
    /// Pre-configuration register B ($D502)
    pub pcrb: u8,
    /// Pre-configuration register C ($D503)
    pub pcrc: u8,
    /// Pre-configuration register D ($D504)
    pub pcrd: u8,
    /// Mode configuration register ($D505)
    pub mcr: u8,
    /// RAM configuration register ($D506)
    pub rcr: u8,
    /// Page 0 pointer low ($D507)
    pub p0l: u8,
    /// Page 0 pointer high ($D508)
    pub p0h: u8,
    /// Page 1 pointer low ($D509)
    pub p1l: u8,
    /// Page 1 pointer high ($D50A)
    pub p1h: u8,
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}

impl Mmu {
    /// Create a new MMU in reset state.
    pub fn new() -> Self {
        Self {
            cr: 0x00, // All ROMs visible, 8502 mode
            pcra: 0x00,
            pcrb: 0x00,
            pcrc: 0x00,
            pcrd: 0x00,
            mcr: 0x00, // 40-column mode
            rcr: 0x00, // No common RAM
            p0l: 0x00,
            p0h: 0x00, // Zero page at $0000
            p1l: 0x01,
            p1h: 0x00, // Stack at $0100
        }
    }

    /// Reset the MMU.
    pub fn reset(&mut self) {
        // Load CR from PCRD on reset
        self.cr = self.pcrd;
        self.mcr = 0x00;
        self.rcr = 0x00;
        self.p0l = 0x00;
        self.p0h = 0x00;
        self.p1l = 0x01;
        self.p1h = 0x00;
    }

    /// Read from MMU register.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xD500 => self.cr,
            0xD501 => self.pcra,
            0xD502 => self.pcrb,
            0xD503 => self.pcrc,
            0xD504 => self.pcrd,
            0xD505 => self.mcr,
            0xD506 => self.rcr,
            0xD507 => self.p0l,
            0xD508 => self.p0h,
            0xD509 => self.p1l,
            0xD50A => self.p1h,
            0xD50B => 0x00, // MMU version (0 for original C128)
            _ => 0xFF,
        }
    }

    /// Write to MMU register.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0xD500 => self.cr = value,
            0xD501 => self.pcra = value,
            0xD502 => self.pcrb = value,
            0xD503 => self.pcrc = value,
            0xD504 => self.pcrd = value,
            0xD505 => self.mcr = value,
            0xD506 => self.rcr = value,
            0xD507 => self.p0l = value,
            0xD508 => self.p0h = value,
            0xD509 => self.p1l = value,
            0xD50A => self.p1h = value,
            0xD50B => {} // Version register is read-only
            _ => {}
        }
    }

    /// Check if Z80 CPU is selected.
    pub fn is_z80_mode(&self) -> bool {
        self.cr & 0x80 != 0
    }

    /// Check if 8502 CPU is selected.
    pub fn is_8502_mode(&self) -> bool {
        self.cr & 0x80 == 0
    }

    /// Get the currently selected RAM bank (0 or 1).
    pub fn ram_bank(&self) -> u8 {
        (self.cr >> 5) & 0x03
    }

    /// Check if I/O is visible (vs RAM/ROM at $D000-$DFFF).
    pub fn io_visible(&self) -> bool {
        self.cr & 0x01 == 0
    }

    /// Check if BASIC ROM is visible at $4000-$7FFF.
    pub fn basic_lo_visible(&self) -> bool {
        self.cr & 0x02 == 0
    }

    /// Check if BASIC/ML ROM is visible at $8000-$BFFF.
    pub fn basic_hi_visible(&self) -> bool {
        self.cr & 0x04 == 0
    }

    /// Check if screen editor ROM is visible at $C000-$CFFF.
    pub fn screen_editor_visible(&self) -> bool {
        self.cr & 0x08 == 0
    }

    /// Check if KERNAL ROM is visible at $E000-$FFFF.
    pub fn kernal_visible(&self) -> bool {
        // KERNAL visible when IO bit is 0 (I/O visible implies KERNAL visible)
        self.cr & 0x01 == 0
    }

    /// Get the ROM set selection (0 or 1).
    pub fn rom_set(&self) -> u8 {
        (self.cr >> 4) & 0x01
    }

    /// Check if 40-column mode is selected.
    pub fn is_40_column(&self) -> bool {
        self.mcr & 0x80 == 0
    }

    /// Check if 80-column mode is selected.
    pub fn is_80_column(&self) -> bool {
        self.mcr & 0x80 != 0
    }

    /// Get the VIC bank (0-3) from MCR.
    pub fn vic_bank(&self) -> u8 {
        (self.mcr >> 4) & 0x03
    }

    /// Get common RAM size in bytes (bottom area).
    pub fn common_ram_bottom_size(&self) -> u16 {
        if self.rcr & 0x04 == 0 {
            return 0;
        }
        match self.rcr & 0x03 {
            0 => 0x400,  // 1K
            1 => 0x1000, // 4K
            2 => 0x2000, // 8K
            3 => 0x4000, // 16K
            _ => unreachable!(),
        }
    }

    /// Get common RAM size in bytes (top area).
    pub fn common_ram_top_size(&self) -> u16 {
        if self.rcr & 0x40 == 0 {
            return 0;
        }
        match (self.rcr >> 4) & 0x03 {
            0 => 0x400,  // 1K (top 1K: $FC00-$FFFF)
            1 => 0x1000, // 4K (top 4K: $F000-$FFFF)
            2 => 0x2000, // 8K (top 8K: $E000-$FFFF)
            3 => 0x4000, // 16K (top 16K: $C000-$FFFF)
            _ => unreachable!(),
        }
    }

    /// Get the physical zero page address.
    pub fn zero_page_addr(&self) -> u32 {
        let page = ((self.p0h as u32) << 8) | (self.p0l as u32);
        // Bank comes from bits 0-1 of P0H
        let bank = (self.p0h & 0x01) as u32;
        (bank << 16) | (page << 8)
    }

    /// Get the physical stack page address.
    pub fn stack_page_addr(&self) -> u32 {
        let page = ((self.p1h as u32) << 8) | (self.p1l as u32);
        // Bank comes from bit 0 of P1H
        let bank = (self.p1h & 0x01) as u32;
        (bank << 16) | (page << 8)
    }

    /// Translate a CPU address to physical memory address.
    /// Returns (physical_address, is_ram, is_io).
    pub fn translate(&self, addr: u16) -> (u32, bool, bool) {
        let bank = self.ram_bank() as u32;

        // Check for zero page relocation
        if addr < 0x100 {
            let phys = self.zero_page_addr() | (addr as u32);
            return (phys, true, false);
        }

        // Check for stack page relocation
        if addr >= 0x100 && addr < 0x200 {
            let phys = self.stack_page_addr() | ((addr & 0xFF) as u32);
            return (phys, true, false);
        }

        // Check for common RAM (bottom)
        let common_bottom = self.common_ram_bottom_size();
        if common_bottom > 0 && addr < common_bottom {
            // Common RAM always comes from bank 0
            return (addr as u32, true, false);
        }

        // Check for common RAM (top)
        let common_top = self.common_ram_top_size();
        if common_top > 0 && addr >= (0x10000 - common_top as u32) as u16 {
            // Common RAM always comes from bank 0
            return (addr as u32, true, false);
        }

        // I/O area at $D000-$DFFF
        if addr >= 0xD000 && addr < 0xE000 {
            if self.io_visible() {
                return ((bank << 16) | (addr as u32), false, true);
            }
            // Character ROM or RAM
            return ((bank << 16) | (addr as u32), true, false);
        }

        // BASIC ROM at $4000-$7FFF
        if addr >= 0x4000 && addr < 0x8000 {
            if self.basic_lo_visible() {
                return (addr as u32, false, false); // ROM, not RAM
            }
            return ((bank << 16) | (addr as u32), true, false);
        }

        // BASIC/ML ROM at $8000-$BFFF
        if addr >= 0x8000 && addr < 0xC000 {
            if self.basic_hi_visible() {
                return (addr as u32, false, false); // ROM
            }
            return ((bank << 16) | (addr as u32), true, false);
        }

        // Screen editor ROM at $C000-$CFFF
        if addr >= 0xC000 && addr < 0xD000 {
            if self.screen_editor_visible() {
                return (addr as u32, false, false); // ROM
            }
            return ((bank << 16) | (addr as u32), true, false);
        }

        // KERNAL ROM at $E000-$FFFF
        if addr >= 0xE000 {
            if self.kernal_visible() {
                return (addr as u32, false, false); // ROM
            }
            return ((bank << 16) | (addr as u32), true, false);
        }

        // Default: RAM in current bank
        ((bank << 16) | (addr as u32), true, false)
    }

    /// Load CR from pre-configuration register based on GAME/EXROM lines.
    pub fn load_preconfig(&mut self, game: bool, exrom: bool) {
        self.cr = match (game, exrom) {
            (false, true) => self.pcra,  // /GAME low only
            (true, false) => self.pcrb,  // /EXROM low only
            (false, false) => self.pcrc, // Both low
            (true, true) => self.cr,     // No change
        };
    }

    /// Check if C64 mode is active.
    /// C64 mode is when the MMU is configured to behave like a C64.
    pub fn is_c64_mode(&self) -> bool {
        // C64 mode typically has specific CR settings
        // This is a simplified check - actual detection is more complex
        self.cr == 0x3F || self.cr == 0x7F
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let mmu = Mmu::new();
        assert!(mmu.is_8502_mode());
        assert!(!mmu.is_z80_mode());
        assert_eq!(mmu.ram_bank(), 0);
    }

    #[test]
    fn test_cpu_mode() {
        let mut mmu = Mmu::new();
        mmu.cr = 0x80;
        assert!(mmu.is_z80_mode());
        assert!(!mmu.is_8502_mode());
    }

    #[test]
    fn test_ram_bank() {
        let mut mmu = Mmu::new();
        mmu.cr = 0x20;
        assert_eq!(mmu.ram_bank(), 1);
        mmu.cr = 0x40;
        assert_eq!(mmu.ram_bank(), 2);
    }

    #[test]
    fn test_translate_zero_page() {
        let mmu = Mmu::new();
        let (phys, is_ram, is_io) = mmu.translate(0x50);
        assert_eq!(phys, 0x50);
        assert!(is_ram);
        assert!(!is_io);
    }

    #[test]
    fn test_io_visibility() {
        let mut mmu = Mmu::new();
        mmu.cr = 0x00;
        assert!(mmu.io_visible());
        mmu.cr = 0x01;
        assert!(!mmu.io_visible());
    }
}
