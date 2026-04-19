//! Memory subsystem — M0 minimal version.
//!
//! At M0 the only storage is the Kickstart ROM. There is no chip RAM,
//! no chipset, no CIAs. Reads either come from ROM (at its anchor or
//! through the OVL=1 overlay) or return floating-bus `$FF`. Writes
//! silently drop.
//!
//! Address map at M0:
//!
//! | Range | Source |
//! |---|---|
//! | `$00_0000-$3F_FFFF` (when OVL=1) | Kickstart ROM (mirrored to fill 4 MiB window) |
//! | `$F8_0000-$FF_FFFF` | Kickstart ROM (anchored, 4-way mirror for 256K ROM) |
//! | everything else | floating bus (`$FF`); writes drop |

const OVL_BASE: u32 = 0x00_0000;
const OVL_TOP: u32 = 0x40_0000;

const ROM_BASE: u32 = 0xF8_0000;
const ROM_TOP: u32 = 0x100_0000;

/// Memory subsystem at M0: just the Kickstart ROM and the OVL overlay.
pub struct Memory {
    kickstart: Vec<u8>,
    /// Mask for wrapping addresses into the ROM image. ROM size is
    /// always a power of two (256K, 512K), so size-1 gives the mask.
    rom_mask: u32,
    /// True while the reset overlay maps ROM into the low memory
    /// region. Set to `true` at construction (real-hardware reset
    /// default) and cleared when the OVL line is later disabled via
    /// CIA-A — not yet wired at M0.
    overlay: bool,
}

impl Memory {
    /// Construct memory with the given Kickstart image. The ROM size
    /// must be a power of two (Amiga Kickstarts are 256K or 512K).
    #[must_use]
    pub fn new(kickstart: Vec<u8>) -> Self {
        assert!(
            kickstart.len().is_power_of_two(),
            "Kickstart ROM size must be a power of two; got {} bytes",
            kickstart.len()
        );
        let rom_mask = (kickstart.len() as u32).wrapping_sub(1);
        Self {
            kickstart,
            rom_mask,
            overlay: true,
        }
    }

    /// Read one byte from the active memory map.
    #[must_use]
    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;
        if self.overlay && (OVL_BASE..OVL_TOP).contains(&addr) {
            return self.rom_byte(addr);
        }
        if (ROM_BASE..ROM_TOP).contains(&addr) {
            return self.rom_byte(addr);
        }
        // Floating bus — nothing drives the data lines low.
        0xFF
    }

    /// Read one word (big-endian) from the active memory map.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        let hi = self.read_byte(addr);
        let lo = self.read_byte(addr.wrapping_add(1));
        (u16::from(hi) << 8) | u16::from(lo)
    }

    /// Read one longword (big-endian) from the active memory map.
    #[must_use]
    pub fn read_long(&self, addr: u32) -> u32 {
        let hi = self.read_word(addr);
        let lo = self.read_word(addr.wrapping_add(2));
        (u32::from(hi) << 16) | u32::from(lo)
    }

    /// Whether the reset overlay is currently mapping ROM into low
    /// memory. Read-only at M0 (cleared by CIA-A in a later milestone).
    #[must_use]
    pub fn overlay(&self) -> bool {
        self.overlay
    }

    fn rom_byte(&self, addr: u32) -> u8 {
        // Wraps both the OVL overlay (4 MiB window over a 256K ROM)
        // and the high anchor (512K window over a 256K ROM) into the
        // single ROM image via the size-mask.
        self.kickstart[(addr & self.rom_mask) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 256 * 1024];
        rom[0] = 0xDE;
        rom[1] = 0xAD;
        rom[2] = 0xBE;
        rom[3] = 0xEF;
        rom[4] = 0xCA;
        rom[5] = 0xFE;
        rom[6] = 0xBA;
        rom[7] = 0xBE;
        rom
    }

    #[test]
    fn ovl_maps_rom_at_zero() {
        let mem = Memory::new(test_rom());
        assert!(mem.overlay());
        assert_eq!(mem.read_long(0x000000), 0xDEAD_BEEF);
        assert_eq!(mem.read_long(0x000004), 0xCAFE_BABE);
    }

    #[test]
    fn rom_anchored_at_high_address() {
        let mem = Memory::new(test_rom());
        assert_eq!(mem.read_long(0xFC_0000), 0xDEAD_BEEF);
        assert_eq!(mem.read_long(0xFC_0004), 0xCAFE_BABE);
    }

    #[test]
    fn rom_mirrors_to_fill_512k_window() {
        // 256K ROM mirrored fills $F80000-$FFFFFF.
        let mem = Memory::new(test_rom());
        assert_eq!(mem.read_long(0xF8_0000), 0xDEAD_BEEF);
    }

    #[test]
    fn unmapped_returns_floating_bus() {
        let mem = Memory::new(test_rom());
        assert_eq!(mem.read_word(0xC0_0000), 0xFFFF);
        assert_eq!(mem.read_word(0xA0_0000), 0xFFFF);
        // High address past ROM mirror still in chipset/expansion space.
        assert_eq!(mem.read_word(0xE0_0000), 0xFFFF);
    }
}
