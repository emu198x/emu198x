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

/// Default chip-RAM size used by `Memory::new` and the A500 bare
/// factory — 512 KiB, the stock A500 config.
pub const DEFAULT_CHIP_RAM_SIZE: usize = 512 * 1024;

/// Back-compat alias. Prefer `DEFAULT_CHIP_RAM_SIZE` at call sites —
/// this will be deprecated once downstream crates migrate.
pub const CHIP_RAM_SIZE: usize = DEFAULT_CHIP_RAM_SIZE;

/// Chip-RAM sizes Agnus can decode with different address-bus widths.
/// The three-chip family:
///   - 8361  — 19-bit bus, 256K/512K only (A1000 / original A500)
///   - 8370  — same, introduced in 8372A revisions (A500 pre-ECS)
///   - 8372A — 20-bit bus, up to 1M (A500Plus / A2000 rev 6)
///   - 8372B — 21-bit bus, up to 2M (ECS, A3000)
#[must_use]
pub fn is_valid_chip_ram_size(bytes: usize) -> bool {
    matches!(bytes, 0x4_0000 | 0x8_0000 | 0x10_0000 | 0x20_0000)
}

/// Slow-RAM sizes the A501 trapdoor and its clones supported. 1.5M is
/// the pre-ECS "fast" A501S trapdoor (split 512K + 1M); ECS A500Plus
/// remapped the trapdoor slot as chip RAM.
#[must_use]
pub fn is_valid_slow_ram_size(bytes: usize) -> bool {
    matches!(bytes, 0 | 0x4_0000 | 0x8_0000 | 0x10_0000 | 0x18_0000)
}

/// Memory subsystem for the Amiga (OCS).
pub struct Memory {
    chip_ram: Vec<u8>,
    chip_ram_mask: u32,
    slow_ram: Vec<u8>,
    kickstart: Vec<u8>,
    rom_mask: u32,
    overlay: bool,
    /// Floating-bus state: the last 16-bit value driven on the chip
    /// bus. When the CPU reads an unmapped address, real Amiga
    /// hardware returns whatever residual charge is on the data lines
    /// from the most recent transaction (instruction prefetch, DMA
    /// cycle, previous write, etc.) rather than a constant.
    ///
    /// Interior-mutable via `Cell` so read paths can update it
    /// without requiring `&mut self` on the public read API.
    last_bus_value: std::cell::Cell<u16>,
}

impl Memory {
    /// Construct memory with the given Kickstart image, stock 512K
    /// chip RAM, no slow RAM.
    #[must_use]
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::new_with_ram(kickstart, DEFAULT_CHIP_RAM_SIZE, 0)
    }

    /// Construct memory with stock 512K chip RAM + a trapdoor slow-RAM
    /// bank of `slow_ram_bytes` at `$C00000`. Pass 0 for no slow RAM.
    #[must_use]
    pub fn new_with_slow_ram(kickstart: Vec<u8>, slow_ram_bytes: usize) -> Self {
        Self::new_with_ram(kickstart, DEFAULT_CHIP_RAM_SIZE, slow_ram_bytes)
    }

    /// Construct memory with fully explicit chip + slow-RAM sizes.
    ///
    /// `chip_bytes` must be one of {256K, 512K, 1M, 2M} (see
    /// `is_valid_chip_ram_size`). `slow_bytes` must be one of
    /// {0, 256K, 512K, 1M, 1.5M} (see `is_valid_slow_ram_size`).
    ///
    /// Fast RAM (Zorro-II autoconfig) lives on a separate chip in a
    /// later milestone — the machine wires it up; `Memory` only owns
    /// chip + slow.
    #[must_use]
    pub fn new_with_ram(kickstart: Vec<u8>, chip_bytes: usize, slow_bytes: usize) -> Self {
        assert!(
            kickstart.len().is_power_of_two(),
            "Kickstart ROM size must be a power of two; got {} bytes",
            kickstart.len()
        );
        assert!(
            is_valid_chip_ram_size(chip_bytes),
            "chip-RAM size must be 256K / 512K / 1M / 2M; got {} bytes",
            chip_bytes
        );
        assert!(
            is_valid_slow_ram_size(slow_bytes),
            "slow-RAM size must be 0 / 256K / 512K / 1M / 1.5M; got {} bytes",
            slow_bytes
        );
        let rom_mask = (kickstart.len() as u32).wrapping_sub(1);
        Self {
            chip_ram: vec![0; chip_bytes],
            chip_ram_mask: (chip_bytes as u32).wrapping_sub(1),
            slow_ram: vec![0; slow_bytes],
            kickstart,
            rom_mask,
            overlay: true,
            last_bus_value: std::cell::Cell::new(0x0000),
        }
    }

    /// Currently-installed chip-RAM size in bytes.
    #[must_use]
    pub fn chip_ram_size(&self) -> usize {
        self.chip_ram.len()
    }

    /// Currently-installed slow-RAM size in bytes. Returns 0 when no
    /// trapdoor expansion is present.
    #[must_use]
    pub fn slow_ram_size(&self) -> usize {
        self.slow_ram.len()
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

    /// Snapshot the current floating-bus value. Diagnostic accessor.
    #[must_use]
    pub fn last_bus_value(&self) -> u16 {
        self.last_bus_value.get()
    }

    /// Directly overwrite the floating-bus state. For callers that
    /// drive the chip bus outside Memory's read/write path — e.g.
    /// AmigaOcs routing CIA or custom-register accesses, which don't
    /// go through Memory but still leave residue on the bus.
    pub fn set_last_bus_value(&self, val: u16) {
        self.last_bus_value.set(val);
    }

    /// Update the floating bus with the byte-slot corresponding to a
    /// single-byte transaction. The chip bus is 16 bits wide; at even
    /// address the high byte is "live", at odd address the low byte.
    fn update_bus_from_byte(&self, addr: u32, byte: u8) {
        let mut bus = self.last_bus_value.get();
        if addr & 1 == 0 {
            bus = (bus & 0x00FF) | (u16::from(byte) << 8);
        } else {
            bus = (bus & 0xFF00) | u16::from(byte);
        }
        self.last_bus_value.set(bus);
    }

    /// Direct chip-RAM byte read — bypasses OVL and does NOT update
    /// the floating bus. For test backdoor inspections and for DMA
    /// code paths that compose bytes manually; genuine DMA
    /// transactions should use `read_chip_ram_word` so the bus
    /// records the access.
    #[must_use]
    pub fn read_chip_ram_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;
        if (CHIP_RAM_DECODE_BASE..CHIP_RAM_DECODE_TOP).contains(&addr) {
            self.chip_ram[(addr & self.chip_ram_mask) as usize]
        } else {
            0
        }
    }

    /// Chip-RAM word read driven by Agnus DMA (Denise, Copper, …).
    /// Bypasses OVL and updates the floating bus — real DMA cycles
    /// drive the chip bus the same way a CPU read does.
    #[must_use]
    pub fn read_chip_ram_word(&self, addr: u32) -> u16 {
        let addr = addr & 0xFF_FFFF;
        let hi = self.read_chip_ram_byte(addr);
        let lo = self.read_chip_ram_byte(addr.wrapping_add(1));
        let word = (u16::from(hi) << 8) | u16::from(lo);
        self.last_bus_value.set(word);
        word
    }

    /// Read one byte from the active memory map.
    #[must_use]
    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;

        // OVL routes low-memory READS to ROM when active.
        if self.overlay && (OVL_BASE..OVL_TOP).contains(&addr) {
            let byte = self.rom_byte(addr);
            self.update_bus_from_byte(addr, byte);
            return byte;
        }

        // Chip RAM, with incomplete address decode (Agnus 19-bit
        // address bus → addresses above 512K alias back).
        if (CHIP_RAM_DECODE_BASE..CHIP_RAM_DECODE_TOP).contains(&addr) {
            let byte = self.chip_ram[(addr & self.chip_ram_mask) as usize];
            self.update_bus_from_byte(addr, byte);
            return byte;
        }

        // Slow RAM (trapdoor) at $C00000, up to installed size.
        if addr >= SLOW_RAM_BASE && !self.slow_ram.is_empty() {
            let off = (addr - SLOW_RAM_BASE) as usize;
            if off < self.slow_ram.len() {
                let byte = self.slow_ram[off];
                self.update_bus_from_byte(addr, byte);
                return byte;
            }
        }

        // ROM at its anchor.
        if (ROM_BASE..ROM_TOP).contains(&addr) {
            let byte = self.rom_byte(addr);
            self.update_bus_from_byte(addr, byte);
            return byte;
        }

        // Unmapped read: no device drives the bus, so the CPU reads
        // whatever the last transaction left on the data lines.
        // (Byte slot selected by address parity: even → high, odd →
        // low of the 16-bit chip bus.)
        let bus = self.last_bus_value.get();
        if addr & 1 == 0 {
            (bus >> 8) as u8
        } else {
            bus as u8
        }
    }

    /// Read one word (big-endian) from the active memory map.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        let hi = self.read_byte(addr);
        let lo = self.read_byte(addr.wrapping_add(1));
        let word = (u16::from(hi) << 8) | u16::from(lo);
        self.last_bus_value.set(word);
        word
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
        self.update_bus_from_byte(addr, val);

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

        // CIA / custom register / ROM / unmapped: silently drop (the
        // bus-value update above still records the write — real
        // hardware drives the lines during unmapped writes too).
        let _ = (CIA_BASE, CIA_TOP, CUSTOM_BASE, CUSTOM_TOP, val);
    }

    /// Write one word (big-endian) through the active memory map.
    pub fn write_word(&mut self, addr: u32, val: u16) {
        self.write_byte(addr, (val >> 8) as u8);
        self.write_byte(addr.wrapping_add(1), val as u8);
        // Full word is what the bus saw at cycle end.
        self.last_bus_value.set(val);
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
    fn unmapped_reads_return_last_bus_value() {
        let mut mem = Memory::new(test_rom());
        // Initial state: bus value is 0, so unmapped reads are 0.
        assert_eq!(mem.read_word(0xC0_0000), 0x0000);
        // Drive the bus with a ROM read (first long of test_rom).
        let _ = mem.read_long(0x00FC_0000);
        // Last word driven onto the bus was the low word of
        // $DEAD_BEEF = $BEEF.
        assert_eq!(mem.last_bus_value(), 0xBEEF);
        // Unmapped reads now return the lingering value.
        assert_eq!(mem.read_word(0xA0_0000), 0xBEEF);
        // A write drives the bus too.
        mem.write_word(0x00_0100, 0x1234);
        assert_eq!(mem.read_word(0xE0_0000), 0x1234);
    }

    #[test]
    fn cia_and_custom_writes_via_memory_are_silent() {
        // Memory writes to CIA / custom address ranges don't mutate
        // any backing store — they just drop. (Real CIA and custom
        // writes go through AmigaOcs::service_cpu_bus, which dispatches
        // to the actual chipset. Direct Memory writes are only hit by
        // test backdoors and test that the drop path works.)
        let mut mem = Memory::new(test_rom());
        mem.write_word(0x00BFE001, 0x0203);
        mem.write_word(0x00DFF09A, 0x7FFF);
        // The floating bus still remembers what the CPU drove.
        assert_eq!(mem.last_bus_value(), 0x7FFF);
        // Subsequent unmapped reads see that residue.
        assert_eq!(mem.read_word(0x00BFE000), 0x7FFF);
    }

    #[test]
    fn dma_word_read_updates_floating_bus() {
        // read_chip_ram_word represents Agnus DMA fetches (Denise /
        // Copper). It bypasses OVL and leaves the fetched word on
        // the bus so subsequent unmapped CPU reads see that value.
        let mut mem = Memory::new(test_rom());
        mem.set_overlay(false);
        mem.write_word(0x0000_0200, 0xCAFE);
        let word = mem.read_chip_ram_word(0x0000_0200);
        assert_eq!(word, 0xCAFE);
        assert_eq!(mem.last_bus_value(), 0xCAFE);
        assert_eq!(mem.read_word(0x00A0_0000), 0xCAFE);
    }

    #[test]
    fn chip_ram_sizes_cover_all_agnus_variants() {
        assert!(is_valid_chip_ram_size(256 * 1024));
        assert!(is_valid_chip_ram_size(512 * 1024));
        assert!(is_valid_chip_ram_size(1024 * 1024));
        assert!(is_valid_chip_ram_size(2048 * 1024));
        assert!(!is_valid_chip_ram_size(128 * 1024));
        assert!(!is_valid_chip_ram_size(3 * 512 * 1024));
        assert!(!is_valid_chip_ram_size(4096 * 1024));
    }

    #[test]
    fn slow_ram_sizes_match_a501_family() {
        assert!(is_valid_slow_ram_size(0));
        assert!(is_valid_slow_ram_size(256 * 1024));
        assert!(is_valid_slow_ram_size(512 * 1024));
        assert!(is_valid_slow_ram_size(1024 * 1024));
        assert!(is_valid_slow_ram_size(1536 * 1024));
        assert!(!is_valid_slow_ram_size(2048 * 1024));
    }

    #[test]
    fn new_with_ram_installs_requested_chip_size() {
        let mem = Memory::new_with_ram(test_rom(), 1024 * 1024, 0);
        assert_eq!(mem.chip_ram_size(), 1024 * 1024);
        assert_eq!(mem.slow_ram_size(), 0);
    }

    #[test]
    fn chip_ram_1m_decodes_full_range_without_aliasing() {
        // With 1 MiB of chip RAM installed, writes at $0000 and
        // $80000 must land in distinct bytes (no 19-bit aliasing).
        let mut mem = Memory::new_with_ram(test_rom(), 1024 * 1024, 0);
        mem.set_overlay(false);
        mem.write_byte(0x0000_0000, 0x11);
        mem.write_byte(0x0008_0000, 0x22);
        assert_eq!(mem.read_chip_ram_byte(0x0000_0000), 0x11);
        assert_eq!(mem.read_chip_ram_byte(0x0008_0000), 0x22);
    }

    #[test]
    fn chip_ram_512k_still_aliases_at_eighty_k_boundary() {
        // Stock 512 KiB config: $80000 wraps back to $0.
        let mut mem = Memory::new_with_ram(test_rom(), 512 * 1024, 0);
        mem.set_overlay(false);
        mem.write_byte(0x0000_0000, 0x33);
        assert_eq!(mem.read_chip_ram_byte(0x0008_0000), 0x33);
    }

    #[test]
    #[should_panic(expected = "chip-RAM size must be")]
    fn invalid_chip_ram_size_panics() {
        let _ = Memory::new_with_ram(test_rom(), 128 * 1024, 0);
    }

    #[test]
    #[should_panic(expected = "slow-RAM size must be")]
    fn invalid_slow_ram_size_panics() {
        let _ = Memory::new_with_ram(test_rom(), 512 * 1024, 768 * 1024);
    }

    #[test]
    fn read_chip_ram_byte_is_silent_backdoor() {
        // read_chip_ram_byte is a test-/internal-only peek that does
        // NOT drive the bus. Verifies we don't accidentally corrupt
        // the floating bus state when inspecting chip-RAM contents.
        let mut mem = Memory::new(test_rom());
        mem.set_overlay(false);
        mem.write_word(0x0000_0400, 0x1111);
        // Note the bus now holds $1111.
        assert_eq!(mem.last_bus_value(), 0x1111);
        // Backdoor read — bus stays unchanged.
        let _ = mem.read_chip_ram_byte(0x0000_0400);
        assert_eq!(mem.last_bus_value(), 0x1111);
    }
}
