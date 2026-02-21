//! Z80 flag register bits.

// These will be used when instructions are implemented.
#![allow(dead_code)]

/// Sign flag (bit 7) - set if result is negative.
pub const SF: u8 = 0b1000_0000;

/// Zero flag (bit 6) - set if result is zero.
pub const ZF: u8 = 0b0100_0000;

/// Undocumented flag (bit 5) - copy of bit 5 of result.
pub const YF: u8 = 0b0010_0000;

/// Half-carry flag (bit 4) - carry from bit 3 to bit 4.
pub const HF: u8 = 0b0001_0000;

/// Undocumented flag (bit 3) - copy of bit 3 of result.
pub const XF: u8 = 0b0000_1000;

/// Parity/Overflow flag (bit 2) - parity or overflow depending on instruction.
pub const PF: u8 = 0b0000_0100;

/// Add/Subtract flag (bit 1) - set if last operation was subtraction.
pub const NF: u8 = 0b0000_0010;

/// Carry flag (bit 0) - carry out of bit 7.
pub const CF: u8 = 0b0000_0001;

/// Compute parity of a byte (true if even number of 1 bits).
#[must_use]
pub const fn parity(value: u8) -> bool {
    value.count_ones().is_multiple_of(2)
}

/// Build flags byte for common arithmetic results.
#[must_use]
pub const fn sz53(value: u8) -> u8 {
    let mut f = 0;
    if value == 0 {
        f |= ZF;
    }
    if value & 0x80 != 0 {
        f |= SF;
    }
    // Copy bits 5 and 3 from value (undocumented flags)
    f |= value & (YF | XF);
    f
}

/// Build flags byte with parity.
#[must_use]
pub const fn sz53p(value: u8) -> u8 {
    let mut f = sz53(value);
    if parity(value) {
        f |= PF;
    }
    f
}
