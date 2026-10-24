//! The instructions, one bus cycle per step.
//!
//! Each instruction is a function of `self.step`. Step 0 is the opcode
//! fetch; on each later step the function first takes in what the previous
//! cycle read (`self.last`), then puts the next cycle on the pins. The step
//! that finishes the instruction calls [`Wdc65c816::finish`], which puts the
//! next opcode fetch (or an interrupt) on the pins in the same tick. A cycle
//! the instruction does not always need, such as the direct-page penalty, is
//! skipped by moving to the next step at once.
//!
//! The sequences are ares's (ISC licence), re-expressed cycle by cycle.

use crate::alu::Alu;
use crate::registers::{FLAG_C, FLAG_I, FLAG_M, FLAG_N, FLAG_V, FLAG_X, FLAG_Z};
use crate::{Interrupt, OP_FETCH, OP_INTERRUPT, OP_RESET, Wdc65c816};

/// How an instruction finds its operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// `addr`, in the data bank.
    Abs,
    AbsX,
    AbsY,
    /// `long`: a 24-bit address.
    Long,
    LongX,
    /// `dp`.
    Dp,
    DpX,
    DpY,
    /// `(dp)`.
    Ind,
    /// `(dp,X)`.
    IndX,
    /// `(dp),Y`.
    IndY,
    /// `[dp]`.
    IndLong,
    /// `[dp],Y`.
    IndLongY,
    /// `sr,S`.
    Sr,
    /// `(sr,S),Y`.
    SrIndY,
}

/// What an instruction does with its operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Read(Alu),
    Write(Src),
    Modify(Alu),
}

/// The register a store writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Src {
    A,
    X,
    Y,
    /// `STZ`.
    Zero,
}

/// A register an implied or transfer instruction names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reg {
    A,
    X,
    Y,
    S,
    D,
}

impl Wdc65c816 {
    /// Move straight on to the next step.
    fn skip(&mut self) {
        self.step += 1;
        self.exec();
    }

    fn last16(&self) -> u32 {
        u32::from(self.last)
    }

    // ── Address formation ──

    /// An address in the data bank. Indexing may carry into the next bank.
    fn bank(&self, addr: u32) -> u32 {
        ((u32::from(self.regs.dbr) << 16) + addr) & 0xFF_FFFF
    }

    /// A direct-page address. In emulation mode with the direct page
    /// register page-aligned, the page wraps as on the 6502.
    fn dp(&self, offset: u32) -> u32 {
        let d = u32::from(self.regs.d);
        if self.regs.e && d & 0xFF == 0 {
            d | (offset & 0xFF)
        } else {
            (d + offset) & 0xFFFF
        }
    }

    /// A direct-page address for the 65C816's own modes, which never wrap
    /// within the page.
    fn dpn(&self, offset: u32) -> u32 {
        (u32::from(self.regs.d) + offset) & 0xFFFF
    }

    /// `(dp,X)`'s pointer address: the ordinary direct-page rule. ares wraps
    /// the pointer's high byte within the page in emulation mode with an
    /// unaligned direct page; the SingleStepTests corpus carries it into the
    /// next page instead (cases `01 e 26`, `01 e 328`, `01 e 366`), and this
    /// core follows the corpus.
    fn dp_x(&self, address: u32, offset: u32) -> u32 {
        self.dp(address + offset)
    }

    fn stack(&self, offset: u32) -> u32 {
        (u32::from(self.regs.s) + offset) & 0xFFFF
    }

    fn dl_nonzero(&self) -> bool {
        self.regs.d & 0xFF != 0
    }

    fn reg(&self, r: Reg) -> u16 {
        match r {
            Reg::A => self.regs.a,
            Reg::X => self.regs.x,
            Reg::Y => self.regs.y,
            Reg::S => self.regs.s,
            Reg::D => self.regs.d,
        }
    }

    fn set_reg(&mut self, r: Reg, value: u16) {
        match r {
            Reg::A => self.regs.a = value,
            Reg::X => self.regs.x = value,
            Reg::Y => self.regs.y = value,
            Reg::S => self.regs.s = value,
            Reg::D => self.regs.d = value,
        }
    }

    fn src(&self, s: Src) -> u16 {
        match s {
            Src::A => self.regs.a,
            Src::X => self.regs.x,
            Src::Y => self.regs.y,
            Src::Zero => 0,
        }
    }

    fn set_nz8(&mut self, v: u8) {
        self.regs.set_flag(FLAG_Z, v == 0);
        self.regs.set_flag(FLAG_N, v & 0x80 != 0);
    }

    fn set_nz16(&mut self, v: u16) {
        self.regs.set_flag(FLAG_Z, v == 0);
        self.regs.set_flag(FLAG_N, v & 0x8000 != 0);
    }

    /// Emulation mode holds the stack in page 1.
    fn fix_stack(&mut self) {
        if self.regs.e {
            self.regs.s = 0x0100 | (self.regs.s & 0xFF);
        }
    }

    /// After P changes: emulation mode forces M and X, and an 8-bit index
    /// width clears the index registers' high bytes.
    fn fix_p(&mut self) {
        if self.regs.e {
            self.regs.p |= FLAG_M | FLAG_X;
        }
        if self.regs.x8() {
            self.regs.x &= 0xFF;
            self.regs.y &= 0xFF;
        }
    }

    // ── Dispatch ──

    pub(crate) fn exec(&mut self) {
        let m = !self.m8;
        let x = !self.x8;
        use Alu::*;
        use Kind::{Modify as Md, Read as Rd, Write as Wr};
        use Mode::*;
        match self.op {
            OP_FETCH => {
                self.op = u16::from(self.last);
                self.m8 = self.regs.m8();
                self.x8 = self.regs.x8();
                self.saved = Some(self.regs);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.step = 1;
                self.exec();
            }
            OP_INTERRUPT => self.op_interrupt(),
            OP_RESET => self.op_reset(),

            0x00 => self.op_brk(if self.regs.e { 0xFFFE } else { 0xFFE6 }),
            0x01 => self.mem(IndX, Rd(Ora), m),
            0x02 => self.op_brk(if self.regs.e { 0xFFF4 } else { 0xFFE4 }),
            0x03 => self.mem(Sr, Rd(Ora), m),
            0x04 => self.mem(Dp, Md(Tsb), m),
            0x05 => self.mem(Dp, Rd(Ora), m),
            0x06 => self.mem(Dp, Md(Asl), m),
            0x07 => self.mem(IndLong, Rd(Ora), m),
            0x08 => self.op_push8(self.regs.p),
            0x09 => self.op_immediate(Ora, m),
            0x0A => self.op_implied_modify(Asl, Reg::A, m),
            0x0B => self.op_push_n(self.regs.d),
            0x0C => self.mem(Abs, Md(Tsb), m),
            0x0D => self.mem(Abs, Rd(Ora), m),
            0x0E => self.mem(Abs, Md(Asl), m),
            0x0F => self.mem(Long, Rd(Ora), m),

            0x10 => self.op_branch(!self.regs.flag(FLAG_N)),
            0x11 => self.mem(IndY, Rd(Ora), m),
            0x12 => self.mem(Ind, Rd(Ora), m),
            0x13 => self.mem(SrIndY, Rd(Ora), m),
            0x14 => self.mem(Dp, Md(Trb), m),
            0x15 => self.mem(DpX, Rd(Ora), m),
            0x16 => self.mem(DpX, Md(Asl), m),
            0x17 => self.mem(IndLongY, Rd(Ora), m),
            0x18 => self.op_flag(FLAG_C, false),
            0x19 => self.mem(AbsY, Rd(Ora), m),
            0x1A => self.op_implied_modify(Inc, Reg::A, m),
            0x1B => self.op_tcs(),
            0x1C => self.mem(Abs, Md(Trb), m),
            0x1D => self.mem(AbsX, Rd(Ora), m),
            0x1E => self.mem(AbsX, Md(Asl), m),
            0x1F => self.mem(LongX, Rd(Ora), m),

            0x20 => self.op_jsr(),
            0x21 => self.mem(IndX, Rd(And), m),
            0x22 => self.op_jsl(),
            0x23 => self.mem(Sr, Rd(And), m),
            0x24 => self.mem(Dp, Rd(Bit), m),
            0x25 => self.mem(Dp, Rd(And), m),
            0x26 => self.mem(Dp, Md(Rol), m),
            0x27 => self.mem(IndLong, Rd(And), m),
            0x28 => self.op_plp(),
            0x29 => self.op_immediate(And, m),
            0x2A => self.op_implied_modify(Rol, Reg::A, m),
            0x2B => self.op_pld(),
            0x2C => self.mem(Abs, Rd(Bit), m),
            0x2D => self.mem(Abs, Rd(And), m),
            0x2E => self.mem(Abs, Md(Rol), m),
            0x2F => self.mem(Long, Rd(And), m),

            0x30 => self.op_branch(self.regs.flag(FLAG_N)),
            0x31 => self.mem(IndY, Rd(And), m),
            0x32 => self.mem(Ind, Rd(And), m),
            0x33 => self.mem(SrIndY, Rd(And), m),
            0x34 => self.mem(DpX, Rd(Bit), m),
            0x35 => self.mem(DpX, Rd(And), m),
            0x36 => self.mem(DpX, Md(Rol), m),
            0x37 => self.mem(IndLongY, Rd(And), m),
            0x38 => self.op_flag(FLAG_C, true),
            0x39 => self.mem(AbsY, Rd(And), m),
            0x3A => self.op_implied_modify(Dec, Reg::A, m),
            0x3B => self.op_transfer(Reg::S, Reg::A, true),
            0x3C => self.mem(AbsX, Rd(Bit), m),
            0x3D => self.mem(AbsX, Rd(And), m),
            0x3E => self.mem(AbsX, Md(Rol), m),
            0x3F => self.mem(LongX, Rd(And), m),

            0x40 => self.op_rti(),
            0x41 => self.mem(IndX, Rd(Eor), m),
            0x42 => self.op_wdm(),
            0x43 => self.mem(Sr, Rd(Eor), m),
            0x44 => self.op_block_move(-1),
            0x45 => self.mem(Dp, Rd(Eor), m),
            0x46 => self.mem(Dp, Md(Lsr), m),
            0x47 => self.mem(IndLong, Rd(Eor), m),
            0x48 => self.op_push(self.regs.a, m),
            0x49 => self.op_immediate(Eor, m),
            0x4A => self.op_implied_modify(Lsr, Reg::A, m),
            0x4B => self.op_push8(self.regs.pbr),
            0x4C => self.op_jmp(),
            0x4D => self.mem(Abs, Rd(Eor), m),
            0x4E => self.mem(Abs, Md(Lsr), m),
            0x4F => self.mem(Long, Rd(Eor), m),

            0x50 => self.op_branch(!self.regs.flag(FLAG_V)),
            0x51 => self.mem(IndY, Rd(Eor), m),
            0x52 => self.mem(Ind, Rd(Eor), m),
            0x53 => self.mem(SrIndY, Rd(Eor), m),
            0x54 => self.op_block_move(1),
            0x55 => self.mem(DpX, Rd(Eor), m),
            0x56 => self.mem(DpX, Md(Lsr), m),
            0x57 => self.mem(IndLongY, Rd(Eor), m),
            0x58 => self.op_flag(FLAG_I, false),
            0x59 => self.mem(AbsY, Rd(Eor), m),
            0x5A => self.op_push(self.regs.y, x),
            0x5B => self.op_transfer(Reg::A, Reg::D, true),
            0x5C => self.op_jml(),
            0x5D => self.mem(AbsX, Rd(Eor), m),
            0x5E => self.mem(AbsX, Md(Lsr), m),
            0x5F => self.mem(LongX, Rd(Eor), m),

            0x60 => self.op_rts(),
            0x61 => self.mem(IndX, Rd(Adc), m),
            0x62 => self.op_per(),
            0x63 => self.mem(Sr, Rd(Adc), m),
            0x64 => self.mem(Dp, Wr(Src::Zero), m),
            0x65 => self.mem(Dp, Rd(Adc), m),
            0x66 => self.mem(Dp, Md(Ror), m),
            0x67 => self.mem(IndLong, Rd(Adc), m),
            0x68 => self.op_pull(Reg::A, m),
            0x69 => self.op_immediate(Adc, m),
            0x6A => self.op_implied_modify(Ror, Reg::A, m),
            0x6B => self.op_rtl(),
            0x6C => self.op_jmp_indirect(),
            0x6D => self.mem(Abs, Rd(Adc), m),
            0x6E => self.mem(Abs, Md(Ror), m),
            0x6F => self.mem(Long, Rd(Adc), m),

            0x70 => self.op_branch(self.regs.flag(FLAG_V)),
            0x71 => self.mem(IndY, Rd(Adc), m),
            0x72 => self.mem(Ind, Rd(Adc), m),
            0x73 => self.mem(SrIndY, Rd(Adc), m),
            0x74 => self.mem(DpX, Wr(Src::Zero), m),
            0x75 => self.mem(DpX, Rd(Adc), m),
            0x76 => self.mem(DpX, Md(Ror), m),
            0x77 => self.mem(IndLongY, Rd(Adc), m),
            0x78 => self.op_flag(FLAG_I, true),
            0x79 => self.mem(AbsY, Rd(Adc), m),
            0x7A => self.op_pull(Reg::Y, x),
            0x7B => self.op_transfer(Reg::D, Reg::A, true),
            0x7C => self.op_jmp_indexed_indirect(),
            0x7D => self.mem(AbsX, Rd(Adc), m),
            0x7E => self.mem(AbsX, Md(Ror), m),
            0x7F => self.mem(LongX, Rd(Adc), m),

            0x80 => self.op_branch(true),
            0x81 => self.mem(IndX, Wr(Src::A), m),
            0x82 => self.op_brl(),
            0x83 => self.mem(Sr, Wr(Src::A), m),
            0x84 => self.mem(Dp, Wr(Src::Y), x),
            0x85 => self.mem(Dp, Wr(Src::A), m),
            0x86 => self.mem(Dp, Wr(Src::X), x),
            0x87 => self.mem(IndLong, Wr(Src::A), m),
            0x88 => self.op_implied_modify(Dec, Reg::Y, x),
            0x89 => self.op_bit_immediate(m),
            0x8A => self.op_transfer(Reg::X, Reg::A, m),
            0x8B => self.op_push8(self.regs.dbr),
            0x8C => self.mem(Abs, Wr(Src::Y), x),
            0x8D => self.mem(Abs, Wr(Src::A), m),
            0x8E => self.mem(Abs, Wr(Src::X), x),
            0x8F => self.mem(Long, Wr(Src::A), m),

            0x90 => self.op_branch(!self.regs.flag(FLAG_C)),
            0x91 => self.mem(IndY, Wr(Src::A), m),
            0x92 => self.mem(Ind, Wr(Src::A), m),
            0x93 => self.mem(SrIndY, Wr(Src::A), m),
            0x94 => self.mem(DpX, Wr(Src::Y), x),
            0x95 => self.mem(DpX, Wr(Src::A), m),
            0x96 => self.mem(DpY, Wr(Src::X), x),
            0x97 => self.mem(IndLongY, Wr(Src::A), m),
            0x98 => self.op_transfer(Reg::Y, Reg::A, m),
            0x99 => self.mem(AbsY, Wr(Src::A), m),
            0x9A => self.op_txs(),
            0x9B => self.op_transfer(Reg::X, Reg::Y, x),
            0x9C => self.mem(Abs, Wr(Src::Zero), m),
            0x9D => self.mem(AbsX, Wr(Src::A), m),
            0x9E => self.mem(AbsX, Wr(Src::Zero), m),
            0x9F => self.mem(LongX, Wr(Src::A), m),

            0xA0 => self.op_immediate(Ldy, x),
            0xA1 => self.mem(IndX, Rd(Lda), m),
            0xA2 => self.op_immediate(Ldx, x),
            0xA3 => self.mem(Sr, Rd(Lda), m),
            0xA4 => self.mem(Dp, Rd(Ldy), x),
            0xA5 => self.mem(Dp, Rd(Lda), m),
            0xA6 => self.mem(Dp, Rd(Ldx), x),
            0xA7 => self.mem(IndLong, Rd(Lda), m),
            0xA8 => self.op_transfer(Reg::A, Reg::Y, x),
            0xA9 => self.op_immediate(Lda, m),
            0xAA => self.op_transfer(Reg::A, Reg::X, x),
            0xAB => self.op_plb(),
            0xAC => self.mem(Abs, Rd(Ldy), x),
            0xAD => self.mem(Abs, Rd(Lda), m),
            0xAE => self.mem(Abs, Rd(Ldx), x),
            0xAF => self.mem(Long, Rd(Lda), m),

            0xB0 => self.op_branch(self.regs.flag(FLAG_C)),
            0xB1 => self.mem(IndY, Rd(Lda), m),
            0xB2 => self.mem(Ind, Rd(Lda), m),
            0xB3 => self.mem(SrIndY, Rd(Lda), m),
            0xB4 => self.mem(DpX, Rd(Ldy), x),
            0xB5 => self.mem(DpX, Rd(Lda), m),
            0xB6 => self.mem(DpY, Rd(Ldx), x),
            0xB7 => self.mem(IndLongY, Rd(Lda), m),
            0xB8 => self.op_flag(FLAG_V, false),
            0xB9 => self.mem(AbsY, Rd(Lda), m),
            0xBA => self.op_transfer(Reg::S, Reg::X, x),
            0xBB => self.op_transfer(Reg::Y, Reg::X, x),
            0xBC => self.mem(AbsX, Rd(Ldy), x),
            0xBD => self.mem(AbsX, Rd(Lda), m),
            0xBE => self.mem(AbsY, Rd(Ldx), x),
            0xBF => self.mem(LongX, Rd(Lda), m),

            0xC0 => self.op_immediate(Cpy, x),
            0xC1 => self.mem(IndX, Rd(Cmp), m),
            0xC2 => self.op_rep_sep(false),
            0xC3 => self.mem(Sr, Rd(Cmp), m),
            0xC4 => self.mem(Dp, Rd(Cpy), x),
            0xC5 => self.mem(Dp, Rd(Cmp), m),
            0xC6 => self.mem(Dp, Md(Dec), m),
            0xC7 => self.mem(IndLong, Rd(Cmp), m),
            0xC8 => self.op_implied_modify(Inc, Reg::Y, x),
            0xC9 => self.op_immediate(Cmp, m),
            0xCA => self.op_implied_modify(Dec, Reg::X, x),
            0xCB => self.op_wai(),
            0xCC => self.mem(Abs, Rd(Cpy), x),
            0xCD => self.mem(Abs, Rd(Cmp), m),
            0xCE => self.mem(Abs, Md(Dec), m),
            0xCF => self.mem(Long, Rd(Cmp), m),

            0xD0 => self.op_branch(!self.regs.flag(FLAG_Z)),
            0xD1 => self.mem(IndY, Rd(Cmp), m),
            0xD2 => self.mem(Ind, Rd(Cmp), m),
            0xD3 => self.mem(SrIndY, Rd(Cmp), m),
            0xD4 => self.op_pei(),
            0xD5 => self.mem(DpX, Rd(Cmp), m),
            0xD6 => self.mem(DpX, Md(Dec), m),
            0xD7 => self.mem(IndLongY, Rd(Cmp), m),
            0xD8 => self.op_flag(crate::registers::FLAG_D, false),
            0xD9 => self.mem(AbsY, Rd(Cmp), m),
            0xDA => self.op_push(self.regs.x, x),
            0xDB => self.op_stp(),
            0xDC => self.op_jml_indirect(),
            0xDD => self.mem(AbsX, Rd(Cmp), m),
            0xDE => self.mem(AbsX, Md(Dec), m),
            0xDF => self.mem(LongX, Rd(Cmp), m),

            0xE0 => self.op_immediate(Cpx, x),
            0xE1 => self.mem(IndX, Rd(Sbc), m),
            0xE2 => self.op_rep_sep(true),
            0xE3 => self.mem(Sr, Rd(Sbc), m),
            0xE4 => self.mem(Dp, Rd(Cpx), x),
            0xE5 => self.mem(Dp, Rd(Sbc), m),
            0xE6 => self.mem(Dp, Md(Inc), m),
            0xE7 => self.mem(IndLong, Rd(Sbc), m),
            0xE8 => self.op_implied_modify(Inc, Reg::X, x),
            0xE9 => self.op_immediate(Sbc, m),
            0xEA => self.op_nop(),
            0xEB => self.op_xba(),
            0xEC => self.mem(Abs, Rd(Cpx), x),
            0xED => self.mem(Abs, Rd(Sbc), m),
            0xEE => self.mem(Abs, Md(Inc), m),
            0xEF => self.mem(Long, Rd(Sbc), m),

            0xF0 => self.op_branch(self.regs.flag(FLAG_Z)),
            0xF1 => self.mem(IndY, Rd(Sbc), m),
            0xF2 => self.mem(Ind, Rd(Sbc), m),
            0xF3 => self.mem(SrIndY, Rd(Sbc), m),
            0xF4 => self.op_pea(),
            0xF5 => self.mem(DpX, Rd(Sbc), m),
            0xF6 => self.mem(DpX, Md(Inc), m),
            0xF7 => self.mem(IndLongY, Rd(Sbc), m),
            0xF8 => self.op_flag(crate::registers::FLAG_D, true),
            0xF9 => self.mem(AbsY, Rd(Sbc), m),
            0xFA => self.op_pull(Reg::X, x),
            0xFB => self.op_xce(),
            0xFC => self.op_jsr_indexed_indirect(),
            0xFD => self.mem(AbsX, Rd(Sbc), m),
            0xFE => self.mem(AbsX, Md(Inc), m),
            0xFF => self.mem(LongX, Rd(Sbc), m),
            _ => unreachable!("opcode {:#X} out of range", self.op),
        }
    }

    // ── Memory instructions ──

    /// Every instruction that reads, writes or modifies memory: the
    /// addressing mode's cycles, then [`Self::data`].
    fn mem(&mut self, mode: Mode, kind: Kind, wide: bool) {
        let read = matches!(kind, Kind::Read(_));
        let s = self.step;
        match mode {
            Mode::Abs | Mode::AbsX | Mode::AbsY => {
                let i = match mode {
                    Mode::AbsX => u32::from(self.regs.x),
                    Mode::AbsY => u32::from(self.regs.y),
                    _ => 0,
                };
                match s {
                    1 => self.fetch(),
                    2 => {
                        self.v = self.last16();
                        self.fetch();
                    }
                    3 => {
                        self.v |= self.last16() << 8;
                        let crosses = (self.v >> 8) != ((self.v + i) >> 8);
                        if mode != Mode::Abs && (!read || !self.x8 || crosses) {
                            let uncarried = (self.v & 0xFF00) | ((self.v + i) & 0xFF);
                            self.idle_at(self.bank(uncarried));
                        } else {
                            self.skip();
                        }
                    }
                    _ => {
                        let a = self.v + i;
                        self.data(4, kind, wide, self.bank(a), self.bank(a + 1));
                    }
                }
            }
            Mode::Long | Mode::LongX => {
                let i = if mode == Mode::LongX {
                    u32::from(self.regs.x)
                } else {
                    0
                };
                match s {
                    1 => self.fetch(),
                    2 => {
                        self.v = self.last16();
                        self.fetch();
                    }
                    3 => {
                        self.v |= self.last16() << 8;
                        self.fetch();
                    }
                    _ => {
                        if s == 4 {
                            self.v |= self.last16() << 16;
                        }
                        let a = self.v + i;
                        self.data(4, kind, wide, a & 0xFF_FFFF, (a + 1) & 0xFF_FFFF);
                    }
                }
            }
            Mode::Dp | Mode::DpX | Mode::DpY => {
                let i = match mode {
                    Mode::DpX => u32::from(self.regs.x),
                    Mode::DpY => u32::from(self.regs.y),
                    _ => 0,
                };
                match s {
                    1 => self.fetch(),
                    2 => {
                        self.u = self.last16();
                        if self.dl_nonzero() {
                            self.idle();
                        } else {
                            self.skip();
                        }
                    }
                    3 => {
                        if mode == Mode::Dp {
                            self.skip();
                        } else {
                            self.idle();
                        }
                    }
                    _ => {
                        let a = self.u + i;
                        self.data(4, kind, wide, self.dp(a), self.dp(a + 1));
                    }
                }
            }
            Mode::Ind | Mode::IndX => {
                let indexed = mode == Mode::IndX;
                let x = u32::from(self.regs.x);
                match s {
                    1 => self.fetch(),
                    2 => {
                        self.u = self.last16();
                        if self.dl_nonzero() {
                            self.idle();
                        } else {
                            self.skip();
                        }
                    }
                    3 => {
                        if indexed {
                            self.idle();
                        } else {
                            self.skip();
                        }
                    }
                    4 => {
                        let a = if indexed {
                            self.dp_x(self.u + x, 0)
                        } else {
                            self.dp(self.u)
                        };
                        self.read(a);
                    }
                    5 => {
                        self.v = self.last16();
                        let a = if indexed {
                            self.dp_x(self.u + x, 1)
                        } else {
                            self.dp(self.u + 1)
                        };
                        self.read(a);
                    }
                    _ => {
                        if s == 6 {
                            self.v |= self.last16() << 8;
                        }
                        self.data(6, kind, wide, self.bank(self.v), self.bank(self.v + 1));
                    }
                }
            }
            Mode::IndY => {
                let y = u32::from(self.regs.y);
                match s {
                    1 => self.fetch(),
                    2 => {
                        self.u = self.last16();
                        if self.dl_nonzero() {
                            self.idle();
                        } else {
                            self.skip();
                        }
                    }
                    3 => self.read(self.dp(self.u)),
                    4 => {
                        self.v = self.last16();
                        self.read(self.dp(self.u + 1));
                    }
                    5 => {
                        self.v |= self.last16() << 8;
                        let crosses = (self.v >> 8) != ((self.v + y) >> 8);
                        if !read || !self.x8 || crosses {
                            let uncarried = (self.v & 0xFF00) | ((self.v + y) & 0xFF);
                            self.idle_at(self.bank(uncarried));
                        } else {
                            self.skip();
                        }
                    }
                    _ => {
                        let a = self.v + y;
                        self.data(6, kind, wide, self.bank(a), self.bank(a + 1));
                    }
                }
            }
            Mode::IndLong | Mode::IndLongY => {
                let i = if mode == Mode::IndLongY {
                    u32::from(self.regs.y)
                } else {
                    0
                };
                match s {
                    1 => self.fetch(),
                    2 => {
                        self.u = self.last16();
                        if self.dl_nonzero() {
                            self.idle();
                        } else {
                            self.skip();
                        }
                    }
                    3 => self.read(self.dpn(self.u)),
                    4 => {
                        self.v = self.last16();
                        self.read(self.dpn(self.u + 1));
                    }
                    5 => {
                        self.v |= self.last16() << 8;
                        self.read(self.dpn(self.u + 2));
                    }
                    _ => {
                        if s == 6 {
                            self.v |= self.last16() << 16;
                        }
                        let a = self.v + i;
                        self.data(6, kind, wide, a & 0xFF_FFFF, (a + 1) & 0xFF_FFFF);
                    }
                }
            }
            Mode::Sr => match s {
                1 => self.fetch(),
                2 => {
                    self.u = self.last16();
                    self.idle();
                }
                _ => self.data(3, kind, wide, self.stack(self.u), self.stack(self.u + 1)),
            },
            Mode::SrIndY => {
                let y = u32::from(self.regs.y);
                match s {
                    1 => self.fetch(),
                    2 => {
                        self.u = self.last16();
                        self.idle();
                    }
                    3 => self.read(self.stack(self.u)),
                    4 => {
                        self.v = self.last16();
                        self.read(self.stack(self.u + 1));
                    }
                    5 => {
                        self.v |= self.last16() << 8;
                        self.idle();
                    }
                    _ => {
                        let a = self.v + y;
                        self.data(6, kind, wide, self.bank(a), self.bank(a + 1));
                    }
                }
            }
        }
    }

    /// The operand cycles, starting at step `k`: one byte at `a0`, or two
    /// at `a0` and `a1` when `wide`.
    fn data(&mut self, k: u8, kind: Kind, wide: bool, a0: u32, a1: u32) {
        let n = self.step - k;
        match kind {
            Kind::Read(op) => match n {
                0 => {
                    if !wide {
                        self.last_cycle();
                    }
                    self.read(a0);
                }
                1 if wide => {
                    self.w = self.last16();
                    self.last_cycle();
                    self.read(a1);
                }
                1 => {
                    self.alu8(op, self.last);
                    self.finish();
                }
                _ => {
                    self.w |= self.last16() << 8;
                    self.alu16(op, self.w as u16);
                    self.finish();
                }
            },
            Kind::Write(src) => {
                let value = self.src(src);
                match n {
                    0 => {
                        if !wide {
                            self.last_cycle();
                        }
                        self.write(a0, value as u8);
                    }
                    1 if wide => {
                        self.last_cycle();
                        self.write(a1, (value >> 8) as u8);
                    }
                    _ => self.finish(),
                }
            }
            Kind::Modify(op) => {
                if wide {
                    match n {
                        0 => self.read(a0),
                        1 => {
                            self.w = self.last16();
                            self.read(a1);
                        }
                        2 => {
                            self.w |= self.last16() << 8;
                            self.idle();
                        }
                        3 => {
                            self.w = u32::from(self.alu16(op, self.w as u16));
                            self.write(a1, (self.w >> 8) as u8);
                        }
                        4 => {
                            self.last_cycle();
                            self.write(a0, self.w as u8);
                        }
                        _ => {
                            self.finish();
                            return;
                        }
                    }
                } else {
                    match n {
                        0 => self.read(a0),
                        1 => {
                            self.w = self.last16();
                            if self.regs.e {
                                // Emulation mode keeps the 6502's dummy
                                // write of the unmodified byte, with VDA low.
                                self.write(a0, self.w as u8);
                                self.vda = false;
                            } else {
                                self.idle();
                            }
                        }
                        2 => {
                            self.w = u32::from(self.alu8(op, self.w as u8));
                            self.last_cycle();
                            self.write(a0, self.w as u8);
                        }
                        _ => {
                            self.finish();
                            return;
                        }
                    }
                }
                self.lock();
            }
        }
    }

    // ── Immediate and implied ──

    fn op_immediate(&mut self, op: Alu, wide: bool) {
        match self.step {
            1 => {
                if !wide {
                    self.last_cycle();
                }
                self.fetch();
            }
            2 if wide => {
                self.w = self.last16();
                self.last_cycle();
                self.fetch();
            }
            2 => {
                self.alu8(op, self.last);
                self.finish();
            }
            _ => {
                self.w |= self.last16() << 8;
                self.alu16(op, self.w as u16);
                self.finish();
            }
        }
    }

    fn op_bit_immediate(&mut self, wide: bool) {
        match self.step {
            1 => {
                if !wide {
                    self.last_cycle();
                }
                self.fetch();
            }
            2 if wide => {
                self.w = self.last16();
                self.last_cycle();
                self.fetch();
            }
            2 => {
                let z = self.last & self.regs.a as u8 == 0;
                self.regs.set_flag(FLAG_Z, z);
                self.finish();
            }
            _ => {
                self.w |= self.last16() << 8;
                let z = self.w as u16 & self.regs.a == 0;
                self.regs.set_flag(FLAG_Z, z);
                self.finish();
            }
        }
    }

    /// A two-cycle instruction's internal cycle, which is also its last.
    fn implied_cycle(&mut self) {
        self.last_cycle();
        self.idle();
    }

    fn op_implied_modify(&mut self, op: Alu, r: Reg, wide: bool) {
        if self.step == 1 {
            self.implied_cycle();
            return;
        }
        let value = self.reg(r);
        let result = if wide {
            self.alu16(op, value)
        } else {
            (value & 0xFF00) | u16::from(self.alu8(op, value as u8))
        };
        self.set_reg(r, result);
        self.finish();
    }

    fn op_flag(&mut self, flag: u8, value: bool) {
        if self.step == 1 {
            self.implied_cycle();
            return;
        }
        self.regs.set_flag(flag, value);
        self.finish();
    }

    fn op_nop(&mut self) {
        if self.step == 1 {
            self.implied_cycle();
        } else {
            self.finish();
        }
    }

    /// `WDM`: a two-byte no-op whose second byte is read without VPA.
    fn op_wdm(&mut self) {
        if self.step == 1 {
            self.last_cycle();
            let addr = self.pc_address();
            self.idle_at(addr);
            self.regs.pc = self.regs.pc.wrapping_add(1);
        } else {
            self.finish();
        }
    }

    fn op_transfer(&mut self, from: Reg, to: Reg, wide: bool) {
        if self.step == 1 {
            self.implied_cycle();
            return;
        }
        let value = self.reg(from);
        if wide {
            self.set_reg(to, value);
            self.set_nz16(value);
        } else {
            let old = self.reg(to);
            self.set_reg(to, (old & 0xFF00) | (value & 0xFF));
            self.set_nz8(value as u8);
        }
        self.finish();
    }

    fn op_tcs(&mut self) {
        if self.step == 1 {
            self.implied_cycle();
            return;
        }
        self.regs.s = self.regs.a;
        self.fix_stack();
        self.finish();
    }

    fn op_txs(&mut self) {
        if self.step == 1 {
            self.implied_cycle();
            return;
        }
        if self.regs.e {
            self.regs.s = 0x0100 | (self.regs.x & 0xFF);
        } else {
            self.regs.s = self.regs.x;
        }
        self.finish();
    }

    fn op_xce(&mut self) {
        if self.step == 1 {
            self.implied_cycle();
            return;
        }
        let carry = self.regs.flag(FLAG_C);
        self.regs.set_flag(FLAG_C, self.regs.e);
        self.regs.e = carry;
        if self.regs.e {
            self.regs.p |= FLAG_M | FLAG_X;
            self.regs.x &= 0xFF;
            self.regs.y &= 0xFF;
            self.regs.s = 0x0100 | (self.regs.s & 0xFF);
        }
        self.finish();
    }

    fn op_xba(&mut self) {
        match self.step {
            1 => self.idle(),
            2 => {
                self.last_cycle();
                self.idle();
            }
            _ => {
                self.regs.a = self.regs.a.rotate_left(8);
                self.set_nz8(self.regs.a as u8);
                self.finish();
            }
        }
    }

    fn op_rep_sep(&mut self, set: bool) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.w = self.last16();
                self.last_cycle();
                self.idle();
            }
            _ => {
                if set {
                    self.regs.p |= self.w as u8;
                } else {
                    self.regs.p &= !(self.w as u8);
                }
                self.fix_p();
                self.finish();
            }
        }
    }

    // ── Stack ──

    fn op_push8(&mut self, value: u8) {
        match self.step {
            1 => self.idle(),
            2 => {
                self.last_cycle();
                self.push(value);
            }
            _ => self.finish(),
        }
    }

    fn op_push(&mut self, value: u16, wide: bool) {
        match self.step {
            1 => self.idle(),
            2 if wide => self.push((value >> 8) as u8),
            2 => {
                self.last_cycle();
                self.push(value as u8);
            }
            3 if wide => {
                self.last_cycle();
                self.push(value as u8);
            }
            _ => self.finish(),
        }
    }

    /// `PHD`.
    fn op_push_n(&mut self, value: u16) {
        match self.step {
            1 => self.idle(),
            2 => self.push_n((value >> 8) as u8),
            3 => {
                self.last_cycle();
                self.push_n(value as u8);
            }
            _ => {
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_pull(&mut self, r: Reg, wide: bool) {
        match self.step {
            1 | 2 => self.idle(),
            3 => {
                if !wide {
                    self.last_cycle();
                }
                self.pull();
            }
            4 if wide => {
                self.w = self.last16();
                self.last_cycle();
                self.pull();
            }
            4 => {
                let old = self.reg(r);
                self.set_reg(r, (old & 0xFF00) | u16::from(self.last));
                self.set_nz8(self.last);
                self.finish();
            }
            _ => {
                let value = (self.w | self.last16() << 8) as u16;
                self.set_reg(r, value);
                self.set_nz16(value);
                self.finish();
            }
        }
    }

    fn op_pld(&mut self) {
        match self.step {
            1 | 2 => self.idle(),
            3 => self.pull_n(),
            4 => {
                self.w = self.last16();
                self.last_cycle();
                self.pull_n();
            }
            _ => {
                self.regs.d = (self.w | self.last16() << 8) as u16;
                self.set_nz16(self.regs.d);
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_plb(&mut self) {
        match self.step {
            1 | 2 => self.idle(),
            3 => {
                self.last_cycle();
                self.pull_n();
            }
            _ => {
                self.regs.dbr = self.last;
                self.set_nz8(self.last);
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_plp(&mut self) {
        match self.step {
            1 | 2 => self.idle(),
            3 => {
                self.last_cycle();
                self.pull();
            }
            _ => {
                self.regs.p = self.last;
                self.fix_p();
                self.finish();
            }
        }
    }

    fn op_pea(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.w = self.last16();
                self.fetch();
            }
            3 => {
                self.w |= self.last16() << 8;
                self.push_n((self.w >> 8) as u8);
            }
            4 => {
                self.last_cycle();
                self.push_n(self.w as u8);
            }
            _ => {
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_pei(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.u = self.last16();
                if self.dl_nonzero() {
                    self.idle();
                } else {
                    self.skip();
                }
            }
            3 => self.read(self.dpn(self.u)),
            4 => {
                self.w = self.last16();
                self.read(self.dpn(self.u + 1));
            }
            5 => {
                self.w |= self.last16() << 8;
                self.push_n((self.w >> 8) as u8);
            }
            6 => {
                self.last_cycle();
                self.push_n(self.w as u8);
            }
            _ => {
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_per(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.v = self.last16();
                self.fetch();
            }
            3 => {
                self.v |= self.last16() << 8;
                self.idle();
            }
            4 => {
                self.w = u32::from(self.regs.pc.wrapping_add(self.v as u16));
                self.push_n((self.w >> 8) as u8);
            }
            5 => {
                self.last_cycle();
                self.push_n(self.w as u8);
            }
            _ => {
                self.fix_stack();
                self.finish();
            }
        }
    }

    // ── Program flow ──

    fn op_branch(&mut self, take: bool) {
        match self.step {
            1 => {
                if !take {
                    self.last_cycle();
                }
                self.fetch();
            }
            2 if !take => self.finish(),
            2 => {
                let offset = self.last as i8 as u16;
                self.v = u32::from(self.regs.pc.wrapping_add(offset));
                if self.regs.e && (self.regs.pc >> 8) != (self.v as u16 >> 8) {
                    self.idle();
                } else {
                    self.skip();
                }
            }
            3 => {
                self.last_cycle();
                self.idle();
            }
            _ => {
                self.regs.pc = self.v as u16;
                self.finish();
            }
        }
    }

    fn op_brl(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.u = self.last16();
                self.fetch();
            }
            3 => {
                self.u |= self.last16() << 8;
                self.v = u32::from(self.regs.pc.wrapping_add(self.u as u16));
                self.last_cycle();
                self.idle();
            }
            _ => {
                self.regs.pc = self.v as u16;
                self.finish();
            }
        }
    }

    fn op_jmp(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.w = self.last16();
                self.last_cycle();
                self.fetch();
            }
            _ => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.finish();
            }
        }
    }

    fn op_jml(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.v = self.last16();
                self.fetch();
            }
            3 => {
                self.v |= self.last16() << 8;
                self.last_cycle();
                self.fetch();
            }
            _ => {
                self.regs.pc = self.v as u16;
                self.regs.pbr = self.last;
                self.finish();
            }
        }
    }

    fn op_jmp_indirect(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.v = self.last16();
                self.fetch();
            }
            3 => {
                self.v |= self.last16() << 8;
                self.read(self.v & 0xFFFF);
            }
            4 => {
                self.w = self.last16();
                self.last_cycle();
                self.read((self.v + 1) & 0xFFFF);
            }
            _ => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.finish();
            }
        }
    }

    fn op_jmp_indexed_indirect(&mut self) {
        let pb = u32::from(self.regs.pbr) << 16;
        let x = u32::from(self.regs.x);
        match self.step {
            1 => self.fetch(),
            2 => {
                self.v = self.last16();
                self.fetch();
            }
            3 => {
                self.v |= self.last16() << 8;
                self.idle();
            }
            4 => self.read(pb | ((self.v + x) & 0xFFFF)),
            5 => {
                self.w = self.last16();
                self.last_cycle();
                self.read(pb | ((self.v + x + 1) & 0xFFFF));
            }
            _ => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.finish();
            }
        }
    }

    fn op_jml_indirect(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.u = self.last16();
                self.fetch();
            }
            3 => {
                self.u |= self.last16() << 8;
                self.read(self.u & 0xFFFF);
            }
            4 => {
                self.v = self.last16();
                self.read((self.u + 1) & 0xFFFF);
            }
            5 => {
                self.v |= self.last16() << 8;
                self.last_cycle();
                self.read((self.u + 2) & 0xFFFF);
            }
            _ => {
                self.regs.pc = self.v as u16;
                self.regs.pbr = self.last;
                self.finish();
            }
        }
    }

    fn op_jsr(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.w = self.last16();
                self.fetch();
            }
            3 => {
                self.w |= self.last16() << 8;
                self.idle();
            }
            4 => {
                self.regs.pc = self.regs.pc.wrapping_sub(1);
                self.push((self.regs.pc >> 8) as u8);
            }
            5 => {
                self.last_cycle();
                self.push(self.regs.pc as u8);
            }
            _ => {
                self.regs.pc = self.w as u16;
                self.finish();
            }
        }
    }

    fn op_jsl(&mut self) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.v = self.last16();
                self.fetch();
            }
            3 => {
                self.v |= self.last16() << 8;
                self.push_n(self.regs.pbr);
            }
            4 => self.idle(),
            5 => self.fetch(),
            6 => {
                self.v |= self.last16() << 16;
                self.regs.pc = self.regs.pc.wrapping_sub(1);
                self.push_n((self.regs.pc >> 8) as u8);
            }
            7 => {
                self.last_cycle();
                self.push_n(self.regs.pc as u8);
            }
            _ => {
                self.regs.pc = self.v as u16;
                self.regs.pbr = (self.v >> 16) as u8;
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_jsr_indexed_indirect(&mut self) {
        let pb = u32::from(self.regs.pbr) << 16;
        let x = u32::from(self.regs.x);
        match self.step {
            1 => self.fetch(),
            2 => {
                self.v = self.last16();
                self.push_n((self.regs.pc >> 8) as u8);
            }
            3 => self.push_n(self.regs.pc as u8),
            4 => self.fetch(),
            5 => {
                self.v |= self.last16() << 8;
                self.idle();
            }
            6 => self.read(pb | ((self.v + x) & 0xFFFF)),
            7 => {
                self.w = self.last16();
                self.last_cycle();
                self.read(pb | ((self.v + x + 1) & 0xFFFF));
            }
            _ => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_rts(&mut self) {
        match self.step {
            1 | 2 => self.idle(),
            3 => self.pull(),
            4 => {
                self.w = self.last16();
                self.pull();
            }
            5 => {
                self.w |= self.last16() << 8;
                self.last_cycle();
                self.idle();
            }
            _ => {
                self.regs.pc = (self.w as u16).wrapping_add(1);
                self.finish();
            }
        }
    }

    fn op_rtl(&mut self) {
        match self.step {
            1 | 2 => self.idle(),
            3 => self.pull_n(),
            4 => {
                self.v = self.last16();
                self.pull_n();
            }
            5 => {
                self.v |= self.last16() << 8;
                self.last_cycle();
                self.pull_n();
            }
            _ => {
                self.regs.pc = (self.v as u16).wrapping_add(1);
                self.regs.pbr = self.last;
                self.fix_stack();
                self.finish();
            }
        }
    }

    fn op_rti(&mut self) {
        match self.step {
            1 | 2 => self.idle(),
            3 => self.pull(),
            4 => {
                // The pulled status takes effect when the instruction ends:
                // the M and X outputs keep their old values until then.
                self.u = self.last16();
                self.pull();
            }
            5 => {
                self.w = self.last16();
                if self.regs.e {
                    self.last_cycle();
                }
                self.pull();
            }
            6 if self.regs.e => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.regs.p = self.u as u8;
                self.fix_p();
                self.finish();
            }
            6 => {
                self.w |= self.last16() << 8;
                self.last_cycle();
                self.pull();
            }
            _ => {
                self.regs.pc = self.w as u16;
                self.regs.pbr = self.last;
                self.regs.p = self.u as u8;
                self.fix_p();
                self.finish();
            }
        }
    }

    /// `BRK` and `COP`.
    fn op_brk(&mut self, vector: u16) {
        match self.step {
            1 => self.fetch(),
            2 => {
                if self.regs.e {
                    self.skip();
                } else {
                    self.push(self.regs.pbr);
                }
            }
            3 => self.push((self.regs.pc >> 8) as u8),
            4 => self.push(self.regs.pc as u8),
            5 => self.push(self.regs.p),
            6 => {
                self.enter_interrupt();
                self.read_vector(vector);
            }
            7 => {
                self.w = self.last16();
                self.last_cycle();
                self.read_vector(vector.wrapping_add(1));
            }
            _ => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.regs.pbr = 0;
                self.finish();
            }
        }
    }

    /// IRQ, NMI or ABORT. Like `BRK`, but the first two cycles fetch and
    /// discard, and emulation mode pushes P with B clear.
    pub(crate) fn op_interrupt(&mut self) {
        match self.step {
            0 => {
                let addr = self.pc_address();
                self.bus(addr, true, true, true);
            }
            1 => self.idle(),
            2 => {
                if self.regs.e {
                    self.skip();
                } else {
                    self.push(self.regs.pbr);
                }
            }
            3 => self.push((self.regs.pc >> 8) as u8),
            4 => self.push(self.regs.pc as u8),
            5 => {
                let p = if self.regs.e {
                    self.regs.p & !FLAG_X
                } else {
                    self.regs.p
                };
                self.push(p);
            }
            6 => {
                self.enter_interrupt();
                let vector = self.interrupt_vector();
                self.read_vector(vector);
            }
            7 => {
                self.w = self.last16();
                self.last_cycle();
                let vector = self.interrupt_vector().wrapping_add(1);
                self.read_vector(vector);
            }
            _ => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.regs.pbr = 0;
                if self.interrupt == Interrupt::Nmi || self.interrupt == Interrupt::Abort {
                    // An NMI or ABORT taken does not leave an IRQ sample
                    // behind; the next boundary samples afresh.
                }
                self.finish();
            }
        }
    }

    /// The reset sequence: two internal cycles, three stack cycles with the
    /// bus held to reads, and the vector.
    pub(crate) fn op_reset(&mut self) {
        match self.step {
            0 => {
                self.regs.e = true;
                self.regs.p = (self.regs.p | FLAG_M | FLAG_X | FLAG_I) & !crate::registers::FLAG_D;
                self.regs.x &= 0xFF;
                self.regs.y &= 0xFF;
                self.regs.d = 0;
                self.regs.dbr = 0;
                self.regs.pbr = 0;
                self.regs.s = 0x0100 | (self.regs.s & 0xFF);
                self.idle_at(self.pc_address());
            }
            1 => self.idle(),
            2..=4 => {
                self.read(u32::from(self.regs.s));
                self.vda = false;
                self.regs.s = 0x0100 | (self.regs.s.wrapping_sub(1) & 0xFF);
            }
            5 => self.read_vector(0xFFFC),
            6 => {
                self.w = self.last16();
                self.read_vector(0xFFFD);
            }
            _ => {
                self.regs.pc = (self.w | self.last16() << 8) as u16;
                self.finish();
            }
        }
    }

    fn op_block_move(&mut self, adjust: i16) {
        match self.step {
            1 => self.fetch(),
            2 => {
                self.u = self.last16();
                self.fetch();
            }
            3 => {
                self.v = self.last16();
                self.regs.dbr = self.u as u8;
                self.read(self.v << 16 | u32::from(self.regs.x));
            }
            4 => self.write(self.u << 16 | u32::from(self.regs.y), self.last),
            5 => {
                let adjust = adjust as u16;
                if self.x8 {
                    self.regs.x = (self.regs.x.wrapping_add(adjust)) & 0xFF;
                    self.regs.y = (self.regs.y.wrapping_add(adjust)) & 0xFF;
                } else {
                    self.regs.x = self.regs.x.wrapping_add(adjust);
                    self.regs.y = self.regs.y.wrapping_add(adjust);
                }
                self.idle();
            }
            6 => {
                self.last_cycle();
                self.idle();
            }
            _ => {
                if self.regs.a != 0 {
                    self.regs.pc = self.regs.pc.wrapping_sub(3);
                }
                self.regs.a = self.regs.a.wrapping_sub(1);
                self.finish();
            }
        }
    }

    fn op_wai(&mut self) {
        match self.step {
            1 => self.idle(),
            2 => {
                self.last_cycle();
                self.idle();
            }
            _ => {
                self.waiting = true;
                self.vda = false;
                self.vpa = false;
            }
        }
    }

    fn op_stp(&mut self) {
        match self.step {
            1 => self.idle(),
            2 => self.idle(),
            _ => {
                self.stopped = true;
                self.vda = false;
                self.vpa = false;
            }
        }
    }
}
