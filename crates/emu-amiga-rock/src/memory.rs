//! Memory management for the Amiga Rock.

pub struct Memory {
    pub chip_ram: Vec<u8>,
    pub kickstart: Vec<u8>,
}

impl Memory {
    pub fn new(chip_ram_size: usize, kickstart: Vec<u8>) -> Self {
        Self {
            chip_ram: vec![0; chip_ram_size],
            kickstart,
        }
    }
}
