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
    /// Backing store for unmapped expansion space ($C00000-$DFFFFF).
    ///
    /// On a real A500, Gary always asserts DTACK for this range even
    /// without expansion RAM. Bus capacitance on the data bus holds
    /// recently written values. The KS 1.2+ boot code uses this space
    /// for the initial stack and ExecBase when no expansion is found,
    /// relying on the values persisting briefly.
    ///
    /// We model this as full RAM-like storage. On real hardware the
    /// values would decay, but the boot code only uses a small region
    /// near $DC0000 for the stack and ExecBase, and all accesses are
    /// sequential enough that decay doesn't matter.
    pub expansion_bus_cache: Vec<u8>,
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
            expansion_bus_cache: vec![0u8; 0x20_0000],
        }
    }

    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFFFFFF;

        if self.overlay && addr < 0x200000 {
            return self.kickstart[(addr & self.kickstart_mask) as usize];
        }

        if addr < 0x20_0000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize]
        } else if (0xC0_0000..0xE0_0000).contains(&addr) {
            if !self.slow_ram.is_empty() {
                let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
                self.slow_ram[offset as usize]
            } else {
                let offset = (addr - 0xC0_0000) as usize;
                self.expansion_bus_cache[offset]
            }
        } else if addr >= ROM_BASE {
            self.kickstart[(addr & self.kickstart_mask) as usize]
        } else {
            0xFF
        }
    }

    pub fn read_chip_byte(&self, addr: u32) -> u8 {
        self.chip_ram[(addr & self.chip_ram_mask) as usize]
    }

    pub fn write_byte(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xFFFFFF;
        if addr < 0x20_0000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize] = val;
        } else if (0xC0_0000..0xE0_0000).contains(&addr) {
            if !self.slow_ram.is_empty() {
                let offset = (addr - 0xC0_0000) & self.slow_ram_mask;
                self.slow_ram[offset as usize] = val;
            } else {
                let offset = (addr - 0xC0_0000) as usize;
                self.expansion_bus_cache[offset] = val;
            }
        }
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
    fn expansion_bus_cache() {
        let mut mem = Memory::new(512 * 1024, test_ks(), 0);
        assert_eq!(mem.read_byte(0xC0_0000), 0x00);
        // Writes persist at the written address
        mem.write_byte(0xC0_1000, 0x3F);
        mem.write_byte(0xC0_1001, 0xFF);
        assert_eq!(mem.read_byte(0xC0_1000), 0x3F);
        assert_eq!(mem.read_byte(0xC0_1001), 0xFF);
        // Other addresses remain 0
        assert_eq!(mem.read_byte(0xC5_0000), 0x00);
        // Stack-like operations work (push then pop)
        mem.write_byte(0xDB_FFFC, 0x00);
        mem.write_byte(0xDB_FFFD, 0xFC);
        mem.write_byte(0xDB_FFFE, 0x02);
        mem.write_byte(0xDB_FFFF, 0xA8);
        assert_eq!(mem.read_byte(0xDB_FFFE), 0x02);
        assert_eq!(mem.read_byte(0xDB_FFFF), 0xA8);
        assert_eq!(mem.read_byte(0xDB_FFFC), 0x00);
        assert_eq!(mem.read_byte(0xDB_FFFD), 0xFC);
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
        assert_eq!(mem.read_byte(0xC8_0000), 0xEE, "should wrap at 512K boundary");
    }
}
