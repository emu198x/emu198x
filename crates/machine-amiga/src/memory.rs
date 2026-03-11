//! Memory management for the Amiga Rock.

pub const CHIP_RAM_BASE: u32 = 0x000000;
pub const CIA_A_BASE: u32 = 0xBFE001;
pub const CIA_B_BASE: u32 = 0xBFD000;
pub const CUSTOM_REGS_BASE: u32 = 0xDFF000;
pub const ROM_BASE: u32 = 0xF80000;

#[derive(Clone)]
pub struct Memory {
    pub chip_ram: Vec<u8>,
    pub chip_ram_mask: u32,
    pub kickstart: Vec<u8>,
    pub kickstart_mask: u32,
    pub overlay: bool,
    pub slow_ram: Vec<u8>,
    pub slow_ram_mask: u32,
    /// Motherboard fast RAM (RAMSEY-controlled, A3000/A4000).
    pub fast_ram: Vec<u8>,
    pub fast_ram_mask: u32,
    /// Base address of fast RAM (e.g. $07E00000 for 2 MB below $08000000).
    pub fast_ram_base: u32,
}

impl Memory {
    pub fn new(chip_ram_size: usize, kickstart: Vec<u8>, slow_ram_size: usize) -> Self {
        Self::new_with_fast_ram(chip_ram_size, kickstart, slow_ram_size, 0, 0)
    }

    pub fn new_with_fast_ram(
        chip_ram_size: usize,
        kickstart: Vec<u8>,
        slow_ram_size: usize,
        fast_ram_size: usize,
        fast_ram_base: u32,
    ) -> Self {
        let ks_len = kickstart.len();
        let slow_ram_mask = if slow_ram_size > 0 {
            (slow_ram_size as u32).wrapping_sub(1)
        } else {
            0
        };
        let fast_ram_mask = if fast_ram_size > 0 {
            (fast_ram_size as u32).wrapping_sub(1)
        } else {
            0
        };
        Self {
            chip_ram: vec![0; chip_ram_size],
            chip_ram_mask: (chip_ram_size as u32).wrapping_sub(1),
            kickstart,
            kickstart_mask: (ks_len as u32).wrapping_sub(1),
            overlay: true,
            slow_ram: vec![0; slow_ram_size],
            slow_ram_mask,
            fast_ram: vec![0; fast_ram_size],
            fast_ram_mask,
            fast_ram_base,
        }
    }

    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFFFFFF;

        if self.overlay && addr < 0x200000 {
            return self.kickstart[(addr & self.kickstart_mask) as usize];
        }

        // Agnus wraps chip RAM addresses to the installed size.
        if addr < 0x20_0000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize]
        } else if (0xC0_0000..0xE0_0000).contains(&addr) && !self.slow_ram.is_empty() {
            let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
            self.slow_ram[offset as usize]
        } else if addr >= ROM_BASE {
            self.kickstart[(addr & self.kickstart_mask) as usize]
        } else {
            // Non-existing memory returns 0, matching WinUAE's
            // NONEXISTINGDATA behaviour. On real hardware the data
            // bus floats; returning 0 is the pragmatic choice that
            // makes KS 1.2+ expansion probes work correctly.
            0x00
        }
    }

    pub fn read_chip_byte(&self, addr: u32) -> u8 {
        self.chip_ram[(addr & self.chip_ram_mask) as usize]
    }

    pub fn write_byte(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xFFFFFF;
        if addr < 0x20_0000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize] = val;
        } else if (0xC0_0000..0xE0_0000).contains(&addr) && !self.slow_ram.is_empty() {
            let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
            self.slow_ram[offset as usize] = val;
        }
        // Everything else (ROM, unmapped space) silently drops writes.
    }

    /// Write a byte using a full 32-bit address, covering fast RAM.
    ///
    /// Used by DMA controllers (SDMAC) that can target 32-bit addresses
    /// beyond the 24-bit chip/slow RAM space.
    pub fn write_byte_32(&mut self, addr: u32, val: u8) {
        if !self.fast_ram.is_empty() {
            let end = self.fast_ram_base.wrapping_add(self.fast_ram.len() as u32);
            if addr >= self.fast_ram_base && addr < end {
                let offset = (addr - self.fast_ram_base) & self.fast_ram_mask;
                self.fast_ram[offset as usize] = val;
                return;
            }
        }
        self.write_byte(addr, val);
    }

    /// Read a byte using a full 32-bit address, covering fast RAM.
    pub fn read_byte_32(&self, addr: u32) -> u8 {
        if !self.fast_ram.is_empty() {
            let end = self.fast_ram_base.wrapping_add(self.fast_ram.len() as u32);
            if addr >= self.fast_ram_base && addr < end {
                let offset = (addr - self.fast_ram_base) & self.fast_ram_mask;
                return self.fast_ram[offset as usize];
            }
        }
        self.read_byte(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ks() -> Vec<u8> {
        vec![0u8; 256 * 1024]
    }

    #[test]
    fn chip_ram_aliasing() {
        let mut mem = Memory::new(512 * 1024, test_ks(), 0);
        mem.overlay = false;
        mem.write_byte(0x001000, 0xAB);
        assert_eq!(mem.read_byte(0x001000), 0xAB);
        mem.write_byte(0x080000, 0xCD);
        assert_eq!(mem.read_byte(0x000000), 0xCD, "should alias to $00000");
        mem.write_byte(0x100000, 0xEF);
        assert_eq!(mem.read_byte(0x000000), 0xEF, "should alias to $00000");
    }

    #[test]
    fn unmapped_expansion_returns_zero() {
        let mem = Memory::new(512 * 1024, test_ks(), 0);
        // No slow RAM: expansion space returns 0 (not $FF)
        assert_eq!(mem.read_byte(0xC0_0000), 0x00);
        assert_eq!(mem.read_byte(0xDB_FFFF), 0x00);
    }

    #[test]
    fn unmapped_other_returns_zero() {
        let mem = Memory::new(512 * 1024, test_ks(), 0);
        // Other unmapped ranges also return 0
        assert_eq!(mem.read_byte(0x20_0000), 0x00);
        assert_eq!(mem.read_byte(0xA0_0000), 0x00);
    }

    #[test]
    fn slow_ram_read_write_roundtrip() {
        let mut mem = Memory::new(512 * 1024, test_ks(), 512 * 1024);
        mem.overlay = false;
        mem.write_byte(0xC0_0000, 0x42);
        mem.write_byte(0xC0_0001, 0xAB);
        assert_eq!(mem.read_byte(0xC0_0000), 0x42);
        assert_eq!(mem.read_byte(0xC0_0001), 0xAB);
    }

    #[test]
    fn slow_ram_address_wrapping() {
        let mut mem = Memory::new(512 * 1024, test_ks(), 512 * 1024);
        mem.overlay = false;
        mem.write_byte(0xC0_0000, 0xEE);
        assert_eq!(
            mem.read_byte(0xC8_0000),
            0xEE,
            "should wrap at 512K boundary"
        );
    }
}
