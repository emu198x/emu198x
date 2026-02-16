//! Shared ALU operations with flag computation.
//!
//! These functions perform arithmetic/logic operations and return the result
//! along with the updated status register flags.

use crate::flags::{C, N, V, X, Z};

/// Operation size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// 8-bit byte.
    Byte,
    /// 16-bit word.
    Word,
    /// 32-bit long.
    Long,
}

impl Size {
    /// Get size from the standard 2-bit encoding (00=byte, 01=word, 10=long).
    #[must_use]
    pub fn from_bits(bits: u8) -> Option<Self> {
        match bits & 0x03 {
            0 => Some(Self::Byte),
            1 => Some(Self::Word),
            2 => Some(Self::Long),
            _ => None,
        }
    }

    /// Get size from the MOVE encoding (01=byte, 11=word, 10=long).
    #[must_use]
    pub fn from_move_bits(bits: u8) -> Option<Self> {
        match bits & 0x03 {
            1 => Some(Self::Byte),
            3 => Some(Self::Word),
            2 => Some(Self::Long),
            _ => None,
        }
    }

    /// Number of bytes for this size.
    #[must_use]
    pub const fn bytes(self) -> u32 {
        match self {
            Self::Byte => 1,
            Self::Word => 2,
            Self::Long => 4,
        }
    }

    /// MSB mask for this size.
    #[must_use]
    pub const fn msb_mask(self) -> u32 {
        match self {
            Self::Byte => 0x80,
            Self::Word => 0x8000,
            Self::Long => 0x8000_0000,
        }
    }

    /// Value mask for this size.
    #[must_use]
    pub const fn mask(self) -> u32 {
        match self {
            Self::Byte => 0xFF,
            Self::Word => 0xFFFF,
            Self::Long => 0xFFFF_FFFF,
        }
    }
}

/// Add with flags (used by ADD, ADDI, ADDQ).
///
/// Returns (result, updated_sr).
#[must_use]
pub fn add(src: u32, dst: u32, size: Size, sr: u16) -> (u32, u16) {
    let mask = size.mask();
    let msb = size.msb_mask();
    let s = src & mask;
    let d = dst & mask;
    let result = s.wrapping_add(d) & mask;

    let mut flags = sr & !(C | V | Z | N | X);
    if result == 0 {
        flags |= Z;
    }
    if result & msb != 0 {
        flags |= N;
    }
    // Carry: result < either operand (unsigned overflow)
    let carry = (s & d) | ((s | d) & !result);
    if carry & msb != 0 {
        flags |= C | X;
    }
    // Overflow: both operands same sign, result different sign
    let overflow = (s ^ result) & (d ^ result);
    if overflow & msb != 0 {
        flags |= V;
    }

    (result, flags)
}

/// Subtract with flags (used by SUB, SUBI, SUBQ, CMP, CMPI).
///
/// Computes dst - src. Returns (result, updated_sr).
#[must_use]
pub fn sub(src: u32, dst: u32, size: Size, sr: u16) -> (u32, u16) {
    let mask = size.mask();
    let msb = size.msb_mask();
    let s = src & mask;
    let d = dst & mask;
    let result = d.wrapping_sub(s) & mask;

    let mut flags = sr & !(C | V | Z | N | X);
    if result == 0 {
        flags |= Z;
    }
    if result & msb != 0 {
        flags |= N;
    }
    // Borrow
    let borrow = (!d & s) | ((!d | s) & result);
    if borrow & msb != 0 {
        flags |= C | X;
    }
    // Overflow: operands different sign, result sign matches src
    let overflow = (s ^ d) & (result ^ d);
    if overflow & msb != 0 {
        flags |= V;
    }

    (result, flags)
}

/// Add with extend (ADDX): dst + src + X flag.
///
/// Z flag is only cleared, never set (for multi-precision).
#[must_use]
pub fn addx(src: u32, dst: u32, size: Size, sr: u16) -> (u32, u16) {
    let mask = size.mask();
    let msb = size.msb_mask();
    let x_in = u32::from(sr & X != 0);
    let s = src & mask;
    let d = dst & mask;
    let result = s.wrapping_add(d).wrapping_add(x_in) & mask;

    let mut flags = sr & !(C | V | N | X); // Z not cleared here — only cleared if result != 0
    if result != 0 {
        flags &= !Z; // Clear Z if non-zero
    }
    if result & msb != 0 {
        flags |= N;
    }
    let carry = (s & d) | ((s | d) & !result);
    if carry & msb != 0 {
        flags |= C | X;
    }
    let overflow = (s ^ result) & (d ^ result);
    if overflow & msb != 0 {
        flags |= V;
    }

    (result, flags)
}

/// Subtract with extend (SUBX): dst - src - X flag.
///
/// Z flag is only cleared, never set.
#[must_use]
pub fn subx(src: u32, dst: u32, size: Size, sr: u16) -> (u32, u16) {
    let mask = size.mask();
    let msb = size.msb_mask();
    let x_in = u32::from(sr & X != 0);
    let s = src & mask;
    let d = dst & mask;
    let result = d.wrapping_sub(s).wrapping_sub(x_in) & mask;

    let mut flags = sr & !(C | V | N | X);
    if result != 0 {
        flags &= !Z;
    }
    if result & msb != 0 {
        flags |= N;
    }
    let borrow = (!d & s) | ((!d | s) & result);
    if borrow & msb != 0 {
        flags |= C | X;
    }
    let overflow = (s ^ d) & (result ^ d);
    if overflow & msb != 0 {
        flags |= V;
    }

    (result, flags)
}

/// Negate with flags (NEG): 0 - dst.
#[must_use]
pub fn neg(dst: u32, size: Size, sr: u16) -> (u32, u16) {
    sub(dst, 0, size, sr)
}

/// Negate with extend (NEGX): 0 - dst - X.
#[must_use]
pub fn negx(dst: u32, size: Size, sr: u16) -> (u32, u16) {
    subx(dst, 0, size, sr)
}
