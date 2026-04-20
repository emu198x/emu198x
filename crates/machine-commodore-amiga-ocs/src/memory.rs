//! Memory subsystem.
//!
//! Built incrementally per the restart milestones in
//! `wiki/decisions/amiga-restart-plan.md`.
//!
//! Current state — M1:
//! - 256K Kickstart ROM at its anchor `$F8_0000-$FF_FFFF` (256K
//!   image mirrored to fill the 512K window).
//! - 512 KiB chip RAM at `$00_0000-$07_FFFF`.
//! - OVL=1 (default) maps ROM into `$00_0000-$3F_FFFF` for **reads**
//!   only. Writes always land in chip RAM, matching real Amiga
//!   behaviour where OVL gates reads alone.
//! - Custom registers (`$DF_0000-$DF_FFFF`) and CIA address space
//!   (`$BF_0000-$BF_FFFF`) silently absorb writes and read as
//!   floating bus. No behaviour wired yet.
//! - Everything else: floating-bus reads (`$FF`), writes drop.
//!
//! Chip-RAM aliasing (`$80000` wraps to `$0` on a 512K-only machine)
//! is **not** modelled yet — it lands in M3/M4 when the chip-RAM probe
//! demands it.

const OVL_BASE: u32 = 0x00_0000;
const OVL_TOP: u32 = 0x40_0000;

/// Gary decodes chip-RAM-style accesses for the entire `$0-$1FFFFF`
/// range on the A500/A2000 (the full Agnus address space). The actual
/// installed chip RAM is smaller; addresses above the installed top
/// alias back via the chip-RAM mask.
const CHIP_RAM_DECODE_BASE: u32 = 0x00_0000;
const CHIP_RAM_DECODE_TOP: u32 = 0x20_0000;

const CIA_BASE: u32 = 0x00BF_0000;
const CIA_TOP: u32 = 0x00C0_0000;

const SLOW_RAM_BASE: u32 = 0x00C0_0000;

const CUSTOM_BASE: u32 = 0x00DF_0000;
const CUSTOM_TOP: u32 = 0x00E0_0000;

const ROM_BASE: u32 = 0x00F8_0000;
const ROM_TOP: u32 = 0x0100_0000;

pub const CHIP_RAM_SIZE: usize = 512 * 1024;

/// Memory subsystem for the Amiga (OCS).
pub struct Memory {
    chip_ram: Vec<u8>,
    chip_ram_mask: u32,
    slow_ram: Vec<u8>,
    kickstart: Vec<u8>,
    rom_mask: u32,
    overlay: bool,
}

impl Memory {
    /// Construct memory with the given Kickstart image and no slow RAM.
    #[must_use]
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::new_with_slow_ram(kickstart, 0)
    }

    /// Construct memory with the given Kickstart image and a trapdoor
    /// slow-RAM bank of `slow_ram_bytes` at `$C00000`. Pass 0 for no
    /// slow RAM (A500 bare configuration).
    #[must_use]
    pub fn new_with_slow_ram(kickstart: Vec<u8>, slow_ram_bytes: usize) -> Self {
        assert!(
            kickstart.len().is_power_of_two(),
            "Kickstart ROM size must be a power of two; got {} bytes",
            kickstart.len()
        );
        let rom_mask = (kickstart.len() as u32).wrapping_sub(1);
        Self {
            chip_ram: vec![0; CHIP_RAM_SIZE],
            chip_ram_mask: (CHIP_RAM_SIZE as u32).wrapping_sub(1),
            slow_ram: vec![0; slow_ram_bytes],
            kickstart,
            rom_mask,
            overlay: true,
        }
    }

    /// Whether the reset overlay is currently mapping ROM into low
    /// memory.
    #[must_use]
    pub fn overlay(&self) -> bool {
        self.overlay
    }

    /// Set the overlay state. Driven by CIA-A PRA bit 0 (gated by
    /// DDRA bit 0). True = ROM mapped at `$0`; false = chip RAM at `$0`.
    pub fn set_overlay(&mut self, overlay: bool) {
        self.overlay = overlay;
    }

    /// Direct chip-RAM byte read — bypasses the OVL-aware public path.
    /// Used by tests to verify chip-RAM contents independent of the
    /// overlay state.
    #[must_use]
    pub fn read_chip_ram_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;
        if (CHIP_RAM_DECODE_BASE..CHIP_RAM_DECODE_TOP).contains(&addr) {
            self.chip_ram[(addr & self.chip_ram_mask) as usize]
        } else {
            0
        }
    }

    /// Read one byte from the active memory map.
    #[must_use]
    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;

        // OVL routes low-memory READS to ROM when active.
        if self.overlay && (OVL_BASE..OVL_TOP).contains(&addr) {
            return self.rom_byte(addr);
        }

        // Chip RAM, with incomplete address decode (Agnus 19-bit
        // address bus → addresses above 512K alias back).
        if (CHIP_RAM_DECODE_BASE..CHIP_RAM_DECODE_TOP).contains(&addr) {
            return self.chip_ram[(addr & self.chip_ram_mask) as usize];
        }

        // Slow RAM (trapdoor) at $C00000, up to installed size.
        if addr >= SLOW_RAM_BASE && !self.slow_ram.is_empty() {
            let off = (addr - SLOW_RAM_BASE) as usize;
            if off < self.slow_ram.len() {
                return self.slow_ram[off];
            }
        }

        // ROM at its anchor.
        if (ROM_BASE..ROM_TOP).contains(&addr) {
            return self.rom_byte(addr);
        }

        // Custom-register space and CIA space read as floating bus —
        // no behaviour wired yet.
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

    /// Write one byte through the active memory map.
    pub fn write_byte(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xFF_FFFF;

        // Chip-RAM writes always land — OVL only affects reads. The
        // 19-bit address mask aliases anything in the chip-RAM
        // decode range into the installed pool.
        if (CHIP_RAM_DECODE_BASE..CHIP_RAM_DECODE_TOP).contains(&addr) {
            self.chip_ram[(addr & self.chip_ram_mask) as usize] = val;
            return;
        }

        // Slow RAM at $C00000, up to installed size.
        if addr >= SLOW_RAM_BASE && !self.slow_ram.is_empty() {
            let off = (addr - SLOW_RAM_BASE) as usize;
            if off < self.slow_ram.len() {
                self.slow_ram[off] = val;
                return;
            }
        }

        // CIA / custom register / ROM / unmapped: silently drop.
        let _ = (CIA_BASE, CIA_TOP, CUSTOM_BASE, CUSTOM_TOP, val);
    }

    /// Write one word (big-endian) through the active memory map.
    pub fn write_word(&mut self, addr: u32, val: u16) {
        self.write_byte(addr, (val >> 8) as u8);
        self.write_byte(addr.wrapping_add(1), val as u8);
    }

    fn rom_byte(&self, addr: u32) -> u8 {
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
    fn ovl_maps_rom_at_zero_for_reads() {
        let mem = Memory::new(test_rom());
        assert!(mem.overlay());
        assert_eq!(mem.read_long(0x000000), 0xDEAD_BEEF);
        assert_eq!(mem.read_long(0x000004), 0xCAFE_BABE);
    }

    #[test]
    fn writes_to_low_memory_land_in_chip_ram_even_when_ovl_is_on() {
        let mut mem = Memory::new(test_rom());
        assert!(mem.overlay());
        mem.write_word(0x100, 0x1234);
        // Public read goes through OVL → returns ROM (not the value
        // we just wrote).
        assert_eq!(mem.read_word(0x100), 0x0000);
        // Direct chip-RAM read confirms the write landed.
        assert_eq!(mem.read_chip_ram_byte(0x100), 0x12);
        assert_eq!(mem.read_chip_ram_byte(0x101), 0x34);
    }

    #[test]
    fn rom_anchored_at_high_address() {
        let mem = Memory::new(test_rom());
        assert_eq!(mem.read_long(0xFC_0000), 0xDEAD_BEEF);
    }

    #[test]
    fn rom_mirrors_to_fill_512k_window() {
        let mem = Memory::new(test_rom());
        assert_eq!(mem.read_long(0xF8_0000), 0xDEAD_BEEF);
    }

    #[test]
    fn unmapped_returns_floating_bus() {
        let mem = Memory::new(test_rom());
        assert_eq!(mem.read_word(0xC0_0000), 0xFFFF);
        assert_eq!(mem.read_word(0xA0_0000), 0xFFFF);
        assert_eq!(mem.read_word(0xE0_0000), 0xFFFF);
    }

    #[test]
    fn cia_and_custom_writes_are_silent() {
        // No panic, no error — just dropped.
        let mut mem = Memory::new(test_rom());
        mem.write_word(0x00BFE001, 0x0203);
        mem.write_word(0x00DFF09A, 0x7FFF);
        assert_eq!(mem.read_word(0x00BFE000), 0xFFFF); // floating bus
        assert_eq!(mem.read_word(0x00DFF09A), 0xFFFF);
    }
}
