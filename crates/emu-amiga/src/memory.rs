//! Amiga memory: Chip RAM (2MB) and Kickstart ROM (256K).
//!
//! On reset, ROM is overlaid at $000000 so the 68000 can read its reset
//! vectors. Kickstart clears the overlay by writing CIA-A port A bit 0.

#![allow(clippy::cast_possible_truncation, clippy::large_stack_arrays)]

/// Chip RAM size: 2MB.
pub const CHIP_RAM_SIZE: usize = 2 * 1024 * 1024;

/// Chip RAM address mask (size must be power of two).
pub const CHIP_RAM_MASK: u32 = (CHIP_RAM_SIZE as u32) - 1;

/// Chip RAM word-aligned mask.
pub const CHIP_RAM_WORD_MASK: u32 = CHIP_RAM_MASK & !1;

/// Kickstart ROM size: 256K.
pub const KICKSTART_SIZE: usize = 256 * 1024;

/// Chip RAM base address.
#[allow(dead_code)]
pub const CHIP_RAM_BASE: u32 = 0x00_0000;

/// Kickstart ROM base address.
pub const KICKSTART_BASE: u32 = 0xF8_0000;

/// Amiga memory subsystem.
pub struct Memory {
    /// 2MB chip RAM.
    pub chip_ram: Box<[u8; CHIP_RAM_SIZE]>,
    /// 256K Kickstart ROM.
    pub kickstart: Box<[u8; KICKSTART_SIZE]>,
    /// When true, ROM is mapped at $000000 (reset overlay).
    pub overlay: bool,
}

impl Memory {
    /// Create memory with the given Kickstart ROM data.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM data is not exactly 256K.
    pub fn new(kickstart_data: &[u8]) -> Result<Self, String> {
        if kickstart_data.len() != KICKSTART_SIZE {
            return Err(format!(
                "Kickstart ROM must be {} bytes, got {}",
                KICKSTART_SIZE,
                kickstart_data.len()
            ));
        }

        let mut kickstart = Box::new([0u8; KICKSTART_SIZE]);
        kickstart.copy_from_slice(kickstart_data);

        Ok(Self {
            chip_ram: Box::new([0u8; CHIP_RAM_SIZE]),
            kickstart,
            overlay: true, // ROM overlaid at $000000 on reset
        })
    }

    /// Read a byte from memory.
    #[must_use]
    pub fn read(&self, addr: u32) -> u8 {
        let addr = addr & 0x00FF_FFFF; // 24-bit address bus

        // Overlay: ROM at $000000 when active
        if self.overlay && addr < KICKSTART_SIZE as u32 {
            return self.kickstart[addr as usize];
        }

        match addr {
            // Chip RAM: $000000-$07FFFF
            0x00_0000..=CHIP_RAM_MASK => self.chip_ram[addr as usize],
            // Kickstart ROM: $F80000-$FFFFFF
            0xF8_0000..=0xFF_FFFF => {
                let offset = (addr - KICKSTART_BASE) as usize;
                self.kickstart[offset % KICKSTART_SIZE]
            }
            // Unmapped: return $FF (open bus)
            _ => 0xFF,
        }
    }

    /// Write a byte to memory.
    pub fn write(&mut self, addr: u32, value: u8) {
        let addr = addr & 0x00FF_FFFF;

        // Chip RAM: $000000-$07FFFF (writes always go to RAM, even with overlay)
        if addr < CHIP_RAM_SIZE as u32 {
            self.chip_ram[addr as usize] = value;
        }
        // ROM and other areas: writes ignored
    }

    /// Clear the overlay (ROM no longer mapped at $000000).
    pub fn clear_overlay(&mut self) {
        if !self.overlay {
            return;
        }
        self.overlay = false;
    }

    /// Set the overlay (ROM mapped at $000000).
    pub fn set_overlay(&mut self) {
        self.overlay = true;
    }

    /// Peek at chip RAM without side effects (for observation).
    #[must_use]
    pub fn peek_chip_ram(&self, addr: u32) -> u8 {
        let offset = (addr & CHIP_RAM_MASK) as usize;
        self.chip_ram[offset]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory() -> Memory {
        let mut rom = vec![0u8; KICKSTART_SIZE];
        // Put test values at reset vector locations
        // SSP at $000000-$000003
        rom[0] = 0x00;
        rom[1] = 0x08;
        rom[2] = 0x00;
        rom[3] = 0x00; // SSP = $00080000
        // PC at $000004-$000007
        rom[4] = 0x00;
        rom[5] = 0xFC;
        rom[6] = 0x00;
        rom[7] = 0xD2; // PC = $00FC00D2 (typical Kickstart entry)
        Memory::new(&rom).expect("valid ROM")
    }

    #[test]
    fn overlay_maps_rom_at_zero() {
        let mem = make_memory();
        assert!(mem.overlay);
        // Read reset vector from $000000 — should come from ROM
        assert_eq!(mem.read(0x000000), 0x00);
        assert_eq!(mem.read(0x000001), 0x08);
        assert_eq!(mem.read(0x000004), 0x00);
        assert_eq!(mem.read(0x000005), 0xFC);
    }

    #[test]
    fn overlay_clear_exposes_chip_ram() {
        let mut mem = make_memory();
        mem.chip_ram[0] = 0xAB;
        // With overlay, reads come from ROM
        assert_eq!(mem.read(0x000000), 0x00);
        // Clear overlay
        mem.clear_overlay();
        assert!(!mem.overlay);
        // Now reads come from chip RAM
        assert_eq!(mem.read(0x000000), 0xAB);
    }

    #[test]
    fn writes_go_to_chip_ram_even_with_overlay() {
        let mut mem = make_memory();
        mem.write(0x000100, 0x42);
        assert_eq!(mem.chip_ram[0x100], 0x42);
    }

    #[test]
    fn kickstart_readable_at_f80000() {
        let mem = make_memory();
        assert_eq!(mem.read(0xF80000), 0x00);
        assert_eq!(mem.read(0xF80001), 0x08);
    }

    #[test]
    fn rom_write_ignored() {
        let mut mem = make_memory();
        let original = mem.read(0xF80000);
        mem.write(0xF80000, 0xFF);
        assert_eq!(mem.read(0xF80000), original);
    }

    #[test]
    fn invalid_rom_size_rejected() {
        let rom = vec![0u8; 1024];
        assert!(Memory::new(&rom).is_err());
    }
}
