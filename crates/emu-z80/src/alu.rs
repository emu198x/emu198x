//! ALU operations for the Z80.

#![allow(clippy::cast_possible_truncation)] // Intentional truncation for low byte extraction.
#![allow(clippy::verbose_bit_mask)] // Clearer to read mask comparisons.

use crate::flags::{CF, HF, NF, PF, SF, XF, YF, ZF};

/// Result of an ALU operation with flags.
#[derive(Debug, Clone, Copy)]
pub struct AluResult {
    pub value: u8,
    pub flags: u8,
}

/// Add two bytes with optional carry, returning result and flags.
#[must_use]
pub fn add8(a: u8, b: u8, carry: bool) -> AluResult {
    let c = u8::from(carry);
    let result16 = u16::from(a) + u16::from(b) + u16::from(c);
    let result = result16 as u8;

    let mut flags = 0;

    // Sign flag
    if result & 0x80 != 0 {
        flags |= SF;
    }

    // Zero flag
    if result == 0 {
        flags |= ZF;
    }

    // Undocumented flags (copy bits 5 and 3 from result)
    flags |= result & (YF | XF);

    // Half-carry flag
    if (a & 0x0F) + (b & 0x0F) + c > 0x0F {
        flags |= HF;
    }

    // Overflow flag (both operands same sign, result different sign)
    let overflow = ((a ^ b) & 0x80 == 0) && ((a ^ result) & 0x80 != 0);
    if overflow {
        flags |= PF;
    }

    // Carry flag
    if result16 > 0xFF {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Subtract two bytes with optional borrow, returning result and flags.
#[must_use]
pub fn sub8(a: u8, b: u8, carry: bool) -> AluResult {
    let c = u8::from(carry);
    let result = a.wrapping_sub(b).wrapping_sub(c);

    let mut flags = NF; // Subtract flag always set

    // Sign flag
    if result & 0x80 != 0 {
        flags |= SF;
    }

    // Zero flag
    if result == 0 {
        flags |= ZF;
    }

    // Undocumented flags (copy bits 5 and 3 from result)
    flags |= result & (YF | XF);

    // Half-carry flag (borrow from bit 4)
    if (a & 0x0F) < (b & 0x0F) + c {
        flags |= HF;
    }

    // Overflow flag (operands different sign, result same sign as subtrahend)
    let overflow = ((a ^ b) & 0x80 != 0) && ((b ^ result) & 0x80 == 0);
    if overflow {
        flags |= PF;
    }

    // Carry flag (borrow)
    if u16::from(a) < u16::from(b) + u16::from(c) {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// AND operation.
#[must_use]
pub fn and8(a: u8, b: u8) -> AluResult {
    let result = a & b;

    let mut flags = HF; // H is always set for AND

    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }

    AluResult { value: result, flags }
}

/// OR operation.
#[must_use]
pub fn or8(a: u8, b: u8) -> AluResult {
    let result = a | b;

    let mut flags = 0;

    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }

    AluResult { value: result, flags }
}

/// XOR operation.
#[must_use]
pub fn xor8(a: u8, b: u8) -> AluResult {
    let result = a ^ b;

    let mut flags = 0;

    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }

    AluResult { value: result, flags }
}

/// Compare (subtract without storing result).
#[must_use]
pub fn cp8(a: u8, b: u8) -> AluResult {
    let mut result = sub8(a, b, false);
    // For CP, undocumented flags come from operand, not result
    result.flags = (result.flags & !(YF | XF)) | (b & (YF | XF));
    result
}

/// Increment byte.
#[must_use]
pub fn inc8(a: u8) -> AluResult {
    let result = a.wrapping_add(1);

    let mut flags = 0;

    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if a & 0x0F == 0x0F {
        flags |= HF;
    }
    if a == 0x7F {
        flags |= PF; // Overflow
    }

    AluResult { value: result, flags }
}

/// Decrement byte.
#[must_use]
pub fn dec8(a: u8) -> AluResult {
    let result = a.wrapping_sub(1);

    let mut flags = NF;

    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if a & 0x0F == 0x00 {
        flags |= HF;
    }
    if a == 0x80 {
        flags |= PF; // Overflow
    }

    AluResult { value: result, flags }
}

/// Rotate left circular (bit 7 -> carry and bit 0).
#[must_use]
pub fn rlc8(a: u8) -> AluResult {
    let carry = a >> 7;
    let result = (a << 1) | carry;

    let mut flags = 0;
    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Rotate right circular (bit 0 -> carry and bit 7).
#[must_use]
pub fn rrc8(a: u8) -> AluResult {
    let carry = a & 1;
    let result = (a >> 1) | (carry << 7);

    let mut flags = 0;
    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Rotate left through carry.
#[must_use]
pub fn rl8(a: u8, old_carry: bool) -> AluResult {
    let new_carry = a >> 7;
    let result = (a << 1) | u8::from(old_carry);

    let mut flags = 0;
    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if new_carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Rotate right through carry.
#[must_use]
pub fn rr8(a: u8, old_carry: bool) -> AluResult {
    let new_carry = a & 1;
    let result = (a >> 1) | (u8::from(old_carry) << 7);

    let mut flags = 0;
    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if new_carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Shift left arithmetic (bit 0 = 0).
#[must_use]
pub fn sla8(a: u8) -> AluResult {
    let carry = a >> 7;
    let result = a << 1;

    let mut flags = 0;
    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Shift right arithmetic (bit 7 preserved).
#[must_use]
pub fn sra8(a: u8) -> AluResult {
    let carry = a & 1;
    let result = (a >> 1) | (a & 0x80);

    let mut flags = 0;
    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Shift left logical (undocumented SLL - bit 0 = 1).
#[must_use]
pub fn sll8(a: u8) -> AluResult {
    let carry = a >> 7;
    let result = (a << 1) | 1;

    let mut flags = 0;
    if result & 0x80 != 0 {
        flags |= SF;
    }
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// Shift right logical (bit 7 = 0).
#[must_use]
pub fn srl8(a: u8) -> AluResult {
    let carry = a & 1;
    let result = a >> 1;

    let mut flags = 0;
    if result == 0 {
        flags |= ZF;
    }
    flags |= result & (YF | XF);
    if result.count_ones().is_multiple_of(2) {
        flags |= PF;
    }
    if carry != 0 {
        flags |= CF;
    }

    AluResult { value: result, flags }
}

/// 16-bit add for HL/IX/IY.
#[must_use]
pub fn add16(a: u16, b: u16) -> (u16, u8) {
    let result32 = u32::from(a) + u32::from(b);
    let result = result32 as u16;

    let mut flags = 0;

    // Undocumented flags from high byte of result
    flags |= ((result >> 8) as u8) & (YF | XF);

    // Half-carry from bit 11
    if (a & 0x0FFF) + (b & 0x0FFF) > 0x0FFF {
        flags |= HF;
    }

    // Carry
    if result32 > 0xFFFF {
        flags |= CF;
    }

    (result, flags)
}

/// 16-bit add with carry for HL.
#[must_use]
pub fn adc16(a: u16, b: u16, carry: bool) -> (u16, u8) {
    let c = u16::from(carry);
    let result32 = u32::from(a) + u32::from(b) + u32::from(c);
    let result = result32 as u16;

    let mut flags = 0;

    // Sign flag
    if result & 0x8000 != 0 {
        flags |= SF;
    }

    // Zero flag
    if result == 0 {
        flags |= ZF;
    }

    // Undocumented flags from high byte
    flags |= ((result >> 8) as u8) & (YF | XF);

    // Half-carry from bit 11
    if (a & 0x0FFF) + (b & 0x0FFF) + u16::from(u8::from(carry)) > 0x0FFF {
        flags |= HF;
    }

    // Overflow
    let overflow = ((a ^ b) & 0x8000 == 0) && ((a ^ result) & 0x8000 != 0);
    if overflow {
        flags |= PF;
    }

    // Carry
    if result32 > 0xFFFF {
        flags |= CF;
    }

    (result, flags)
}

/// 16-bit subtract with borrow for HL.
#[must_use]
pub fn sbc16(a: u16, b: u16, carry: bool) -> (u16, u8) {
    let c = u16::from(carry);
    let result = a.wrapping_sub(b).wrapping_sub(c);

    let mut flags = NF;

    // Sign flag
    if result & 0x8000 != 0 {
        flags |= SF;
    }

    // Zero flag
    if result == 0 {
        flags |= ZF;
    }

    // Undocumented flags from high byte
    flags |= ((result >> 8) as u8) & (YF | XF);

    // Half-carry (borrow from bit 12)
    if (a & 0x0FFF) < (b & 0x0FFF) + c {
        flags |= HF;
    }

    // Overflow
    let overflow = ((a ^ b) & 0x8000 != 0) && ((b ^ result) & 0x8000 == 0);
    if overflow {
        flags |= PF;
    }

    // Carry (borrow)
    if u32::from(a) < u32::from(b) + u32::from(c) {
        flags |= CF;
    }

    (result, flags)
}
