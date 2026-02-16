//! Memory management for the Amiga Rock.

pub const CHIP_RAM_BASE: u32 = 0x000000;
pub const CIA_A_BASE: u32 = 0xBFE001;
pub const CIA_B_BASE: u32 = 0xBFD000;
pub const CUSTOM_REGS_BASE: u32 = 0xDFF000;
pub const ROM_BASE: u32 = 0xF80000;

pub struct Memory {
    pub chip_ram: Vec<u8>,
    pub chip_ram_mask: u32,
    pub kickstart: Vec<u8>,
    pub kickstart_mask: u32,
    pub overlay: bool,
}

impl Memory {
    pub fn new(chip_ram_size: usize, kickstart: Vec<u8>) -> Self {
        let ks_len = kickstart.len();
        Self {
            chip_ram: vec![0; chip_ram_size],
            chip_ram_mask: (chip_ram_size as u32).wrapping_sub(1),
            kickstart,
            kickstart_mask: (ks_len as u32).wrapping_sub(1),
            overlay: true, // Amiga starts with ROM overlay at $0
        }
    }

    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFFFFFF;
        
        if self.overlay && addr < 0x200000 {
            // Overlay maps ROM to $0
            return self.kickstart[(addr & self.kickstart_mask) as usize];
        }

        if addr < 0x200000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize]
        } else if addr >= ROM_BASE {
            self.kickstart[(addr & self.kickstart_mask) as usize]
        } else {
            0xFF // Open bus
        }
    }

    pub fn write_byte(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xFFFFFF;
        if addr < 0x200000 {
            self.chip_ram[(addr & self.chip_ram_mask) as usize] = val;
        }
        // ROM is read-only
    }
}
