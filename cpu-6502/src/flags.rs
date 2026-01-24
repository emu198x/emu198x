//! 6502 status register (P) operations.
//!
//! The status register has the following bits:
//! - Bit 0: C (Carry)
//! - Bit 1: Z (Zero)
//! - Bit 2: I (Interrupt disable)
//! - Bit 3: D (Decimal mode)
//! - Bit 4: B (Break - not a real flag, set on stack by BRK/PHP)
//! - Bit 5: - (Always 1)
//! - Bit 6: V (Overflow)
//! - Bit 7: N (Negative)

use crate::Mos6502;

// Flag bit positions
pub const FLAG_C: u8 = 0; // Carry
pub const FLAG_Z: u8 = 1; // Zero
pub const FLAG_I: u8 = 2; // Interrupt disable
pub const FLAG_D: u8 = 3; // Decimal mode
pub const FLAG_B: u8 = 4; // Break (pseudo-flag)
pub const FLAG_U: u8 = 5; // Unused (always 1)
pub const FLAG_V: u8 = 6; // Overflow
pub const FLAG_N: u8 = 7; // Negative

impl Mos6502 {
    pub(crate) fn get_flag(&self, flag: u8) -> bool {
        (self.p & (1 << flag)) != 0
    }

    pub(crate) fn set_flag(&mut self, flag: u8, value: bool) {
        if value {
            self.p |= 1 << flag;
        } else {
            self.p &= !(1 << flag);
        }
    }

    pub(crate) fn carry(&self) -> bool {
        self.get_flag(FLAG_C)
    }

    pub(crate) fn zero(&self) -> bool {
        self.get_flag(FLAG_Z)
    }

    pub(crate) fn interrupt_disable(&self) -> bool {
        self.get_flag(FLAG_I)
    }

    pub(crate) fn decimal(&self) -> bool {
        self.get_flag(FLAG_D)
    }

    pub(crate) fn overflow(&self) -> bool {
        self.get_flag(FLAG_V)
    }

    pub(crate) fn negative(&self) -> bool {
        self.get_flag(FLAG_N)
    }

    /// Set Zero and Negative flags based on value.
    pub(crate) fn set_zn(&mut self, value: u8) {
        self.set_flag(FLAG_Z, value == 0);
        self.set_flag(FLAG_N, value & 0x80 != 0);
    }

    /// Get status register for pushing (sets B and U bits).
    pub(crate) fn status_for_push(&self, brk: bool) -> u8 {
        let mut p = self.p | (1 << FLAG_U); // Always set bit 5
        if brk {
            p |= 1 << FLAG_B; // Set B flag for BRK/PHP
        }
        p
    }

    /// Set status register from stack (clears B, sets U).
    pub(crate) fn set_status_from_stack(&mut self, value: u8) {
        // B flag is ignored when pulling from stack
        // U flag is always 1
        self.p = (value | (1 << FLAG_U)) & !(1 << FLAG_B);
    }
}
