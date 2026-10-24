//! ALU helpers for 8-bit add/sub/logical operations and INC/DEC flag
//! semantics.
//!
//! The SM83 ALU group (`$80..$BF` and the `$C6/$CE/$D6/$DE/$E6/$EE/$F6/$FE`
//! immediate variants) selects an operation from the three middle bits
//! of the opcode. INC/DEC share the same flag rules as ADD/SUB with
//! carry preserved, so those live here too.

use crate::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z, Sm83};

/// 8-bit ALU operation selected by bits 5-3 of the opcode:
/// `000=ADD`, `001=ADC`, `010=SUB`, `011=SBC`, `100=AND`, `101=XOR`,
/// `110=OR`, `111=CP`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AluOp {
    Add,
    Adc,
    Sub,
    Sbc,
    And,
    Xor,
    Or,
    Cp,
}

impl AluOp {
    pub(crate) const fn from_opcode_bits(opcode: u8) -> Self {
        match (opcode >> 3) & 0b111 {
            0 => Self::Add,
            1 => Self::Adc,
            2 => Self::Sub,
            3 => Self::Sbc,
            4 => Self::And,
            5 => Self::Xor,
            6 => Self::Or,
            _ => Self::Cp,
        }
    }
}

/// Compute `SP + sign-extended r8` and the resulting flags. Shared
/// between `ADD SP, r8` ($E8) and `LD HL, SP+r8` ($F8); both ops
/// clear Z and N, derive H from the low-nibble carry of the low byte
/// of SP, and derive C from the carry out of bit 7 of the low byte
/// (i.e. unsigned 8-bit math, not 16-bit) — that's the documented
/// Game Boy behaviour and what Blargg `instr_timing` expects.
pub(crate) fn sp_add_offset(sp: u16, offset: u8) -> (u16, u8) {
    let signed = i16::from(offset as i8);
    let result = (sp as i32).wrapping_add(i32::from(signed)) as u16;

    let mut flags = 0u8;
    if (sp & 0xF) + (u16::from(offset) & 0xF) > 0xF {
        flags |= FLAG_H;
    }
    if (sp & 0xFF) + u16::from(offset) > 0xFF {
        flags |= FLAG_C;
    }
    (result, flags)
}

impl Sm83 {
    /// Apply an ALU operation with `operand` against the accumulator,
    /// updating flags per the standard SM83 rules and (for non-CP ops)
    /// writing the result back to `A`.
    pub(crate) fn alu(&mut self, op: AluOp, operand: u8) {
        match op {
            AluOp::Add => self.alu_add(operand, false),
            AluOp::Adc => self.alu_add(operand, true),
            AluOp::Sub => self.alu_sub(operand, false, true),
            AluOp::Sbc => self.alu_sub(operand, true, true),
            AluOp::And => {
                self.a &= operand;
                self.f = FLAG_H | if self.a == 0 { FLAG_Z } else { 0 };
            }
            AluOp::Xor => {
                self.a ^= operand;
                self.f = if self.a == 0 { FLAG_Z } else { 0 };
            }
            AluOp::Or => {
                self.a |= operand;
                self.f = if self.a == 0 { FLAG_Z } else { 0 };
            }
            AluOp::Cp => self.alu_sub(operand, false, false),
        }
    }

    fn alu_add(&mut self, operand: u8, with_carry: bool) {
        let carry_in: u16 = if with_carry && (self.f & FLAG_C) != 0 {
            1
        } else {
            0
        };
        let a = u16::from(self.a);
        let b = u16::from(operand);
        let result = a + b + carry_in;
        let half = (a & 0xF) + (b & 0xF) + carry_in;

        let mut flags = 0u8;
        if (result as u8) == 0 {
            flags |= FLAG_Z;
        }
        if half > 0xF {
            flags |= FLAG_H;
        }
        if result > 0xFF {
            flags |= FLAG_C;
        }
        self.a = result as u8;
        self.f = flags;
    }

    fn alu_sub(&mut self, operand: u8, with_carry: bool, store: bool) {
        let carry_in: u16 = if with_carry && (self.f & FLAG_C) != 0 {
            1
        } else {
            0
        };
        let a = u16::from(self.a);
        let b = u16::from(operand);
        let result = a.wrapping_sub(b).wrapping_sub(carry_in);
        let half = (a & 0xF).wrapping_sub(b & 0xF).wrapping_sub(carry_in);

        let mut flags = FLAG_N;
        if (result as u8) == 0 {
            flags |= FLAG_Z;
        }
        if (half & 0x10) != 0 {
            flags |= FLAG_H;
        }
        if (result & 0x100) != 0 {
            flags |= FLAG_C;
        }
        if store {
            self.a = result as u8;
        }
        self.f = flags;
    }

    /// Increment an 8-bit value, returning the result. Updates Z, N=0,
    /// H based on the pre-increment low nibble; preserves C.
    pub(crate) fn alu_inc8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        let mut flags = self.f & FLAG_C;
        if result == 0 {
            flags |= FLAG_Z;
        }
        if (value & 0xF) == 0xF {
            flags |= FLAG_H;
        }
        self.f = flags;
        result
    }

    /// Decrement an 8-bit value, returning the result. Updates Z, N=1,
    /// H based on the pre-decrement low nibble; preserves C.
    pub(crate) fn alu_dec8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        let mut flags = (self.f & FLAG_C) | FLAG_N;
        if result == 0 {
            flags |= FLAG_Z;
        }
        if (value & 0xF) == 0 {
            flags |= FLAG_H;
        }
        self.f = flags;
        result
    }

    /// Test whether the 2-bit condition code encoded at bits 4-3 of the
    /// current opcode holds. Used by `JR cc`, `JP cc`, `CALL cc`,
    /// `RET cc`.
    #[inline]
    pub(crate) fn condition_met(&self) -> bool {
        match (self.opcode >> 3) & 0b11 {
            0 => (self.f & FLAG_Z) == 0, // NZ
            1 => (self.f & FLAG_Z) != 0, // Z
            2 => (self.f & FLAG_C) == 0, // NC
            _ => (self.f & FLAG_C) != 0, // C
        }
    }

    /// Decimal Adjust A. Re-aligns the accumulator after a binary ADD
    /// or SUB on BCD-packed nibbles. Behaviour follows the standard
    /// SM83 flag rules: Z reflects the post-adjust A value, N is
    /// preserved, H is cleared, C is set when the high adjust is
    /// applied.
    pub(crate) fn daa(&mut self) {
        let n = (self.f & FLAG_N) != 0;
        let h = (self.f & FLAG_H) != 0;
        let c = (self.f & FLAG_C) != 0;

        let mut adjust = 0u8;
        let mut new_carry = false;

        if h || (!n && (self.a & 0x0F) > 9) {
            adjust |= 0x06;
        }
        if c || (!n && self.a > 0x99) {
            adjust |= 0x60;
            new_carry = true;
        }

        self.a = if n {
            self.a.wrapping_sub(adjust)
        } else {
            self.a.wrapping_add(adjust)
        };

        let mut flags = self.f & FLAG_N;
        if new_carry {
            flags |= FLAG_C;
        }
        if self.a == 0 {
            flags |= FLAG_Z;
        }
        self.f = flags;
    }
}
