//! Amiga memory subsystem — chip RAM, Kickstart ROM, slow RAM, overlay.
//!
//! Ported from `~/Projects/Emu198x-archive/crates/machine-commodore-amiga/src/memory.rs`.

pub const ROM_BASE: u32 = 0xF8_0000;

#[derive(Clone)]
pub struct Memory {
    pub chip_ram: Vec<u8>,
    pub chip_ram_mask: u32,
    pub kickstart: Vec<u8>,
    pub kickstart_mask: u32,
    /// When true, Kickstart ROM is mirrored at $000000 (reset overlay).
    pub overlay: bool,
    pub slow_ram: Vec<u8>,
    pub slow_ram_mask: u32,
}

impl Memory {
    pub fn new(chip_ram_size: usize, kickstart: Vec<u8>, slow_ram_size: usize) -> Self {
        let ks_len = kickstart.len();
        let slow_ram_mask = if slow_ram_size > 0 {
            (slow_ram_size as u32).wrapping_sub(1)
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
        }
    }

    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;

        if self.overlay && addr < 0x20_0000 {
            return self.kickstart[(addr & self.kickstart_mask) as usize];
        }

        // Chip RAM: Agnus wraps addresses with the chip RAM mask.
        // OCS Agnus only has 19 address lines, so $080000 aliases to $0
        // on a 512K system — matching real hardware.
        if addr < 0x20_0000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize]
        } else if (0xC0_0000..0xE0_0000).contains(&addr) && !self.slow_ram.is_empty() {
            let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
            self.slow_ram[offset as usize]
        } else if addr >= ROM_BASE {
            self.kickstart[(addr & self.kickstart_mask) as usize]
        } else {
            0x00
        }
    }

    pub fn read_word(&self, addr: u32) -> u16 {
        let hi = self.read_byte(addr);
        let lo = self.read_byte(addr | 1);
        (u16::from(hi) << 8) | u16::from(lo)
    }

    /// Read from chip RAM only (for DMA — Agnus can only see chip RAM).
    pub fn read_chip_byte(&self, addr: u32) -> u8 {
        self.chip_ram[(addr & self.chip_ram_mask) as usize]
    }

    pub fn write_byte(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xFF_FFFF;
        if addr < 0x20_0000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize] = val;
        } else if (0xC0_0000..0xE0_0000).contains(&addr) && !self.slow_ram.is_empty() {
            let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
            self.slow_ram[offset as usize] = val;
        }
        // ROM and unmapped space silently drop writes.
    }

    pub fn write_word(&mut self, addr: u32, val: u16) {
        self.write_byte(addr, (val >> 8) as u8);
        self.write_byte(addr | 1, val as u8);
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
        // OCS Agnus wraps: $080000 aliases to $0
        mem.write_byte(0x080000, 0xCD);
        assert_eq!(mem.read_byte(0x000000), 0xCD);
    }

    #[test]
    fn overlay_maps_rom_at_zero() {
        let mut ks = vec![0u8; 256 * 1024];
        ks[0] = 0x11;
        ks[1] = 0x22;
        let mem = Memory::new(512 * 1024, ks, 0);
        assert!(mem.overlay);
        assert_eq!(mem.read_byte(0x000000), 0x11);
        assert_eq!(mem.read_byte(0x000001), 0x22);
    }

    #[test]
    fn overlay_off_exposes_chip_ram() {
        let ks = vec![0xFFu8; 256 * 1024];
        let mut mem = Memory::new(512 * 1024, ks, 0);
        mem.overlay = false;
        mem.write_byte(0x000000, 0xAA);
        assert_eq!(mem.read_byte(0x000000), 0xAA);
    }

    #[test]
    fn rom_readable_at_f80000() {
        let mut ks = vec![0u8; 256 * 1024];
        ks[0] = 0xDE;
        ks[1] = 0xAD;
        let mut mem = Memory::new(512 * 1024, ks, 0);
        mem.overlay = false;
        assert_eq!(mem.read_byte(0xF8_0000), 0xDE);
        assert_eq!(mem.read_byte(0xF8_0001), 0xAD);
    }

    #[test]
    fn slow_ram_accessible() {
        let mut mem = Memory::new(512 * 1024, test_ks(), 512 * 1024);
        mem.overlay = false;
        mem.write_byte(0xC0_0000, 0x42);
        assert_eq!(mem.read_byte(0xC0_0000), 0x42);
    }

    #[test]
    fn unmapped_reads_zero() {
        let mut mem = Memory::new(512 * 1024, test_ks(), 0);
        mem.overlay = false;
        assert_eq!(mem.read_byte(0xA0_0000), 0x00);
    }
}
