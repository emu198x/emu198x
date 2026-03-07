//! Commodore Fat Gary address decoder and motherboard resource registers.
//!
//! For the current Amiga bring-up this crate models the small resource register
//! surface the A3000/A4000 ROMs touch during early boot, plus the coarse
//! 24-bit forwarding gate used by motherboard fast-RAM machines.

/// Fat Gary state used by the current machine model.
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

    /// Update the timeout-enable register.
    pub fn write_toenb(&mut self, val: u8) {
        self.toenb = val;
    }

    /// Update the timeout register.
    pub fn write_timeout(&mut self, val: u8) {
        self.timeout = val;
    }

    /// True when Fat Gary forwards the address onto the 24-bit chip bus.
    ///
    /// On A3000/A4000 systems, addresses above 24-bit space are not forwarded
    /// unless they hit a separate fast-RAM window first.
    #[must_use]
    pub const fn forwards_to_24bit_bus(&self, addr: u32) -> bool {
        addr < 0x0100_0000
    }

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
    ///
    /// The current boot path only uses `TOENB` and `TIMEOUT`; other writes are
    /// ignored.
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
}
