//! Motorola 68000 status register flags.
//!
//! The status register is 16 bits:
//! - Bits 0-4: Condition code register (CCR)
//!   - C (bit 0): Carry
//!   - V (bit 1): Overflow
//!   - Z (bit 2): Zero
//!   - N (bit 3): Negative
//!   - X (bit 4): Extend (copy of C for multi-precision arithmetic)
//! - Bits 5-7: Reserved (always 0)
//! - Bits 8-10: Interrupt mask (I0, I1, I2)
//! - Bits 11-12: Reserved (always 0)
//! - Bit 13: Supervisor mode (S)
//! - Bit 14: Reserved (always 0)
//! - Bit 15: Trace mode (T)

/// Carry flag.
pub const C: u16 = 0x0001;
/// Overflow flag.
pub const V: u16 = 0x0002;
/// Zero flag.
pub const Z: u16 = 0x0004;
/// Negative flag.
pub const N: u16 = 0x0008;
/// Extend flag.
pub const X: u16 = 0x0010;

/// Interrupt mask bit 0.
pub const I0: u16 = 0x0100;
/// Interrupt mask bit 1.
pub const I1: u16 = 0x0200;
/// Interrupt mask bit 2.
pub const I2: u16 = 0x0400;

/// Supervisor mode flag.
pub const S: u16 = 0x2000;
/// Trace mode flag.
pub const T: u16 = 0x8000;

/// Mask for condition codes only (bits 0-4).
pub const CCR_MASK: u16 = 0x001F;
/// Mask for the system byte (bits 8-15).
pub const SYSTEM_MASK: u16 = 0xFF00;
/// Mask for valid SR bits (excluding reserved bits).
pub const SR_MASK: u16 = 0xA71F;

/// Status register helper functions.
pub struct Status;

impl Status {
    /// Update N and Z flags based on a byte value.
    #[must_use]
    pub fn update_nz_byte(sr: u16, value: u8) -> u16 {
        let mut result = sr & !(N | Z);
        if value == 0 {
            result |= Z;
        }
        if value & 0x80 != 0 {
            result |= N;
        }
        result
    }

    /// Update N and Z flags based on a word value.
    #[must_use]
    pub fn update_nz_word(sr: u16, value: u16) -> u16 {
        let mut result = sr & !(N | Z);
        if value == 0 {
            result |= Z;
        }
        if value & 0x8000 != 0 {
            result |= N;
        }
        result
    }

    /// Update N and Z flags based on a long value.
    #[must_use]
    pub fn update_nz_long(sr: u16, value: u32) -> u16 {
        let mut result = sr & !(N | Z);
        if value == 0 {
            result |= Z;
        }
        if value & 0x8000_0000 != 0 {
            result |= N;
        }
        result
    }

    /// Clear V and C flags (used by MOVE, AND, OR, EOR, etc).
    #[must_use]
    pub fn clear_vc(sr: u16) -> u16 {
        sr & !(V | C)
    }

    /// Set a flag if condition is true, clear if false.
    #[must_use]
    pub fn set_if(sr: u16, flag: u16, condition: bool) -> u16 {
        if condition {
            sr | flag
        } else {
            sr & !flag
        }
    }

    /// Evaluate a condition code (0-15).
    #[must_use]
    pub fn condition(sr: u16, cc: u8) -> bool {
        match cc & 0x0F {
            0x0 => true,                                    // T (true)
            0x1 => false,                                   // F (false)
            0x2 => (sr & C) == 0 && (sr & Z) == 0,          // HI (high)
            0x3 => (sr & C) != 0 || (sr & Z) != 0,          // LS (low or same)
            0x4 => (sr & C) == 0,                           // CC/HS (carry clear)
            0x5 => (sr & C) != 0,                           // CS/LO (carry set)
            0x6 => (sr & Z) == 0,                           // NE (not equal)
            0x7 => (sr & Z) != 0,                           // EQ (equal)
            0x8 => (sr & V) == 0,                           // VC (overflow clear)
            0x9 => (sr & V) != 0,                           // VS (overflow set)
            0xA => (sr & N) == 0,                           // PL (plus)
            0xB => (sr & N) != 0,                           // MI (minus)
            0xC => {
                // GE: N and V same
                let n = (sr & N) != 0;
                let v = (sr & V) != 0;
                n == v
            }
            0xD => {
                // LT: N and V different
                let n = (sr & N) != 0;
                let v = (sr & V) != 0;
                n != v
            }
            0xE => {
                // GT: Z clear and (N == V)
                let z = (sr & Z) != 0;
                let n = (sr & N) != 0;
                let v = (sr & V) != 0;
                !z && (n == v)
            }
            0xF => {
                // LE: Z set or (N != V)
                let z = (sr & Z) != 0;
                let n = (sr & N) != 0;
                let v = (sr & V) != 0;
                z || (n != v)
            }
            _ => unreachable!(),
        }
    }
}
