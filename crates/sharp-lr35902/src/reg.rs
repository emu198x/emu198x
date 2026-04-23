//! Register-pair accessors and operand decoders.
//!
//! The SM83 instruction encoding packs register selection into 3-bit
//! and 2-bit fields (`r`, `rr`). The same `r` index is used by `LD r,r`,
//! the ALU group (`80..BF`), and the CB-prefix sub-table; the `(HL)`
//! slot lives at `r == 6` and is handled inline by each instruction
//! since it requires a memory access.
//!
//! Several of these helpers are only consumed by opcode arms that land
//! in step 3 of the port; suppress the dead-code warning until then.

#![allow(dead_code)]

use crate::Sm83;

impl Sm83 {
    /// 16-bit `BC` register pair.
    #[inline]
    #[must_use]
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }

    /// 16-bit `DE` register pair.
    #[inline]
    #[must_use]
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }

    /// 16-bit `HL` register pair.
    #[inline]
    #[must_use]
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    /// 16-bit internal scratch `WZ` (analogous to the Z80 `MEMPTR`
    /// hidden register). Composed from `w` (high) and `z` (low).
    #[inline]
    #[must_use]
    pub fn wz(&self) -> u16 {
        u16::from_be_bytes([self.w, self.z])
    }

    #[inline]
    pub(crate) fn set_bc(&mut self, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.b = hi;
        self.c = lo;
    }

    #[inline]
    pub(crate) fn set_de(&mut self, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.d = hi;
        self.e = lo;
    }

    #[inline]
    pub(crate) fn set_hl(&mut self, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.h = hi;
        self.l = lo;
    }

    /// Read the register selected by a 3-bit operand index.
    ///
    /// `reg == 6` (`(HL)`) is the responsibility of the calling
    /// instruction — it requires a memory access and so cannot be
    /// served from CPU state alone.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `reg == 6`.
    #[inline]
    pub(crate) fn read_reg8(&self, reg: u8) -> u8 {
        match reg & 0b111 {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => {
                debug_assert!(false, "read_reg8 called with (HL) — handle inline");
                0
            }
            _ => self.a,
        }
    }

    /// Write the register selected by a 3-bit operand index.
    ///
    /// `reg == 6` (`(HL)`) is the responsibility of the calling
    /// instruction — see [`read_reg8`](Self::read_reg8).
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `reg == 6`.
    #[inline]
    pub(crate) fn write_reg8(&mut self, reg: u8, value: u8) {
        match reg & 0b111 {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => self.h = value,
            5 => self.l = value,
            6 => debug_assert!(false, "write_reg8 called with (HL) — handle inline"),
            _ => self.a = value,
        }
    }

    /// Read the 16-bit register pair selected by a 2-bit operand
    /// index. The `LD rr,d16` family treats slot 3 as `SP`; PUSH/POP
    /// treats it as `AF` (handled separately).
    #[inline]
    pub(crate) fn read_reg16_sp(&self, pair: u8) -> u16 {
        match pair & 0b11 {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.sp,
        }
    }

    /// Write the 16-bit register pair selected by a 2-bit operand
    /// index. Slot 3 is `SP`.
    #[inline]
    pub(crate) fn write_reg16_sp(&mut self, pair: u8, value: u16) {
        match pair & 0b11 {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.set_hl(value),
            _ => self.sp = value,
        }
    }

    /// Read the 16-bit register pair selected by a 2-bit operand
    /// index, using the PUSH/POP convention where slot 3 is `AF`
    /// rather than `SP`.
    #[inline]
    pub(crate) fn read_reg16_af(&self, pair: u8) -> u16 {
        match pair & 0b11 {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => u16::from_be_bytes([self.a, self.f]),
        }
    }

    /// Write the 16-bit register pair selected by a 2-bit operand
    /// index, using the PUSH/POP convention (slot 3 = `AF`). The low
    /// nibble of `F` is hardwired to zero on real hardware; any write
    /// via `POP AF` masks those bits off.
    #[inline]
    pub(crate) fn write_reg16_af(&mut self, pair: u8, value: u16) {
        match pair & 0b11 {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.set_hl(value),
            _ => {
                let [a, f] = value.to_be_bytes();
                self.a = a;
                self.f = f & 0xF0;
            }
        }
    }
}
