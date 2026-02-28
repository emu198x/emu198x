//! REU (RAM Expansion Unit) 1700/1764/1750 emulation.
//!
//! The REU provides additional RAM accessible via DMA transfers
//! between C64 memory and the expansion RAM. Registers at $DF00-$DF0A.
//!
//! # Register map
//!
//! | Addr  | Register            |
//! |-------|---------------------|
//! | $DF00 | Status register     |
//! | $DF01 | Command register    |
//! | $DF02 | C64 base addr lo    |
//! | $DF03 | C64 base addr hi    |
//! | $DF04 | REU base addr lo    |
//! | $DF05 | REU base addr hi    |
//! | $DF06 | REU bank            |
//! | $DF07 | Transfer length lo  |
//! | $DF08 | Transfer length hi  |
//! | $DF09 | IRQ mask            |
//! | $DF0A | Address control     |

#![allow(clippy::cast_possible_truncation)]

/// REU (RAM Expansion Unit).
pub struct Reu {
    /// Expansion RAM (128K, 256K, or 512K).
    ram: Vec<u8>,
    /// Status register ($DF00).
    status: u8,
    /// Command register ($DF01).
    command: u8,
    /// C64 base address (16-bit).
    c64_addr: u16,
    /// REU base address (19-bit: bank + 16-bit address).
    reu_addr: u32,
    /// Transfer length (16-bit, 0 = 65536).
    length: u16,
    /// IRQ mask ($DF09).
    irq_mask: u8,
    /// Address control ($DF0A).
    addr_control: u8,
    /// RAM size in bytes.
    ram_size: u32,
}

impl Reu {
    /// Create a new REU with the given size in KB.
    ///
    /// Valid sizes: 128, 256, 512.
    #[must_use]
    pub fn new(size_kb: u32) -> Self {
        let ram_size = size_kb * 1024;
        Self {
            ram: vec![0; ram_size as usize],
            status: 0x10, // Version bits (1750 = $10)
            command: 0,
            c64_addr: 0,
            reu_addr: 0,
            length: 0xFFFF,
            irq_mask: 0,
            addr_control: 0,
            ram_size,
        }
    }

    /// Read a REU register ($DF00-$DF0A).
    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xDF00 => self.status,
            0xDF01 => self.command,
            0xDF02 => self.c64_addr as u8,
            0xDF03 => (self.c64_addr >> 8) as u8,
            0xDF04 => self.reu_addr as u8,
            0xDF05 => (self.reu_addr >> 8) as u8,
            0xDF06 => (self.reu_addr >> 16) as u8,
            0xDF07 => self.length as u8,
            0xDF08 => (self.length >> 8) as u8,
            0xDF09 => self.irq_mask,
            0xDF0A => self.addr_control,
            _ => 0xFF,
        }
    }

    /// Write a REU register ($DF00-$DF0A).
    ///
    /// Writing $DF01 with the execute bit set triggers a DMA transfer.
    pub fn write(&mut self, addr: u16, value: u8, c64_ram: &mut [u8; 0x10000]) {
        match addr {
            0xDF00 => { /* Status is read-only */ }
            0xDF01 => {
                self.command = value;
                if value & 0x80 != 0 {
                    // FF bit: trigger autoload from shadow registers
                    // (we don't implement shadow registers — just execute)
                }
                if value & 0x90 != 0 {
                    self.execute_dma(c64_ram);
                }
            }
            0xDF02 => self.c64_addr = (self.c64_addr & 0xFF00) | u16::from(value),
            0xDF03 => self.c64_addr = (self.c64_addr & 0x00FF) | (u16::from(value) << 8),
            0xDF04 => self.reu_addr = (self.reu_addr & 0x07FF00) | u32::from(value),
            0xDF05 => self.reu_addr = (self.reu_addr & 0x0700FF) | (u32::from(value) << 8),
            0xDF06 => self.reu_addr = (self.reu_addr & 0x00FFFF) | ((u32::from(value) & 0x07) << 16),
            0xDF07 => self.length = (self.length & 0xFF00) | u16::from(value),
            0xDF08 => self.length = (self.length & 0x00FF) | (u16::from(value) << 8),
            0xDF09 => self.irq_mask = value & 0xE0,
            0xDF0A => self.addr_control = value & 0xC0,
            _ => {}
        }
    }

    /// Execute a DMA transfer based on the command register.
    fn execute_dma(&mut self, c64_ram: &mut [u8; 0x10000]) {
        let op = self.command & 0x03;
        let count = if self.length == 0 { 0x10000u32 } else { u32::from(self.length) };
        let fix_c64 = self.addr_control & 0x80 != 0;
        let fix_reu = self.addr_control & 0x40 != 0;

        let mut c64_ptr = u32::from(self.c64_addr);
        let mut reu_ptr = self.reu_addr;
        let mut verify_error = false;

        for _ in 0..count {
            let c64_idx = (c64_ptr & 0xFFFF) as usize;
            let reu_idx = (reu_ptr % self.ram_size) as usize;

            match op {
                0 => {
                    // STASH: C64 → REU
                    if reu_idx < self.ram.len() {
                        self.ram[reu_idx] = c64_ram[c64_idx];
                    }
                }
                1 => {
                    // FETCH: REU → C64
                    if reu_idx < self.ram.len() {
                        c64_ram[c64_idx] = self.ram[reu_idx];
                    }
                }
                2 => {
                    // SWAP: exchange bytes
                    if reu_idx < self.ram.len() {
                        let tmp = c64_ram[c64_idx];
                        c64_ram[c64_idx] = self.ram[reu_idx];
                        self.ram[reu_idx] = tmp;
                    }
                }
                3 => {
                    // VERIFY: compare bytes
                    if reu_idx < self.ram.len() && c64_ram[c64_idx] != self.ram[reu_idx] {
                        verify_error = true;
                        break;
                    }
                }
                _ => {}
            }

            if !fix_c64 {
                c64_ptr = c64_ptr.wrapping_add(1);
            }
            if !fix_reu {
                reu_ptr = reu_ptr.wrapping_add(1);
            }
        }

        // Update registers (unless autoload is set for next transfer)
        if self.command & 0x20 == 0 {
            self.c64_addr = (c64_ptr & 0xFFFF) as u16;
            self.reu_addr = reu_ptr & 0x07FFFF;
            self.length = 1; // DMA sets length to 1 after completion
        }

        // Set status bits
        self.status &= 0x1F;
        self.status |= 0x40; // End-of-block
        if verify_error {
            self.status |= 0x20; // Verify error
        }

        // Auto-clear execute bit in one-shot mode
        if self.command & 0x20 == 0 {
            self.command &= !0x90;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_copies_c64_to_reu() {
        let mut reu = Reu::new(128);
        let mut ram = [0u8; 0x10000];
        ram[0x1000] = 0xAA;
        ram[0x1001] = 0xBB;
        ram[0x1002] = 0xCC;

        // Set C64 address to $1000
        reu.write(0xDF02, 0x00, &mut ram);
        reu.write(0xDF03, 0x10, &mut ram);
        // Set REU address to $000000
        reu.write(0xDF04, 0x00, &mut ram);
        reu.write(0xDF05, 0x00, &mut ram);
        reu.write(0xDF06, 0x00, &mut ram);
        // Set length to 3
        reu.write(0xDF07, 0x03, &mut ram);
        reu.write(0xDF08, 0x00, &mut ram);
        // Execute STASH (command = $90)
        reu.write(0xDF01, 0x90, &mut ram);

        assert_eq!(reu.ram[0], 0xAA);
        assert_eq!(reu.ram[1], 0xBB);
        assert_eq!(reu.ram[2], 0xCC);
    }

    #[test]
    fn fetch_copies_reu_to_c64() {
        let mut reu = Reu::new(128);
        let mut ram = [0u8; 0x10000];
        reu.ram[0] = 0x11;
        reu.ram[1] = 0x22;

        reu.write(0xDF02, 0x00, &mut ram);
        reu.write(0xDF03, 0x20, &mut ram);
        reu.write(0xDF04, 0x00, &mut ram);
        reu.write(0xDF05, 0x00, &mut ram);
        reu.write(0xDF06, 0x00, &mut ram);
        reu.write(0xDF07, 0x02, &mut ram);
        reu.write(0xDF08, 0x00, &mut ram);
        // Execute FETCH (command = $91)
        reu.write(0xDF01, 0x91, &mut ram);

        assert_eq!(ram[0x2000], 0x11);
        assert_eq!(ram[0x2001], 0x22);
    }

    #[test]
    fn swap_exchanges_bytes() {
        let mut reu = Reu::new(128);
        let mut ram = [0u8; 0x10000];
        ram[0x3000] = 0xAA;
        reu.ram[0] = 0xBB;

        reu.write(0xDF02, 0x00, &mut ram);
        reu.write(0xDF03, 0x30, &mut ram);
        reu.write(0xDF04, 0x00, &mut ram);
        reu.write(0xDF05, 0x00, &mut ram);
        reu.write(0xDF06, 0x00, &mut ram);
        reu.write(0xDF07, 0x01, &mut ram);
        reu.write(0xDF08, 0x00, &mut ram);
        // Execute SWAP (command = $92)
        reu.write(0xDF01, 0x92, &mut ram);

        assert_eq!(ram[0x3000], 0xBB);
        assert_eq!(reu.ram[0], 0xAA);
    }

    #[test]
    fn verify_sets_error_on_mismatch() {
        let mut reu = Reu::new(128);
        let mut ram = [0u8; 0x10000];
        ram[0x4000] = 0xAA;
        reu.ram[0] = 0xBB;

        reu.write(0xDF02, 0x00, &mut ram);
        reu.write(0xDF03, 0x40, &mut ram);
        reu.write(0xDF04, 0x00, &mut ram);
        reu.write(0xDF05, 0x00, &mut ram);
        reu.write(0xDF06, 0x00, &mut ram);
        reu.write(0xDF07, 0x01, &mut ram);
        reu.write(0xDF08, 0x00, &mut ram);
        // Execute VERIFY (command = $93)
        reu.write(0xDF01, 0x93, &mut ram);

        assert!(reu.status & 0x20 != 0, "Verify error bit should be set");
    }

    #[test]
    fn register_read_back() {
        let reu = Reu::new(512);
        assert_eq!(reu.read(0xDF00) & 0x10, 0x10); // Version bits
    }
}
