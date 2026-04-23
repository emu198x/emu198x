//! CB-prefixed instruction sub-table.
//!
//! All 256 `$CB xx` opcodes share the same shape: bits 7-6 select the
//! family (`00=rotate/shift`, `01=BIT`, `10=RES`, `11=SET`), bits 5-3
//! select the bit / rotate variant, bits 2-0 select the operand
//! register or `(HL)`. The dispatch in [`crate::opcodes`] handles the
//! m-cycle plumbing; this module owns the pure register-side
//! computation.

use crate::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z, Sm83};

/// Top-level CB family selected by the sub-opcode's bits 7-6.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CbFamily {
    RotShift,
    Bit,
    Res,
    Set,
}

impl CbFamily {
    pub(crate) const fn from_cb_opcode(cb_op: u8) -> Self {
        match cb_op >> 6 {
            0 => Self::RotShift,
            1 => Self::Bit,
            2 => Self::Res,
            _ => Self::Set,
        }
    }
}

impl Sm83 {
    /// Execute a CB-prefixed instruction whose operand is a register
    /// (i.e. the operand index is not `(HL)`). 1-m-cycle work after
    /// the prefix + sub-opcode fetch.
    pub(crate) fn cb_execute_reg(&mut self, cb_op: u8) {
        let reg = cb_op & 0b111;
        let value = self.read_reg8(reg);

        match CbFamily::from_cb_opcode(cb_op) {
            CbFamily::RotShift => {
                let result = self.cb_rot_shift(cb_op, value);
                self.write_reg8(reg, result);
            }
            CbFamily::Bit => self.cb_bit_test(cb_op, value),
            CbFamily::Res => {
                let bit = (cb_op >> 3) & 0b111;
                self.write_reg8(reg, value & !(1 << bit));
            }
            CbFamily::Set => {
                let bit = (cb_op >> 3) & 0b111;
                self.write_reg8(reg, value | (1 << bit));
            }
        }
    }

    /// Apply a CB modification to a memory operand byte. Used by
    /// `(HL)`-flavoured CB ops other than `BIT b, (HL)` (which is a
    /// pure test and handled in the dispatch directly).
    pub(crate) fn cb_modify(&mut self, cb_op: u8, value: u8) -> u8 {
        match CbFamily::from_cb_opcode(cb_op) {
            CbFamily::RotShift => self.cb_rot_shift(cb_op, value),
            CbFamily::Bit => {
                debug_assert!(false, "BIT b,(HL) handled inline by dispatch");
                value
            }
            CbFamily::Res => {
                let bit = (cb_op >> 3) & 0b111;
                value & !(1 << bit)
            }
            CbFamily::Set => {
                let bit = (cb_op >> 3) & 0b111;
                value | (1 << bit)
            }
        }
    }

    /// `BIT b, r` / `BIT b, (HL)` — non-destructive bit test.
    /// Sets `Z` when the tested bit is clear, sets `H`, clears `N`,
    /// preserves `C`.
    pub(crate) fn cb_bit_test(&mut self, cb_op: u8, value: u8) {
        let bit = (cb_op >> 3) & 0b111;
        let mut flags = (self.f & FLAG_C) | FLAG_H;
        if (value & (1 << bit)) == 0 {
            flags |= FLAG_Z;
        }
        // N is intentionally cleared (mask above).
        let _ = FLAG_N;
        self.f = flags;
    }

    /// CB rotate / shift (`$00..$3F` sub-opcodes). Operation chosen by
    /// bits 5-3 of the sub-opcode. Sets `Z` when the result is zero,
    /// clears `N` and `H`, and stores the bit shifted out in `C`.
    fn cb_rot_shift(&mut self, cb_op: u8, value: u8) -> u8 {
        let op = (cb_op >> 3) & 0b111;
        let (result, carry_out) = match op {
            0 => {
                // RLC
                let carry = value >> 7;
                ((value << 1) | carry, carry != 0)
            }
            1 => {
                // RRC
                let carry = value & 1;
                ((value >> 1) | (carry << 7), carry != 0)
            }
            2 => {
                // RL
                let carry_in = if (self.f & FLAG_C) != 0 { 1 } else { 0 };
                let carry_out = (value & 0x80) != 0;
                ((value << 1) | carry_in, carry_out)
            }
            3 => {
                // RR
                let carry_in: u8 = if (self.f & FLAG_C) != 0 { 0x80 } else { 0 };
                let carry_out = (value & 1) != 0;
                ((value >> 1) | carry_in, carry_out)
            }
            4 => {
                // SLA
                let carry_out = (value & 0x80) != 0;
                (value << 1, carry_out)
            }
            5 => {
                // SRA — arithmetic right shift, bit 7 preserved.
                let carry_out = (value & 1) != 0;
                ((value >> 1) | (value & 0x80), carry_out)
            }
            6 => {
                // SWAP — exchange nibbles, C cleared.
                (value.rotate_left(4), false)
            }
            _ => {
                // SRL — logical right shift, bit 7 cleared.
                let carry_out = (value & 1) != 0;
                (value >> 1, carry_out)
            }
        };

        let mut flags = 0u8;
        if result == 0 {
            flags |= FLAG_Z;
        }
        if carry_out {
            flags |= FLAG_C;
        }
        self.f = flags;
        result
    }
}
