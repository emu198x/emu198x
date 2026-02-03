//! 6502 processor status register (P).
//!
//! The status register contains flags that reflect the result of operations
//! and control CPU behavior.

/// Carry flag - set if operation resulted in carry/borrow.
pub const C: u8 = 0x01;

/// Zero flag - set if result is zero.
pub const Z: u8 = 0x02;

/// Interrupt disable - when set, IRQ interrupts are ignored.
pub const I: u8 = 0x04;

/// Decimal mode - enables BCD arithmetic for ADC/SBC.
pub const D: u8 = 0x08;

/// Break flag - not a real flag, only appears when status is pushed.
/// Set when BRK pushes status, clear when IRQ/NMI pushes status.
pub const B: u8 = 0x10;

/// Unused bit - always reads as 1.
pub const U: u8 = 0x20;

/// Overflow flag - set if signed arithmetic overflowed.
pub const V: u8 = 0x40;

/// Negative flag - set if result has bit 7 set.
pub const N: u8 = 0x80;

/// Processor status register.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Status(pub u8);

impl Status {
    /// Create a new status register with the unused bit set.
    #[must_use]
    pub const fn new() -> Self {
        Self(U)
    }

    /// Create status from raw value, ensuring unused bit is set.
    #[must_use]
    pub const fn from_byte(value: u8) -> Self {
        Self(value | U)
    }

    /// Get raw value with unused bit set and break clear (for RTI).
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        (self.0 | U) & !B
    }

    /// Get raw value for BRK/PHP (break and unused both set).
    #[must_use]
    pub const fn to_byte_brk(self) -> u8 {
        self.0 | U | B
    }

    /// Get raw value for IRQ/NMI (unused set, break clear).
    #[must_use]
    pub const fn to_byte_irq(self) -> u8 {
        (self.0 | U) & !B
    }

    /// Check if a flag is set.
    #[must_use]
    pub const fn is_set(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    /// Set a flag.
    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }

    /// Clear a flag.
    pub fn clear(&mut self, flag: u8) {
        self.0 &= !flag;
    }

    /// Set or clear a flag based on condition.
    pub fn set_if(&mut self, flag: u8, condition: bool) {
        if condition {
            self.set(flag);
        } else {
            self.clear(flag);
        }
    }

    /// Update N and Z flags based on a value.
    pub fn update_nz(&mut self, value: u8) {
        self.set_if(N, value & 0x80 != 0);
        self.set_if(Z, value == 0);
    }
}
