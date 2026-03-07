//! Commodore Ramsey DRAM controller resource registers.
//!
//! The current Amiga bring-up only needs the resource-register view the
//! A3000/A4000 ROMs probe during early boot.

/// Ramsey state used by the current machine model.
#[derive(Debug, Clone)]
pub struct Ramsey {
    config: u8,
}

impl Ramsey {
    /// Power-on revision reported by the current stub.
    pub const REVISION: u8 = 0x0D;
    /// Power-on config register value. Bit 3 is the wrap bit the ROM expects.
    pub const DEFAULT_CONFIG: u8 = 0x08;

    /// Create a new Ramsey in power-on state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: Self::DEFAULT_CONFIG,
        }
    }

    /// Reset the chip back to power-on state.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Current RAMSEY config register value.
    #[must_use]
    pub const fn config(&self) -> u8 {
        self.config
    }

    /// Fixed revision value exposed to the ROM.
    #[must_use]
    pub const fn revision(&self) -> u8 {
        Self::REVISION
    }

    /// True when the wrap bit is enabled in the current config.
    #[must_use]
    pub const fn wrap_enabled(&self) -> bool {
        (self.config & 0x08) != 0
    }

    /// Update the config register.
    pub fn write_config(&mut self, val: u8) {
        self.config = val;
    }

    /// Read one resource-register byte.
    ///
    /// `addr64` is `(addr >> 6) & 3`, matching the current machine decode.
    /// `addr2` is `addr & 3`.
    #[must_use]
    pub const fn read_resource_byte(&self, addr64: u32, addr2: u32) -> u8 {
        match (addr64, addr2) {
            (1, 3) => Self::REVISION,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_restores_power_on_defaults() {
        let mut chip = Ramsey::new();
        chip.write_config(0xFF);

        chip.reset();

        assert_eq!(chip.config(), Ramsey::DEFAULT_CONFIG);
        assert_eq!(chip.revision(), Ramsey::REVISION);
    }

    #[test]
    fn resource_register_decode_matches_current_boot_stub() {
        let mut chip = Ramsey::new();

        assert_eq!(chip.read_resource_byte(1, 3), Ramsey::REVISION);
        assert_eq!(chip.read_resource_byte(0, 3), Ramsey::DEFAULT_CONFIG);

        chip.write_resource_byte(0, 3, 0x1C);
        assert_eq!(chip.read_resource_byte(0, 3), 0x1C);
        assert_eq!(chip.read_resource_byte(1, 3), Ramsey::REVISION);
    }

    #[test]
    fn wrap_bit_reflects_config_bit_three() {
        let mut chip = Ramsey::new();

        assert!(chip.wrap_enabled());

        chip.write_config(0x00);
        assert!(!chip.wrap_enabled());
    }

    #[test]
    fn unrelated_resource_addresses_read_as_zero_and_ignore_writes() {
        let mut chip = Ramsey::new();

        assert_eq!(chip.read_resource_byte(0, 0), 0);
        chip.write_resource_byte(1, 0, 0xAA);

        assert_eq!(chip.config(), Ramsey::DEFAULT_CONFIG);
    }
}
