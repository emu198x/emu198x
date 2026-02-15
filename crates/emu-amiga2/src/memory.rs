//! Amiga memory subsystem.
//!
//! Supports variable chip RAM sizes, WCS (A1000) or ROM Kickstart,
//! and the reset overlay mechanism.
//!
//! Memory map (24-bit address bus):
//! - $000000-$07FFFF: Chip RAM (256K on A1000, mirrored)
//! - $000000-$1FFFFF: Chip RAM (up to 2MB, depending on Agnus variant)
//! - $C00000-$D7FFFF: Slow (Ranger) RAM (optional, A500 trapdoor)
//! - $F80000-$FFFFFF: Kickstart ROM/WCS (256K)
//!
//! On reset, ROM/WCS is overlaid at $000000 so the 68000 can read reset vectors.

#![allow(clippy::cast_possible_truncation)]

use crate::config::{AmigaConfig, KickstartSource};

/// Kickstart ROM/WCS size: 256K.
pub const KICKSTART_SIZE: usize = 256 * 1024;

/// Kickstart ROM base address.
pub const KICKSTART_BASE: u32 = 0xF8_0000;

/// Amiga memory subsystem.
pub struct Memory {
    /// Chip RAM (variable size, power of two).
    pub chip_ram: Vec<u8>,
    /// Chip RAM address mask (size - 1).
    pub(crate) chip_ram_mask: u32,
    /// Slow (Ranger) RAM at $C00000.
    pub slow_ram: Vec<u8>,
    /// Kickstart data (256K).
    kickstart: Box<[u8; KICKSTART_SIZE]>,
    /// Whether kickstart is in WCS (writable) or ROM (read-only).
    kickstart_writable: bool,
    /// When true, kickstart is mapped at $000000 (reset overlay).
    pub overlay: bool,
    /// Debug: chip RAM watchpoint address. Triggers eprintln on write.
    pub watch_addr: Option<u32>,
}

impl Memory {
    /// Create memory from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the kickstart data is not 256K.
    pub fn new(config: &AmigaConfig) -> Result<Self, String> {
        let (ks_data, writable) = match &config.kickstart {
            KickstartSource::Rom(data) => (data.as_slice(), false),
            KickstartSource::Wcs(data) => (data.as_slice(), true),
        };

        if ks_data.len() != KICKSTART_SIZE {
            return Err(format!(
                "Kickstart must be {} bytes, got {}",
                KICKSTART_SIZE,
                ks_data.len()
            ));
        }

        let chip_ram_size = config.chip_ram_size;
        if !chip_ram_size.is_power_of_two() || chip_ram_size == 0 {
            return Err(format!(
                "Chip RAM size must be a power of two, got {chip_ram_size}"
            ));
        }

        let mut kickstart = Box::new([0u8; KICKSTART_SIZE]);
        kickstart.copy_from_slice(ks_data);

        Ok(Self {
            chip_ram: vec![0u8; chip_ram_size],
            chip_ram_mask: (chip_ram_size as u32) - 1,
            slow_ram: vec![0u8; config.slow_ram_size],
            kickstart,
            kickstart_writable: writable,
            overlay: true,
            watch_addr: None,
        })
    }

    /// Read a byte from the memory map.
    #[must_use]
    pub fn read(&self, addr: u32) -> u8 {
        let addr = addr & 0x00FF_FFFF;

        // Overlay: kickstart at $000000 on reset
        if self.overlay && addr < KICKSTART_SIZE as u32 {
            #[cfg(debug_assertions)]
            if addr < 8 {
                eprintln!("  MEM READ ${addr:06X} → ROM (overlay ON) = ${:02X}", self.kickstart[addr as usize]);
            }
            return self.kickstart[addr as usize];
        }

        match addr {
            // Chip RAM: wraps at Agnus addressing boundary (chip_ram_mask).
            // Real Agnus only has N address pins, so higher bits are ignored.
            // CPU and DMA both see the same wrapping behavior.
            0x00_0000..=0x1F_FFFF => {
                let offset = (addr & self.chip_ram_mask) as usize;
                self.chip_ram[offset]
            }
            // Slow (Ranger) RAM: $C00000-$D7FFFF
            // Only respond within actual slow RAM size (no wrapping).
            0xC0_0000..=0xD7_FFFF => {
                let offset = (addr - 0xC0_0000) as usize;
                if offset < self.slow_ram.len() {
                    self.slow_ram[offset]
                } else {
                    0xFF // open bus
                }
            }
            // Kickstart ROM/WCS: $F80000-$FFFFFF
            0xF8_0000..=0xFF_FFFF => {
                let offset = (addr - KICKSTART_BASE) as usize;
                self.kickstart[offset % KICKSTART_SIZE]
            }
            // Unmapped
            _ => 0xFF,
        }
    }

    /// Write a byte to the memory map.
    pub fn write(&mut self, addr: u32, value: u8) {
        let addr = addr & 0x00FF_FFFF;

        match addr {
            // Chip RAM: wraps at Agnus addressing boundary (chip_ram_mask).
            0x00_0000..=0x1F_FFFF => {
                let offset = (addr & self.chip_ram_mask) as usize;
                if let Some(wa) = self.watch_addr {
                    if addr >= wa && addr < wa + 32 {
                        eprintln!("  WATCH: CPU write ${addr:06X} = ${value:02X}");
                    }
                }
                self.chip_ram[offset] = value;
            }
            // Slow RAM: only writable within actual size
            0xC0_0000..=0xD7_FFFF => {
                let offset = (addr - 0xC0_0000) as usize;
                if offset < self.slow_ram.len() {
                    self.slow_ram[offset] = value;
                }
            }
            // WCS is writable; ROM is not
            0xF8_0000..=0xFF_FFFF => {
                if self.kickstart_writable {
                    let offset = (addr - KICKSTART_BASE) as usize;
                    self.kickstart[offset % KICKSTART_SIZE] = value;
                }
            }
            _ => {}
        }
    }

    /// Read a word from chip RAM (for DMA). Word-aligned.
    #[must_use]
    pub fn read_chip_word(&self, addr: u32) -> u16 {
        let offset = (addr & self.chip_ram_mask & !1) as usize;
        let hi = self.chip_ram[offset];
        let lo = self.chip_ram[offset + 1];
        u16::from(hi) << 8 | u16::from(lo)
    }

    /// Write a word to chip RAM (for DMA). Word-aligned.
    pub fn write_chip_word(&mut self, addr: u32, value: u16) {
        let offset = (addr & self.chip_ram_mask & !1) as usize;
        if let Some(wa) = self.watch_addr {
            let wa = wa as usize;
            if offset >= wa && offset < wa + 32 {
                eprintln!("  WATCH: DMA write_chip_word ${:06X} = ${value:04X}", offset);
            }
        }
        self.chip_ram[offset] = (value >> 8) as u8;
        self.chip_ram[offset + 1] = value as u8;
    }

    /// Chip RAM mask for word-aligned DMA access.
    #[must_use]
    pub fn chip_ram_word_mask(&self) -> u32 {
        self.chip_ram_mask & !1
    }

    /// Clear the overlay (ROM/WCS no longer mapped at $000000).
    pub fn clear_overlay(&mut self) {
        #[cfg(debug_assertions)]
        if self.overlay {
            eprintln!("  OVERLAY: ON → OFF");
        }
        self.overlay = false;
    }

    /// Set the overlay (ROM/WCS mapped at $000000).
    pub fn set_overlay(&mut self) {
        #[cfg(debug_assertions)]
        if !self.overlay {
            eprintln!("  OVERLAY: OFF → ON");
        }
        self.overlay = true;
    }

    /// Peek at chip RAM without side effects.
    #[must_use]
    pub fn peek_chip_ram(&self, addr: u32) -> u8 {
        let offset = (addr & self.chip_ram_mask) as usize;
        self.chip_ram[offset]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AmigaConfig, AmigaModel, AgnusVariant, Chipset, CpuVariant,
        DeniseVariant, KickstartSource, Region,
    };

    fn make_config(chip_ram_size: usize) -> AmigaConfig {
        let mut ks = vec![0u8; KICKSTART_SIZE];
        // Reset vectors
        ks[0] = 0x00; ks[1] = 0x08; ks[2] = 0x00; ks[3] = 0x00;
        ks[4] = 0x00; ks[5] = 0xF8; ks[6] = 0x00; ks[7] = 0x08;
        AmigaConfig {
            model: AmigaModel::A1000,
            chipset: Chipset::Ocs,
            agnus: AgnusVariant::Agnus8361,
            denise: DeniseVariant::Denise8362,
            cpu: CpuVariant::M68000,
            region: Region::Pal,
            chip_ram_size,
            slow_ram_size: 0,
            fast_ram_size: 0,
            kickstart: KickstartSource::Wcs(ks),
        }
    }

    #[test]
    fn overlay_maps_kickstart_at_zero() {
        let mem = Memory::new(&make_config(256 * 1024)).expect("valid");
        assert!(mem.overlay);
        assert_eq!(mem.read(0x000000), 0x00);
        assert_eq!(mem.read(0x000001), 0x08);
    }

    #[test]
    fn overlay_clear_exposes_chip_ram() {
        let mut mem = Memory::new(&make_config(256 * 1024)).expect("valid");
        mem.chip_ram[0] = 0xAB;
        assert_eq!(mem.read(0x000000), 0x00); // overlay active
        mem.clear_overlay();
        assert_eq!(mem.read(0x000000), 0xAB);
    }

    #[test]
    fn writes_go_to_chip_ram_with_overlay() {
        let mut mem = Memory::new(&make_config(256 * 1024)).expect("valid");
        mem.write(0x000100, 0x42);
        assert_eq!(mem.chip_ram[0x100], 0x42);
    }

    #[test]
    fn kickstart_readable_at_f80000() {
        let mem = Memory::new(&make_config(256 * 1024)).expect("valid");
        assert_eq!(mem.read(0xF80000), 0x00);
        assert_eq!(mem.read(0xF80001), 0x08);
    }

    #[test]
    fn wcs_is_writable() {
        let mut mem = Memory::new(&make_config(256 * 1024)).expect("valid");
        mem.write(0xF80010, 0xAB);
        assert_eq!(mem.read(0xF80010), 0xAB);
    }

    #[test]
    fn chip_ram_word_access() {
        let mut mem = Memory::new(&make_config(256 * 1024)).expect("valid");
        mem.write_chip_word(0x100, 0xABCD);
        assert_eq!(mem.read_chip_word(0x100), 0xABCD);
    }

    #[test]
    fn invalid_ks_size_rejected() {
        let mut config = make_config(256 * 1024);
        config.kickstart = KickstartSource::Rom(vec![0u8; 1024]);
        assert!(Memory::new(&config).is_err());
    }
}
