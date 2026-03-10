//! Commodore Ramsey DRAM controller.
//!
//! Ramsey manages the motherboard DRAM on A3000 and A4000 systems.
//! It exposes resource registers at `$DE0000` that the Kickstart ROM
//! probes during early boot, plus control bits for page mode, burst,
//! refresh rate, and memory-size detection (wrap bit).

// ---------------------------------------------------------------------------
// Config register bit-fields
// ---------------------------------------------------------------------------

/// Bit 0: DRAM page mode enable.
pub const CFG_PAGE_MODE: u8 = 0x01;
/// Bit 1: Static column mode enable.
pub const CFG_STATIC_COL: u8 = 0x02;
/// Bit 2: Refresh rate select (0 = ~238 clocks, 1 = ~380 clocks).
pub const CFG_REFRESH: u8 = 0x04;
/// Bit 3: Address wrap bit — used by exec for memory-size detection.
pub const CFG_WRAP: u8 = 0x08;
/// Bit 4: Burst mode enable.
pub const CFG_BURST: u8 = 0x10;
/// Bit 5: DRAM type (0 = 1M×1, 1 = 256K×4).
pub const CFG_256K_X4: u8 = 0x20;
/// Bit 6: Factory test mode.
pub const CFG_TEST: u8 = 0x40;
/// Bit 7: Bus timeout enable.
pub const CFG_TIMEOUT: u8 = 0x80;

// ---------------------------------------------------------------------------
// Revision IDs
// ---------------------------------------------------------------------------

/// Ramsey-04 revision (A3000 rev 6.x boards).
pub const REVISION_04: u8 = 0x0D;
/// Ramsey-07 revision (A3000 rev 9.x boards, A4000).
pub const REVISION_07: u8 = 0x0F;

// ---------------------------------------------------------------------------
// Ramsey state
// ---------------------------------------------------------------------------

/// Ramsey DRAM controller state.
///
/// Exposes a config register (read/write) and a revision register
/// (read-only) through the motherboard resource block at `$DE0000`.
#[derive(Debug, Clone)]
pub struct Ramsey {
    config: u8,
    revision: u8,
}

impl Ramsey {
    /// Default power-on config: wrap bit set, all others clear.
    pub const DEFAULT_CONFIG: u8 = CFG_WRAP;

    /// Create a Ramsey-04 (default A3000) in power-on state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: Self::DEFAULT_CONFIG,
            revision: REVISION_04,
        }
    }

    /// Create a Ramsey with a specific revision ID.
    #[must_use]
    pub fn with_revision(revision: u8) -> Self {
        Self {
            config: Self::DEFAULT_CONFIG,
            revision,
        }
    }

    /// Reset the chip back to power-on state (preserves revision).
    pub fn reset(&mut self) {
        self.config = Self::DEFAULT_CONFIG;
    }

    // -- Accessors ----------------------------------------------------------

    /// Current config register value.
    #[must_use]
    pub const fn config(&self) -> u8 {
        self.config
    }

    /// Revision ID exposed to the ROM.
    #[must_use]
    pub const fn revision(&self) -> u8 {
        self.revision
    }

    /// True when the wrap bit is set (used for memory-size detection).
    #[must_use]
    pub const fn wrap_enabled(&self) -> bool {
        (self.config & CFG_WRAP) != 0
    }

    /// True when DRAM page mode is enabled.
    #[must_use]
    pub const fn page_mode_enabled(&self) -> bool {
        (self.config & CFG_PAGE_MODE) != 0
    }

    /// True when static column mode is enabled.
    #[must_use]
    pub const fn static_column_enabled(&self) -> bool {
        (self.config & CFG_STATIC_COL) != 0
    }

    /// True when burst mode is enabled.
    #[must_use]
    pub const fn burst_enabled(&self) -> bool {
        (self.config & CFG_BURST) != 0
    }

    /// True when the bus timeout bit is set.
    #[must_use]
    pub const fn timeout_enabled(&self) -> bool {
        (self.config & CFG_TIMEOUT) != 0
    }

    /// True when the refresh rate is set to the slower (~380 clock) rate.
    #[must_use]
    pub const fn slow_refresh(&self) -> bool {
        (self.config & CFG_REFRESH) != 0
    }

    /// True when the DRAM type is 256K×4 (vs 1M×1).
    #[must_use]
    pub const fn dram_256k_x4(&self) -> bool {
        (self.config & CFG_256K_X4) != 0
    }

    // -- Register I/O -------------------------------------------------------

    /// Update the config register.
    pub fn write_config(&mut self, val: u8) {
        self.config = val;
    }

    /// Read one resource-register byte.
    ///
    /// `addr64` is `(addr >> 6) & 3`, `addr2` is `addr & 3` — matching the
    /// address decode in the motherboard resource block at `$DE0000`.
    #[must_use]
    pub const fn read_resource_byte(&self, addr64: u32, addr2: u32) -> u8 {
        match (addr64, addr2) {
            (1, 3) => self.revision,
            (0, 3) => self.config,
            _ => 0,
        }
    }

    /// Write one resource-register byte.
    pub fn write_resource_byte(&mut self, addr64: u32, addr2: u32, val: u8) {
        if let (0, 3) = (addr64, addr2) {
            self.config = val;
        }
    }
}

impl Default for Ramsey {
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
    fn power_on_defaults() {
        let chip = Ramsey::new();
        assert_eq!(chip.config(), CFG_WRAP);
        assert_eq!(chip.revision(), REVISION_04);
        assert!(chip.wrap_enabled());
        assert!(!chip.page_mode_enabled());
        assert!(!chip.burst_enabled());
        assert!(!chip.timeout_enabled());
    }

    #[test]
    fn reset_restores_config_but_preserves_revision() {
        let mut chip = Ramsey::with_revision(REVISION_07);
        chip.write_config(0xFF);

        chip.reset();

        assert_eq!(chip.config(), Ramsey::DEFAULT_CONFIG);
        assert_eq!(chip.revision(), REVISION_07);
    }

    #[test]
    fn with_revision_sets_revision() {
        let chip = Ramsey::with_revision(REVISION_07);
        assert_eq!(chip.revision(), REVISION_07);
    }

    #[test]
    fn resource_register_decode() {
        let mut chip = Ramsey::new();

        assert_eq!(chip.read_resource_byte(1, 3), REVISION_04);
        assert_eq!(chip.read_resource_byte(0, 3), Ramsey::DEFAULT_CONFIG);

        chip.write_resource_byte(0, 3, 0x1C);
        assert_eq!(chip.read_resource_byte(0, 3), 0x1C);
        assert_eq!(chip.read_resource_byte(1, 3), REVISION_04);
    }

    #[test]
    fn config_bit_field_accessors() {
        let mut chip = Ramsey::new();

        chip.write_config(CFG_PAGE_MODE | CFG_BURST | CFG_WRAP);
        assert!(chip.page_mode_enabled());
        assert!(chip.burst_enabled());
        assert!(chip.wrap_enabled());
        assert!(!chip.static_column_enabled());
        assert!(!chip.timeout_enabled());
        assert!(!chip.slow_refresh());
        assert!(!chip.dram_256k_x4());

        chip.write_config(CFG_STATIC_COL | CFG_REFRESH | CFG_256K_X4 | CFG_TIMEOUT);
        assert!(chip.static_column_enabled());
        assert!(chip.slow_refresh());
        assert!(chip.dram_256k_x4());
        assert!(chip.timeout_enabled());
        assert!(!chip.page_mode_enabled());
        assert!(!chip.burst_enabled());
        assert!(!chip.wrap_enabled());
    }

    #[test]
    fn unrelated_resource_addresses_read_as_zero_and_ignore_writes() {
        let mut chip = Ramsey::new();

        assert_eq!(chip.read_resource_byte(0, 0), 0);
        chip.write_resource_byte(1, 0, 0xAA);

        assert_eq!(chip.config(), Ramsey::DEFAULT_CONFIG);
    }

    #[test]
    fn revision_register_is_read_only_via_resource_byte() {
        let mut chip = Ramsey::new();
        chip.write_resource_byte(1, 3, 0xFF);
        assert_eq!(chip.revision(), REVISION_04);
    }
}
