//! The arithmetic and logic operations, at 8 and 16 bits.
//!
//! Re-expressed from ares's `algorithms.cpp` (ISC licence), whose decimal
//! adjust matches the 65C816's: the overflow flag is taken from the binary
//! sum before the final adjustment.

use crate::Wdc65c816;
use crate::registers::{FLAG_C, FLAG_D, FLAG_N, FLAG_V, FLAG_Z};

/// An operation applied to a value read from memory, or to a register for
/// the implied forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Alu {
    Adc,
    And,
    Asl,
    Bit,
    Cmp,
    Cpx,
    Cpy,
    Dec,
    Eor,
    Inc,
    Lda,
    Ldx,
    Ldy,
    Lsr,
    Ora,
    Rol,
    Ror,
    Sbc,
    Trb,
    Tsb,
}

impl Wdc65c816 {
    fn nz8(&mut self, v: u8) {
        self.regs.set_flag(FLAG_Z, v == 0);
        self.regs.set_flag(FLAG_N, v & 0x80 != 0);
    }

    fn nz16(&mut self, v: u16) {
        self.regs.set_flag(FLAG_Z, v == 0);
        self.regs.set_flag(FLAG_N, v & 0x8000 != 0);
    }

    /// Apply `op` to an 8-bit value, returning what a read-modify-write
    /// stores back.
    pub(crate) fn alu8(&mut self, op: Alu, data: u8) -> u8 {
        let a = self.regs.a as u8;
        match op {
            Alu::Adc => {
                let r = self.add8(data);
                self.set_a8(r);
                r
            }
            Alu::Sbc => {
                let r = self.sub8(data);
                self.set_a8(r);
                r
            }
            Alu::And => {
                let r = a & data;
                self.set_a8(r);
                self.nz8(r);
                r
            }
            Alu::Ora => {
                let r = a | data;
                self.set_a8(r);
                self.nz8(r);
                r
            }
            Alu::Eor => {
                let r = a ^ data;
                self.set_a8(r);
                self.nz8(r);
                r
            }
            Alu::Lda => {
                self.set_a8(data);
                self.nz8(data);
                data
            }
            Alu::Ldx => {
                self.regs.x = u16::from(data);
                self.nz8(data);
                data
            }
            Alu::Ldy => {
                self.regs.y = u16::from(data);
                self.nz8(data);
                data
            }
            Alu::Cmp => self.compare8(a, data),
            Alu::Cpx => self.compare8(self.regs.x as u8, data),
            Alu::Cpy => self.compare8(self.regs.y as u8, data),
            Alu::Bit => {
                self.regs.set_flag(FLAG_Z, data & a == 0);
                self.regs.set_flag(FLAG_V, data & 0x40 != 0);
                self.regs.set_flag(FLAG_N, data & 0x80 != 0);
                data
            }
            Alu::Asl => {
                self.regs.set_flag(FLAG_C, data & 0x80 != 0);
                let r = data << 1;
                self.nz8(r);
                r
            }
            Alu::Lsr => {
                self.regs.set_flag(FLAG_C, data & 1 != 0);
                let r = data >> 1;
                self.nz8(r);
                r
            }
            Alu::Rol => {
                let carry = u8::from(self.regs.flag(FLAG_C));
                self.regs.set_flag(FLAG_C, data & 0x80 != 0);
                let r = data << 1 | carry;
                self.nz8(r);
                r
            }
            Alu::Ror => {
                let carry = u8::from(self.regs.flag(FLAG_C));
                self.regs.set_flag(FLAG_C, data & 1 != 0);
                let r = carry << 7 | data >> 1;
                self.nz8(r);
                r
            }
            Alu::Inc => {
                let r = data.wrapping_add(1);
                self.nz8(r);
                r
            }
            Alu::Dec => {
                let r = data.wrapping_sub(1);
                self.nz8(r);
                r
            }
            Alu::Trb => {
                self.regs.set_flag(FLAG_Z, data & a == 0);
                data & !a
            }
            Alu::Tsb => {
                self.regs.set_flag(FLAG_Z, data & a == 0);
                data | a
            }
        }
    }

    /// Apply `op` to a 16-bit value.
    pub(crate) fn alu16(&mut self, op: Alu, data: u16) -> u16 {
        let a = self.regs.a;
        match op {
            Alu::Adc => {
                let r = self.add16(data);
                self.regs.a = r;
                r
            }
            Alu::Sbc => {
                let r = self.sub16(data);
                self.regs.a = r;
                r
            }
            Alu::And => {
                self.regs.a = a & data;
                self.nz16(self.regs.a);
                self.regs.a
            }
            Alu::Ora => {
                self.regs.a = a | data;
                self.nz16(self.regs.a);
                self.regs.a
            }
            Alu::Eor => {
                self.regs.a = a ^ data;
                self.nz16(self.regs.a);
                self.regs.a
            }
            Alu::Lda => {
                self.regs.a = data;
                self.nz16(data);
                data
            }
            Alu::Ldx => {
                self.regs.x = data;
                self.nz16(data);
                data
            }
            Alu::Ldy => {
                self.regs.y = data;
                self.nz16(data);
                data
            }
            Alu::Cmp => self.compare16(a, data),
            Alu::Cpx => self.compare16(self.regs.x, data),
            Alu::Cpy => self.compare16(self.regs.y, data),
            Alu::Bit => {
                self.regs.set_flag(FLAG_Z, data & a == 0);
                self.regs.set_flag(FLAG_V, data & 0x4000 != 0);
                self.regs.set_flag(FLAG_N, data & 0x8000 != 0);
                data
            }
            Alu::Asl => {
                self.regs.set_flag(FLAG_C, data & 0x8000 != 0);
                let r = data << 1;
                self.nz16(r);
                r
            }
            Alu::Lsr => {
                self.regs.set_flag(FLAG_C, data & 1 != 0);
                let r = data >> 1;
                self.nz16(r);
                r
            }
            Alu::Rol => {
                let carry = u16::from(self.regs.flag(FLAG_C));
                self.regs.set_flag(FLAG_C, data & 0x8000 != 0);
                let r = data << 1 | carry;
                self.nz16(r);
                r
            }
            Alu::Ror => {
                let carry = u16::from(self.regs.flag(FLAG_C));
                self.regs.set_flag(FLAG_C, data & 1 != 0);
                let r = carry << 15 | data >> 1;
                self.nz16(r);
                r
            }
            Alu::Inc => {
                let r = data.wrapping_add(1);
                self.nz16(r);
                r
            }
            Alu::Dec => {
                let r = data.wrapping_sub(1);
                self.nz16(r);
                r
            }
            Alu::Trb => {
                self.regs.set_flag(FLAG_Z, data & a == 0);
                data & !a
            }
            Alu::Tsb => {
                self.regs.set_flag(FLAG_Z, data & a == 0);
                data | a
            }
        }
    }

    fn set_a8(&mut self, v: u8) {
        self.regs.a = (self.regs.a & 0xFF00) | u16::from(v);
    }

    fn compare8(&mut self, reg: u8, data: u8) -> u8 {
        let r = reg.wrapping_sub(data);
        self.regs.set_flag(FLAG_C, reg >= data);
        self.nz8(r);
        r
    }

    fn compare16(&mut self, reg: u16, data: u16) -> u16 {
        let r = reg.wrapping_sub(data);
        self.regs.set_flag(FLAG_C, reg >= data);
        self.nz16(r);
        r
    }

    fn add8(&mut self, data: u8) -> u8 {
        let a = i32::from(self.regs.a as u8);
        let data = i32::from(data);
        let c = i32::from(self.regs.flag(FLAG_C));
        let decimal = self.regs.flag(FLAG_D);
        let mut result;
        if decimal {
            result = (a & 0x0F) + (data & 0x0F) + c;
            if result > 0x09 {
                result += 0x06;
            }
            let carry = i32::from(result > 0x0F);
            result = (a & 0xF0) + (data & 0xF0) + (carry << 4) + (result & 0x0F);
        } else {
            result = a + data + c;
        }
        self.finish_add8(a, data, result, decimal, |r| r > 0x9F, 0x60)
    }

    fn sub8(&mut self, data: u8) -> u8 {
        let a = i32::from(self.regs.a as u8);
        let data = i32::from(!data);
        let c = i32::from(self.regs.flag(FLAG_C));
        let decimal = self.regs.flag(FLAG_D);
        let mut result;
        if decimal {
            result = (a & 0x0F) + (data & 0x0F) + c;
            if result <= 0x0F {
                result -= 0x06;
            }
            let carry = i32::from(result > 0x0F);
            result = (a & 0xF0) + (data & 0xF0) + (carry << 4) + (result & 0x0F);
        } else {
            result = a + data + c;
        }
        self.finish_add8(a, data, result, decimal, |r| r <= 0xFF, -0x60)
    }

    /// Flags and final decimal adjustment shared by ADC and SBC.
    fn finish_add8(
        &mut self,
        a: i32,
        data: i32,
        mut result: i32,
        decimal: bool,
        adjust_when: impl Fn(i32) -> bool,
        adjust: i32,
    ) -> u8 {
        self.regs
            .set_flag(FLAG_V, !(a ^ data) & (a ^ result) & 0x80 != 0);
        if decimal && adjust_when(result) {
            result += adjust;
        }
        self.regs.set_flag(FLAG_C, result > 0xFF);
        let r = result as u8;
        self.nz8(r);
        r
    }

    /// 16-bit add, nibble by nibble when decimal, as the chip carries.
    fn add16(&mut self, data: u16) -> u16 {
        let a = i32::from(self.regs.a);
        let data = i32::from(data);
        let c = i32::from(self.regs.flag(FLAG_C));
        let decimal = self.regs.flag(FLAG_D);
        let mut result;
        if decimal {
            result = (a & 0x000F) + (data & 0x000F) + c;
            if result > 0x0009 {
                result += 0x0006;
            }
            let mut carry = i32::from(result > 0x000F);
            result = (a & 0x00F0) + (data & 0x00F0) + (carry << 4) + (result & 0x000F);
            if result > 0x009F {
                result += 0x0060;
            }
            carry = i32::from(result > 0x00FF);
            result = (a & 0x0F00) + (data & 0x0F00) + (carry << 8) + (result & 0x00FF);
            if result > 0x09FF {
                result += 0x0600;
            }
            carry = i32::from(result > 0x0FFF);
            result = (a & 0xF000) + (data & 0xF000) + (carry << 12) + (result & 0x0FFF);
        } else {
            result = a + data + c;
        }
        self.finish_add16(a, data, result, decimal, |r| r > 0x9FFF, 0x6000)
    }

    fn sub16(&mut self, data: u16) -> u16 {
        let a = i32::from(self.regs.a);
        let data = i32::from(!data);
        let c = i32::from(self.regs.flag(FLAG_C));
        let decimal = self.regs.flag(FLAG_D);
        let mut result;
        if decimal {
            result = (a & 0x000F) + (data & 0x000F) + c;
            if result <= 0x000F {
                result -= 0x0006;
            }
            let mut carry = i32::from(result > 0x000F);
            result = (a & 0x00F0) + (data & 0x00F0) + (carry << 4) + (result & 0x000F);
            if result <= 0x00FF {
                result -= 0x0060;
            }
            carry = i32::from(result > 0x00FF);
            result = (a & 0x0F00) + (data & 0x0F00) + (carry << 8) + (result & 0x00FF);
            if result <= 0x0FFF {
                result -= 0x0600;
            }
            carry = i32::from(result > 0x0FFF);
            result = (a & 0xF000) + (data & 0xF000) + (carry << 12) + (result & 0x0FFF);
        } else {
            result = a + data + c;
        }
        self.finish_add16(a, data, result, decimal, |r| r <= 0xFFFF, -0x6000)
    }

    fn finish_add16(
        &mut self,
        a: i32,
        data: i32,
        mut result: i32,
        decimal: bool,
        adjust_when: impl Fn(i32) -> bool,
        adjust: i32,
    ) -> u16 {
        self.regs
            .set_flag(FLAG_V, !(a ^ data) & (a ^ result) & 0x8000 != 0);
        if decimal && adjust_when(result) {
            result += adjust;
        }
        self.regs.set_flag(FLAG_C, result > 0xFFFF);
        let r = result as u16;
        self.nz16(r);
        r
    }
}
