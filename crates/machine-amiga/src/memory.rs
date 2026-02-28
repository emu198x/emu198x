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
}

impl Memory {
    pub fn new(chip_ram_size: usize, kickstart: Vec<u8>, slow_ram_size: usize) -> Self {
        let ks_len = kickstart.len();
        // Slow RAM at $C00000-$DFFFFF (max 2MB). Size must be power of 2.
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
            overlay: true, // Amiga starts with ROM overlay at $0
            slow_ram: vec![0; slow_ram_size],
            slow_ram_mask,
        }
    }

    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFFFFFF;

        if self.overlay && addr < 0x200000 {
            // Overlay maps ROM to $0
            return self.kickstart[(addr & self.kickstart_mask) as usize];
        }

        if addr <= self.chip_ram_mask {
            // Within installed chip RAM
            self.chip_ram[addr as usize]
        } else if (0xC0_0000..0xE0_0000).contains(&addr) && !self.slow_ram.is_empty() {
            let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
            self.slow_ram[offset as usize]
        } else if addr >= ROM_BASE {
            self.kickstart[(addr & self.kickstart_mask) as usize]
        } else {
            0xFF // Open bus / unmapped
        }
    }

    pub fn read_chip_byte(&self, addr: u32) -> u8 {
        self.chip_ram[(addr & self.chip_ram_mask) as usize]
    }

    pub fn write_byte(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xFFFFFF;
        // Only addresses within installed chip RAM respond.
        // Agnus decodes A0-A18 for 512KB, A0-A19 for 1MB, etc.
        // Addresses above the installed size are unmapped (no DTACK).
        if addr <= self.chip_ram_mask {
            self.chip_ram[addr as usize] = val;
        } else if (0xC0_0000..0xE0_0000).contains(&addr) && !self.slow_ram.is_empty() {
            let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
            self.slow_ram[offset as usize] = val;
        }
        // ROM is read-only; addresses above chip/slow RAM are open bus.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ks() -> Vec<u8> {
        vec![0u8; 256 * 1024]
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
    fn slow_ram_unmapped_when_disabled() {
        let mem = Memory::new(512 * 1024, test_ks(), 0);
        assert_eq!(mem.read_byte(0xC0_0000), 0xFF, "should be open bus");
    }

    #[test]
    fn slow_ram_address_wrapping() {
        let mut mem = Memory::new(512 * 1024, test_ks(), 512 * 1024);
        mem.overlay = false;
        // 512K = $80000, so $C80000 wraps to $C00000
        mem.write_byte(0xC0_0000, 0xEE);
        assert_eq!(mem.read_byte(0xC8_0000), 0xEE, "should wrap at 512K boundary");
    }
}
