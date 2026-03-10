//! Commodore Fat Gary address decoder and motherboard resource registers.
//!
//! Fat Gary is the enhanced address decoder on A3000 and A4000 systems.
//! It provides the 24-bit bus forwarding gate, motherboard resource
//! registers at `$DE0000`, and timeout/bus-error generation for accesses
//! to unmapped address ranges.

// ---------------------------------------------------------------------------
// TOENB register bit-fields
// ---------------------------------------------------------------------------

/// Bit 7 of TOENB: when set, accesses to unmapped (nonrange) addresses
/// generate a bus timeout instead of returning floating-bus data.
pub const TOENB_ENABLE: u8 = 0x80;

// ---------------------------------------------------------------------------
// Timeout check result
// ---------------------------------------------------------------------------

/// Outcome of checking a 24-bit address against Fat Gary's decode map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutResult {
    /// Address is in a known-good range (CIA, custom, ROM, chip RAM, etc.).
    /// No timeout — proceed with normal decode.
    Ok,
    /// Address is in Fat Gary's nonrange AND TOENB bit 7 is set.
    /// The bus master should see a bus error / timeout.
    BusTimeout,
    /// Address is in Fat Gary's nonrange but TOENB bit 7 is clear.
    /// Return 0 for reads, sink writes (floating bus convention).
    Unmapped,
}

// ---------------------------------------------------------------------------
// Fat Gary state
// ---------------------------------------------------------------------------

/// Fat Gary address decoder and resource register state.
///
/// Provides three resource registers at `$DE0000` (timeout, toenb,
/// coldboot) and determines whether a 24-bit address is in a valid
/// hardware range or should trigger a bus timeout.
#[derive(Debug, Clone)]
pub struct FatGary {
    toenb: u8,
    timeout: u8,
}

impl FatGary {
    /// Power-on value returned by the cold-boot flag register.
    pub const COLDBOOT_FLAG: u8 = 0x80;
    /// Power-on value of the timeout-enable register.
    pub const DEFAULT_TOENB: u8 = 0x80;
    /// Power-on value of the timeout register.
    pub const DEFAULT_TIMEOUT: u8 = 0x00;

    /// Create a new Fat Gary in power-on state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            toenb: Self::DEFAULT_TOENB,
            timeout: Self::DEFAULT_TIMEOUT,
        }
    }

    /// Reset the chip back to power-on state.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    // -- Accessors ----------------------------------------------------------

    /// Current timeout-enable register value.
    #[must_use]
    pub const fn toenb(&self) -> u8 {
        self.toenb
    }

    /// Current timeout register value.
    #[must_use]
    pub const fn timeout(&self) -> u8 {
        self.timeout
    }

    /// Cold-boot flag value presented to the ROM.
    #[must_use]
    pub const fn coldboot_flag(&self) -> u8 {
        Self::COLDBOOT_FLAG
    }

    /// True when the TOENB enable bit is set (unmapped accesses trigger
    /// bus timeout rather than returning floating-bus data).
    #[must_use]
    pub const fn timeout_enabled(&self) -> bool {
        (self.toenb & TOENB_ENABLE) != 0
    }

    // -- Register writes ----------------------------------------------------

    /// Update the timeout-enable register.
    pub fn write_toenb(&mut self, val: u8) {
        self.toenb = val;
    }

    /// Update the timeout register.
    pub fn write_timeout(&mut self, val: u8) {
        self.timeout = val;
    }

    // -- 24-bit bus gate ----------------------------------------------------

    /// True when Fat Gary forwards the address onto the 24-bit chip bus.
    ///
    /// On A3000/A4000 systems, addresses above 24-bit space are not
    /// forwarded unless they hit a separate fast-RAM window first.
    #[must_use]
    pub const fn forwards_to_24bit_bus(&self, addr: u32) -> bool {
        addr < 0x0100_0000
    }

    // -- Nonrange / timeout -------------------------------------------------

    /// True when the 24-bit address is in a range where no hardware
    /// responds — Fat Gary's "nonrange".
    ///
    /// The nonrange is the complement of known-good decode regions:
    /// chip RAM, CIA, custom registers, ROM, expansion/autoconfig,
    /// and the resource register block itself. Addresses in the nonrange
    /// cause a bus timeout when TOENB is enabled, or return floating-bus
    /// data (0) when TOENB is disabled.
    ///
    /// Reference: WinUAE `gary_nonrange()`.
    #[must_use]
    pub const fn is_nonrange(&self, addr: u32) -> bool {
        let addr = addr & 0xFF_FFFF;

        // Chip RAM: $000000–$1FFFFF — always present.
        if addr < 0x20_0000 {
            return false;
        }

        // CIA-A: $BFE000 block.
        if (addr & 0xFFF000) == 0xBFE000 {
            return false;
        }

        // CIA-B: $BFD000 block.
        if (addr & 0xFFF000) == 0xBFD000 {
            return false;
        }

        // Custom chip registers: $DFF000 block.
        if (addr & 0xFFF000) == 0xDFF000 {
            return false;
        }

        // DMAC: $DD0000–$DDFFFF (A3000).
        if addr >= 0xDD_0000 && addr < 0xDE_0000 {
            return false;
        }

        // Resource registers: $DE0000–$DEFFFF (Fat Gary + Ramsey).
        if addr >= 0xDE_0000 && addr < 0xDF_0000 {
            return false;
        }

        // Autoconfig / Zorro: $E80000–$EFFFFF.
        if addr >= 0xE8_0000 && addr < 0xF0_0000 {
            return false;
        }

        // Kickstart ROM: $F80000–$FFFFFF.
        if addr >= 0xF8_0000 {
            return false;
        }

        // Slow RAM / ranger: $C00000–$DCFFFF. Not nonrange because the
        // address space is decoded even if no hardware responds — the CIA
        // select logic uses the full $BFxxxx range, and $C00000 is the
        // traditional slow-RAM window. Reads return 0 (no expansion) but
        // don't trigger a bus timeout.
        if addr >= 0xC0_0000 && addr < 0xE0_0000 {
            return false;
        }

        // Everything else ($200000–$BFFFFF minus CIAs, $E00000–$E7FFFF,
        // $F00000–$F7FFFF) is nonrange.
        true
    }

    /// Check whether a 24-bit address should trigger a timeout.
    ///
    /// Call this for addresses that did not match any specific chip select
    /// in the bus decode chain. Returns `Ok` if the address is in a
    /// known-good range, `BusTimeout` if TOENB is set and the address is
    /// unmapped, or `Unmapped` if TOENB is clear.
    #[must_use]
    pub const fn check_timeout(&self, addr: u32) -> TimeoutResult {
        if !self.is_nonrange(addr) {
            return TimeoutResult::Ok;
        }
        if self.toenb & TOENB_ENABLE != 0 {
            TimeoutResult::BusTimeout
        } else {
            TimeoutResult::Unmapped
        }
    }

    // -- Resource register I/O ----------------------------------------------

    /// Read one resource-register byte.
    ///
    /// `addr2` is the low two address bits from the `$DExxxx` resource block.
    #[must_use]
    pub const fn read_resource_byte(&self, addr2: u32) -> u8 {
        match addr2 {
            2 => self.coldboot_flag(),
            1 => self.toenb,
            0 => self.timeout,
            _ => 0,
        }
    }

    /// Write one resource-register byte.
    pub fn write_resource_byte(&mut self, addr2: u32, val: u8) {
        match addr2 {
            1 => self.toenb = val,
            0 => self.timeout = val,
            _ => {}
        }
    }
}

impl Default for FatGary {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_restores_power_on_defaults() {
        let mut chip = FatGary::new();
        chip.write_toenb(0x12);
        chip.write_timeout(0x34);

        chip.reset();

        assert_eq!(chip.toenb(), FatGary::DEFAULT_TOENB);
        assert_eq!(chip.timeout(), FatGary::DEFAULT_TIMEOUT);
    }

    #[test]
    fn resource_registers_roundtrip_expected_bytes() {
        let mut chip = FatGary::new();

        assert_eq!(chip.read_resource_byte(2), FatGary::COLDBOOT_FLAG);
        assert_eq!(chip.read_resource_byte(1), FatGary::DEFAULT_TOENB);
        assert_eq!(chip.read_resource_byte(0), FatGary::DEFAULT_TIMEOUT);

        chip.write_resource_byte(1, 0x55);
        chip.write_resource_byte(0, 0xAA);

        assert_eq!(chip.read_resource_byte(1), 0x55);
        assert_eq!(chip.read_resource_byte(0), 0xAA);
    }

    #[test]
    fn coldboot_flag_is_read_only() {
        let mut chip = FatGary::new();
        chip.write_resource_byte(2, 0x00);

        assert_eq!(chip.read_resource_byte(2), FatGary::COLDBOOT_FLAG);
    }

    #[test]
    fn high_addresses_are_not_forwarded_to_24bit_bus() {
        let chip = FatGary::new();

        assert!(chip.forwards_to_24bit_bus(0x00FF_FFFF));
        assert!(!chip.forwards_to_24bit_bus(0x0100_0000));
        assert!(!chip.forwards_to_24bit_bus(0x7E00_0000));
    }

    #[test]
    fn timeout_enabled_reflects_toenb_bit_7() {
        let mut chip = FatGary::new();
        assert!(chip.timeout_enabled()); // default TOENB = $80

        chip.write_toenb(0x00);
        assert!(!chip.timeout_enabled());

        chip.write_toenb(0xFF);
        assert!(chip.timeout_enabled());
    }

    // -- Nonrange tests -----------------------------------------------------

    #[test]
    fn chip_ram_is_not_nonrange() {
        let chip = FatGary::new();
        assert!(!chip.is_nonrange(0x000000));
        assert!(!chip.is_nonrange(0x080000));
        assert!(!chip.is_nonrange(0x1FFFFF));
    }

    #[test]
    fn cia_addresses_are_not_nonrange() {
        let chip = FatGary::new();
        assert!(!chip.is_nonrange(0xBFE001)); // CIA-A
        assert!(!chip.is_nonrange(0xBFD000)); // CIA-B
    }

    #[test]
    fn custom_registers_are_not_nonrange() {
        let chip = FatGary::new();
        assert!(!chip.is_nonrange(0xDFF000));
        assert!(!chip.is_nonrange(0xDFF1FE));
    }

    #[test]
    fn rom_is_not_nonrange() {
        let chip = FatGary::new();
        assert!(!chip.is_nonrange(0xF80000));
        assert!(!chip.is_nonrange(0xFFFFFF));
    }

    #[test]
    fn dmac_and_resource_regs_are_not_nonrange() {
        let chip = FatGary::new();
        assert!(!chip.is_nonrange(0xDD0000)); // DMAC
        assert!(!chip.is_nonrange(0xDE0000)); // Resource
    }

    #[test]
    fn autoconfig_is_not_nonrange() {
        let chip = FatGary::new();
        assert!(!chip.is_nonrange(0xE80000));
        assert!(!chip.is_nonrange(0xEFFFFF));
    }

    #[test]
    fn slow_ram_window_is_not_nonrange() {
        let chip = FatGary::new();
        assert!(!chip.is_nonrange(0xC00000));
        assert!(!chip.is_nonrange(0xD7FFFF));
    }

    #[test]
    fn expansion_gap_is_nonrange() {
        let chip = FatGary::new();
        // $200000–$9FFFFF is nonrange (no hardware here on stock boards).
        assert!(chip.is_nonrange(0x200000));
        assert!(chip.is_nonrange(0x500000));
        assert!(chip.is_nonrange(0x9FFFFF));
    }

    #[test]
    fn ranger_gap_is_nonrange() {
        let chip = FatGary::new();
        // $A00000–$BEFFFF is nonrange (before CIA decode).
        assert!(chip.is_nonrange(0xA00000));
        assert!(chip.is_nonrange(0xB00000));
        // But $BFD000 and $BFE000 are NOT nonrange (CIAs).
        assert!(!chip.is_nonrange(0xBFD000));
        assert!(!chip.is_nonrange(0xBFE000));
    }

    #[test]
    fn diagnostics_rom_gap_is_nonrange() {
        let chip = FatGary::new();
        // $E00000–$E7FFFF and $F00000–$F7FFFF are nonrange.
        assert!(chip.is_nonrange(0xE00000));
        assert!(chip.is_nonrange(0xE7FFFF));
        assert!(chip.is_nonrange(0xF00000));
        assert!(chip.is_nonrange(0xF7FFFF));
    }

    // -- Timeout check tests ------------------------------------------------

    #[test]
    fn check_timeout_returns_ok_for_known_ranges() {
        let chip = FatGary::new();
        assert_eq!(chip.check_timeout(0x000000), TimeoutResult::Ok);
        assert_eq!(chip.check_timeout(0xBFE001), TimeoutResult::Ok);
        assert_eq!(chip.check_timeout(0xDFF000), TimeoutResult::Ok);
        assert_eq!(chip.check_timeout(0xF80000), TimeoutResult::Ok);
    }

    #[test]
    fn check_timeout_returns_bus_timeout_when_toenb_set() {
        let chip = FatGary::new(); // TOENB = $80 (enabled)
        assert_eq!(chip.check_timeout(0x200000), TimeoutResult::BusTimeout);
        assert_eq!(chip.check_timeout(0xA00000), TimeoutResult::BusTimeout);
        assert_eq!(chip.check_timeout(0xF00000), TimeoutResult::BusTimeout);
    }

    #[test]
    fn check_timeout_returns_unmapped_when_toenb_clear() {
        let mut chip = FatGary::new();
        chip.write_toenb(0x00);

        assert_eq!(chip.check_timeout(0x200000), TimeoutResult::Unmapped);
        assert_eq!(chip.check_timeout(0xA00000), TimeoutResult::Unmapped);
    }
}
