//! Instruction execution for the Z80.
//!
//! STRIPPED DOWN FOR DEBUGGING - panics on unimplemented opcodes.

#![allow(clippy::too_many_lines)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]

use crate::alu;
use crate::flags::{sz53p, CF, HF, NF, PF, SF, XF, YF, ZF};
use crate::microcode::MicroOp;

use super::Z80;

impl Z80 {
    /// Execute unprefixed instruction.
    pub(super) fn execute_unprefixed(&mut self) {
        let op = self.opcode;

        match op {
            // NOP
            0x00 => {}

            // LD BC, nn
            0x01 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LD (BC), A
            0x02 => {
                self.addr = self.regs.bc();
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // INC BC
            0x03 => {
                self.queue_internal(2);
                self.regs.set_bc(self.regs.bc().wrapping_add(1));
            }

            // INC B
            0x04 => {
                let result = alu::inc8(self.regs.b);
                self.regs.b = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC B
            0x05 => {
                let result = alu::dec8(self.regs.b);
                self.regs.b = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // LD B, n
            0x06 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // RLCA
            0x07 => {
                let carry = self.regs.a >> 7;
                self.regs.a = (self.regs.a << 1) | carry;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if carry != 0 { CF } else { 0 };
            }

            // DEC BC
            0x0B => {
                self.queue_internal(2);
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));
            }

            // INC C
            0x0C => {
                let result = alu::inc8(self.regs.c);
                self.regs.c = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC C
            0x0D => {
                let result = alu::dec8(self.regs.c);
                self.regs.c = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // ADD HL, BC
            0x09 => {
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.bc());
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // LD A, (BC)
            0x0A => {
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LD C, n
            0x0E => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // RRCA
            0x0F => {
                let carry = self.regs.a & 1;
                self.regs.a = (self.regs.a >> 1) | (carry << 7);
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if carry != 0 { CF } else { 0 };
            }

            // RLA - rotate left A through carry
            0x17 => {
                let old_carry = if self.regs.f & CF != 0 { 1 } else { 0 };
                let new_carry = self.regs.a >> 7;
                self.regs.a = (self.regs.a << 1) | old_carry;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if new_carry != 0 { CF } else { 0 };
            }

            // RRA - rotate right A through carry
            0x1F => {
                let old_carry = if self.regs.f & CF != 0 { 0x80 } else { 0 };
                let new_carry = self.regs.a & 1;
                self.regs.a = (self.regs.a >> 1) | old_carry;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if new_carry != 0 { CF } else { 0 };
            }

            // DJNZ e (Decrement B and Jump if Not Zero)
            0x10 => {
                self.queue_internal(1); // 1 extra T-state for internal processing
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD DE, nn
            0x11 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LD (DE), A
            0x12 => {
                self.addr = self.regs.de();
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // JR e (unconditional relative jump)
            0x18 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // JR NZ, e (relative jump if not zero)
            0x20 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // JR Z, e (relative jump if zero)
            0x28 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // JR NC, e (relative jump if no carry)
            0x30 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // JR C, e (relative jump if carry)
            0x38 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // INC DE
            0x13 => {
                self.queue_internal(2);
                self.regs.set_de(self.regs.de().wrapping_add(1));
            }

            // INC D
            0x14 => {
                let result = alu::inc8(self.regs.d);
                self.regs.d = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC D
            0x15 => {
                let result = alu::dec8(self.regs.d);
                self.regs.d = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // LD D, n
            0x16 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD E, n
            0x1E => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // INC E
            0x1C => {
                let result = alu::inc8(self.regs.e);
                self.regs.e = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC E
            0x1D => {
                let result = alu::dec8(self.regs.e);
                self.regs.e = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // INC DE
            0x13 => {
                self.queue_internal(2);
                self.regs.set_de(self.regs.de().wrapping_add(1));
            }

            // LD A, (DE)
            0x1A => {
                self.addr = self.regs.de();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // DEC DE
            0x1B => {
                self.queue_internal(2);
                self.regs.set_de(self.regs.de().wrapping_sub(1));
            }

            // ADD HL, DE
            0x19 => {
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.de());
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // LD HL, nn
            0x21 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // ADD HL, HL
            0x29 => {
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.hl());
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // LD (nn), HL
            0x22 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // INC HL
            0x23 => {
                self.queue_internal(2);
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
            }

            // INC H
            0x24 => {
                let result = alu::inc8(self.regs.h);
                self.regs.h = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC H
            0x25 => {
                let result = alu::dec8(self.regs.h);
                self.regs.h = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // LD H, n
            0x26 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // INC L
            0x2C => {
                let result = alu::inc8(self.regs.l);
                self.regs.l = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC L
            0x2D => {
                let result = alu::dec8(self.regs.l);
                self.regs.l = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // LD HL, (nn)
            0x2A => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // DEC HL
            0x2B => {
                self.queue_internal(2);
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
            }

            // DAA - Decimal Adjust Accumulator
            0x27 => {
                let a = self.regs.a;
                let nf = self.regs.f & NF != 0;
                let cf = self.regs.f & CF != 0;
                let hf = self.regs.f & HF != 0;

                let mut correction: u8 = 0;
                let mut new_cf = cf;

                // Low nibble correction
                if hf || (a & 0x0F) > 9 {
                    correction |= 0x06;
                }

                // High nibble correction
                if cf || a > 0x99 {
                    correction |= 0x60;
                    new_cf = true;
                }

                // Apply correction
                let result = if nf {
                    a.wrapping_sub(correction)
                } else {
                    a.wrapping_add(correction)
                };

                // H flag:
                // After addition: set if (original A & 0x0F) > 9
                // After subtraction: set if original H AND (original A & 0x0F) < 6
                let new_hf = if nf {
                    hf && (a & 0x0F) < 6
                } else {
                    (a & 0x0F) > 9
                };

                self.regs.a = result;
                self.regs.f = sz53p(result)
                    | if nf { NF } else { 0 }
                    | if new_cf { CF } else { 0 }
                    | if new_hf { HF } else { 0 };
            }

            // CPL
            0x2F => {
                self.regs.a = !self.regs.a;
                self.regs.f = (self.regs.f & (SF | ZF | PF | CF))
                    | HF
                    | NF
                    | (self.regs.a & (XF | YF));
            }

            // LD L, n
            0x2E => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD SP, nn
            0x31 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // INC (HL)
            0x34 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // DEC (HL)
            0x35 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LD (nn), A
            0x32 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LD A, (nn)
            0x3A => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // INC A
            0x3C => {
                let result = alu::inc8(self.regs.a);
                self.regs.a = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC A
            0x3D => {
                let result = alu::dec8(self.regs.a);
                self.regs.a = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // INC SP
            0x33 => {
                self.queue_internal(2);
                self.regs.sp = self.regs.sp.wrapping_add(1);
            }

            // LD (HL), n
            0x36 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // DEC SP
            0x3B => {
                self.queue_internal(2);
                self.regs.sp = self.regs.sp.wrapping_sub(1);
            }

            // SCF (Set Carry Flag)
            0x37 => {
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | CF
                    | (self.regs.a & (XF | YF));
            }

            // CCF (Complement Carry Flag)
            0x3F => {
                let old_cf = self.regs.f & CF;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (XF | YF))
                    | if old_cf != 0 { HF } else { CF };
            }

            // ADD HL, SP
            0x39 => {
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.sp);
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // LD A, n
            0x3E => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD B, r and LD C, r etc - register to register moves
            0x40..=0x7F if op != 0x76 => {
                let src = op & 7;
                let dst = (op >> 3) & 7;
                if src == 6 {
                    // LD r, (HL)
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else if dst == 6 {
                    // LD (HL), r
                    self.addr = self.regs.hl();
                    self.data_lo = self.get_reg8(src);
                    self.micro_ops.push(MicroOp::WriteMem);
                } else {
                    // LD r, r
                    let value = self.get_reg8(src);
                    self.set_reg8(dst, value);
                }
            }

            // HALT
            0x76 => {
                self.regs.halted = true;
            }

            // ADD A, r
            0x80..=0x87 => {
                let r = op & 7;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    let result = alu::add8(self.regs.a, value, false);
                    self.regs.a = result.value;
                    self.regs.f = result.flags;
                }
            }

            // ADC A, r
            0x88..=0x8F => {
                let r = op & 7;
                let carry = self.regs.f & CF != 0;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    let result = alu::add8(self.regs.a, value, carry);
                    self.regs.a = result.value;
                    self.regs.f = result.flags;
                }
            }

            // SUB r
            0x90..=0x97 => {
                let r = op & 7;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    let result = alu::sub8(self.regs.a, value, false);
                    self.regs.a = result.value;
                    self.regs.f = result.flags;
                }
            }

            // SBC A, r
            0x98..=0x9F => {
                let r = op & 7;
                let carry = self.regs.f & CF != 0;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    let result = alu::sub8(self.regs.a, value, carry);
                    self.regs.a = result.value;
                    self.regs.f = result.flags;
                }
            }

            // AND r
            0xA0..=0xA7 => {
                let r = op & 7;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    self.regs.a &= value;
                    self.regs.f = sz53p(self.regs.a) | HF;
                }
            }

            // XOR r
            0xA8..=0xAF => {
                let r = op & 7;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    self.regs.a ^= value;
                    self.regs.f = sz53p(self.regs.a);
                }
            }

            // OR r
            0xB0..=0xB7 => {
                let r = op & 7;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    self.regs.a |= value;
                    self.regs.f = sz53p(self.regs.a);
                }
            }

            // CP r
            0xB8..=0xBF => {
                let r = op & 7;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    let result = alu::sub8(self.regs.a, value, false);
                    // CP doesn't store result, just sets flags
                    // But undocumented flags come from operand, not result
                    self.regs.f = (result.flags & !(YF | XF)) | (value & (YF | XF));
                }
            }

            // RET NZ
            0xC0 => {
                self.queue_internal(1);
                if self.regs.f & ZF == 0 {
                    self.addr = self.regs.sp;
                    self.micro_ops.push(MicroOp::ReadMem16Lo);
                    self.micro_ops.push(MicroOp::ReadMem16Hi);
                    self.queue_execute_followup();
                }
            }

            // POP BC
            0xC1 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // JP NZ, nn
            0xC2 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // JP nn
            0xC3 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // CALL NZ, nn
            0xC4 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // PUSH BC
            0xC5 => {
                self.queue_internal(1);
                // WriteMemHiFirst/WriteMemLoSecond handle SP decrement and write to SP
                self.data_hi = self.regs.b;
                self.data_lo = self.regs.c;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            // ADD A, n
            0xC6 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // ADC A, n
            0xCE => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // RET Z
            0xC8 => {
                self.queue_internal(1);
                if self.regs.f & ZF != 0 {
                    self.addr = self.regs.sp;
                    self.micro_ops.push(MicroOp::ReadMem16Lo);
                    self.micro_ops.push(MicroOp::ReadMem16Hi);
                    self.queue_execute_followup();
                }
            }

            // RET
            0xC9 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // JP Z, nn
            0xCA => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // CB prefix
            0xCB => {
                self.prefix = 0xCB;
                self.micro_ops.push(MicroOp::FetchOpcode);
            }

            // CALL nn
            0xCD => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // POP DE
            0xD1 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // PUSH DE
            0xD5 => {
                self.queue_internal(1);
                // WriteMemHiFirst/WriteMemLoSecond handle SP decrement and write to SP
                self.data_hi = self.regs.d;
                self.data_lo = self.regs.e;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            // SUB n
            0xD6 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // SBC A, n
            0xDE => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // RET NC
            0xD0 => {
                self.queue_internal(1);
                if self.regs.f & CF == 0 {
                    self.addr = self.regs.sp;
                    self.micro_ops.push(MicroOp::ReadMem16Lo);
                    self.micro_ops.push(MicroOp::ReadMem16Hi);
                    self.queue_execute_followup();
                }
            }

            // JP NC, nn
            0xD2 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // CALL NC, nn
            0xD4 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // RET C
            0xD8 => {
                self.queue_internal(1);
                if self.regs.f & CF != 0 {
                    self.addr = self.regs.sp;
                    self.micro_ops.push(MicroOp::ReadMem16Lo);
                    self.micro_ops.push(MicroOp::ReadMem16Hi);
                    self.queue_execute_followup();
                }
            }

            // JP C, nn
            0xDA => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // CALL C, nn
            0xDC => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // POP HL
            0xE1 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // PUSH HL
            0xE5 => {
                self.queue_internal(1);
                // WriteMemHiFirst/WriteMemLoSecond handle SP decrement and write to SP
                self.data_hi = self.regs.h;
                self.data_lo = self.regs.l;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            // AND n
            0xE6 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // XOR n
            0xEE => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // JP (HL)
            0xE9 => {
                self.regs.pc = self.regs.hl();
            }

            // EX DE, HL
            0xEB => {
                let tmp = self.regs.de();
                self.regs.set_de(self.regs.hl());
                self.regs.set_hl(tmp);
            }

            // ED prefix
            0xED => {
                self.prefix = 0xED;
                self.micro_ops.push(MicroOp::FetchOpcode);
            }

            // CP n
            0xFE => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // POP AF
            0xF1 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // PUSH AF
            0xF5 => {
                self.queue_internal(1);
                // WriteMemHiFirst/WriteMemLoSecond handle SP decrement and write to SP
                self.data_hi = self.regs.a;
                self.data_lo = self.regs.f;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            // DI
            0xF3 => {
                self.regs.iff1 = false;
                self.regs.iff2 = false;
            }

            // EI
            0xFB => {
                self.regs.iff1 = true;
                self.regs.iff2 = true;
            }

            // OR n
            0xF6 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD SP, HL
            0xF9 => {
                self.queue_internal(2);
                self.regs.sp = self.regs.hl();
            }

            _ => {
                panic!(
                    "Unimplemented opcode: {:02X} at PC={:04X}",
                    op,
                    self.regs.pc.wrapping_sub(1)
                );
            }
        }
    }

    /// Execute follow-up for instructions that need immediate/memory data.
    pub(super) fn execute_followup(&mut self) {
        // Dispatch based on prefix
        if self.prefix == 0xED {
            self.execute_ed_followup();
            return;
        }
        if (self.prefix == 0xDD || self.prefix == 0xFD) && self.prefix2 == 0xCB {
            self.execute_ddcb_fdcb_followup();
            return;
        }
        if self.prefix == 0xDD || self.prefix == 0xFD {
            self.execute_dd_fd_followup();
            return;
        }
        if self.prefix == 0xCB {
            self.execute_cb_followup();
            return;
        }

        let op = self.opcode;

        match op {
            // LD BC, nn
            0x01 => {
                self.regs.c = self.data_lo;
                self.regs.b = self.data_hi;
            }

            // LD B, n
            0x06 => {
                self.regs.b = self.data_lo;
            }

            // LD C, n
            0x0E => {
                self.regs.c = self.data_lo;
            }

            // DJNZ e (Decrement B and Jump if Not Zero)
            0x10 => {
                self.regs.b = self.regs.b.wrapping_sub(1);
                if self.regs.b != 0 {
                    self.queue_internal(5); // 5 extra T-states when branch taken
                    let displacement = self.data_lo as i8;
                    self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                }
            }

            // LD D, n
            0x16 => {
                self.regs.d = self.data_lo;
            }

            // LD E, n
            0x1E => {
                self.regs.e = self.data_lo;
            }

            // JR e (unconditional relative jump)
            0x18 => {
                // Displacement is signed, add 5 internal cycles for taken jump
                self.queue_internal(5);
                let displacement = self.data_lo as i8;
                self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
            }

            // JR NZ, e (relative jump if not zero)
            0x20 => {
                if self.regs.f & ZF == 0 {
                    self.queue_internal(5);
                    let displacement = self.data_lo as i8;
                    self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                }
            }

            // JR Z, e (relative jump if zero)
            0x28 => {
                if self.regs.f & ZF != 0 {
                    self.queue_internal(5);
                    let displacement = self.data_lo as i8;
                    self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                }
            }

            // JR NC, e (relative jump if no carry)
            0x30 => {
                if self.regs.f & CF == 0 {
                    self.queue_internal(5);
                    let displacement = self.data_lo as i8;
                    self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                }
            }

            // JR C, e (relative jump if carry)
            0x38 => {
                if self.regs.f & CF != 0 {
                    self.queue_internal(5);
                    let displacement = self.data_lo as i8;
                    self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                }
            }

            // LD DE, nn
            0x11 => {
                self.regs.e = self.data_lo;
                self.regs.d = self.data_hi;
            }

            // LD A, (BC)
            0x0A => {
                self.regs.a = self.data_lo;
            }

            // LD A, (DE)
            0x1A => {
                self.regs.a = self.data_lo;
            }

            // LD HL, nn
            0x21 => {
                self.regs.l = self.data_lo;
                self.regs.h = self.data_hi;
            }

            // LD (nn), HL
            0x22 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.l;
                self.data_hi = self.regs.h;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD H, n
            0x26 => {
                self.regs.h = self.data_lo;
            }

            // LD HL, (nn) - second stage: data read from memory, load into HL
            0x2A if self.followup_stage >= 2 => {
                self.regs.l = self.data_lo;
                self.regs.h = self.data_hi;
            }

            // LD HL, (nn) - first stage: set up memory read from immediate address
            0x2A => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // LD L, n
            0x2E => {
                self.regs.l = self.data_lo;
            }

            // LD SP, nn
            0x31 => {
                self.regs.sp = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // INC (HL)
            0x34 => {
                self.queue_internal(1); // Extra cycle for read-modify-write
                let result = alu::inc8(self.data_lo);
                self.data_lo = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // DEC (HL)
            0x35 => {
                self.queue_internal(1); // Extra cycle for read-modify-write
                let result = alu::dec8(self.data_lo);
                self.data_lo = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // LD (nn), A
            0x32 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // LD A, (nn) - second stage: load byte into A
            0x3A if self.followup_stage >= 2 => {
                self.regs.a = self.data_lo;
            }

            // LD A, (nn) - first stage: set up memory read
            0x3A => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LD A, n (followup)
            0x3E => {
                self.regs.a = self.data_lo;
            }

            // LD (HL), n
            0x36 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // LD A, n
            0x3E => {
                self.regs.a = self.data_lo;
            }

            // LD r, (HL)
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                let dst = (op >> 3) & 7;
                self.set_reg8(dst, self.data_lo);
            }

            // ADD/ADC/SUB/SBC/AND/XOR/OR/CP (HL)
            0x86 => {
                let result = alu::add8(self.regs.a, self.data_lo, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0x8E => {
                let carry = self.regs.f & CF != 0;
                let result = alu::add8(self.regs.a, self.data_lo, carry);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0x96 => {
                let result = alu::sub8(self.regs.a, self.data_lo, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0x9E => {
                let carry = self.regs.f & CF != 0;
                let result = alu::sub8(self.regs.a, self.data_lo, carry);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0xA6 => {
                self.regs.a &= self.data_lo;
                self.regs.f = sz53p(self.regs.a) | HF;
            }
            0xAE => {
                self.regs.a ^= self.data_lo;
                self.regs.f = sz53p(self.regs.a);
            }
            0xB6 => {
                self.regs.a |= self.data_lo;
                self.regs.f = sz53p(self.regs.a);
            }
            0xBE => {
                let result = alu::sub8(self.regs.a, self.data_lo, false);
                self.regs.f = (result.flags & !(YF | XF)) | (self.data_lo & (YF | XF));
            }

            // RET NZ/Z/NC (conditional returns)
            0xC0 | 0xC8 | 0xD0 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // POP BC
            0xC1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.c = self.data_lo;
                self.regs.b = self.data_hi;
            }

            // JP NZ, nn
            0xC2 => {
                if self.regs.f & ZF == 0 {
                    self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                }
            }

            // JP nn
            0xC3 => {
                self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // ADD A, n
            0xC6 => {
                let result = alu::add8(self.regs.a, self.data_lo, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // ADC A, n
            0xCE => {
                let carry = self.regs.f & CF != 0;
                let result = alu::add8(self.regs.a, self.data_lo, carry);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // RET
            0xC9 => {
                let ret_addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.pc = ret_addr;
            }

            // JP Z, nn
            0xCA => {
                if self.regs.f & ZF != 0 {
                    self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                }
            }

            // CALL NZ, nn
            0xC4 => {
                // Save target address before we overwrite data_lo/data_hi
                let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                if self.regs.f & ZF == 0 {
                    self.queue_internal(1);
                    // WriteMemHiFirst/WriteMemLoSecond handle SP decrement
                    let ret_addr = self.regs.pc;
                    self.data_hi = (ret_addr >> 8) as u8;
                    self.data_lo = ret_addr as u8;
                    self.micro_ops.push(MicroOp::WriteMemHiFirst);
                    self.micro_ops.push(MicroOp::WriteMemLoSecond);
                    self.regs.pc = target;
                }
            }

            // CALL nn
            0xCD => {
                // Save target address before we overwrite data_lo/data_hi
                let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.queue_internal(1);
                // WriteMemHiFirst/WriteMemLoSecond handle SP decrement
                let ret_addr = self.regs.pc;
                self.data_hi = (ret_addr >> 8) as u8;
                self.data_lo = ret_addr as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
                self.regs.pc = target;
            }

            // POP DE
            0xD1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.e = self.data_lo;
                self.regs.d = self.data_hi;
            }

            // JP NC, nn
            0xD2 => {
                if self.regs.f & CF == 0 {
                    self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                }
            }

            // CALL NC, nn
            0xD4 => {
                // Save target address before we overwrite data_lo/data_hi
                let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                if self.regs.f & CF == 0 {
                    self.queue_internal(1);
                    // WriteMemHiFirst/WriteMemLoSecond handle SP decrement
                    let ret_addr = self.regs.pc;
                    self.data_hi = (ret_addr >> 8) as u8;
                    self.data_lo = ret_addr as u8;
                    self.micro_ops.push(MicroOp::WriteMemHiFirst);
                    self.micro_ops.push(MicroOp::WriteMemLoSecond);
                    self.regs.pc = target;
                }
            }

            // RET C (conditional return)
            0xD8 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // JP C, nn
            0xDA => {
                if self.regs.f & CF != 0 {
                    self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                }
            }

            // CALL C, nn
            0xDC => {
                // Save target address before we overwrite data_lo/data_hi
                let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                if self.regs.f & CF != 0 {
                    self.queue_internal(1);
                    let ret_addr = self.regs.pc;
                    self.data_hi = (ret_addr >> 8) as u8;
                    self.data_lo = ret_addr as u8;
                    self.micro_ops.push(MicroOp::WriteMemHiFirst);
                    self.micro_ops.push(MicroOp::WriteMemLoSecond);
                    self.regs.pc = target;
                }
            }

            // SUB n
            0xD6 => {
                let result = alu::sub8(self.regs.a, self.data_lo, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // SBC A, n
            0xDE => {
                let carry = self.regs.f & CF != 0;
                let result = alu::sub8(self.regs.a, self.data_lo, carry);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // POP HL
            0xE1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.l = self.data_lo;
                self.regs.h = self.data_hi;
            }

            // AND n
            0xE6 => {
                self.regs.a &= self.data_lo;
                self.regs.f = sz53p(self.regs.a) | HF;
            }

            // XOR n
            0xEE => {
                self.regs.a ^= self.data_lo;
                self.regs.f = sz53p(self.regs.a);
            }

            // POP AF
            0xF1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.f = self.data_lo;
                self.regs.a = self.data_hi;
            }

            // OR n
            0xF6 => {
                self.regs.a |= self.data_lo;
                self.regs.f = sz53p(self.regs.a);
            }

            // CP n
            0xFE => {
                let result = alu::sub8(self.regs.a, self.data_lo, false);
                self.regs.f = (result.flags & !(YF | XF)) | (self.data_lo & (YF | XF));
            }

            _ => {
                panic!(
                    "Unimplemented followup: opcode={:02X} PC={:04X}",
                    op, self.regs.pc
                );
            }
        }
    }

    /// Execute CB-prefixed instruction.
    pub(super) fn execute_cb(&mut self) {
        let op = self.opcode;
        let r = op & 7;

        // For (HL) operations, need memory access
        if r == 6 {
            self.addr = self.regs.hl();
            self.micro_ops.push(MicroOp::ReadMem);
            self.queue_internal(1);
            self.queue_execute_followup();
            return;
        }

        // Register operations
        let value = self.get_reg8(r);
        let result = self.execute_cb_operation(op, value);

        if let Some(res) = result {
            self.set_reg8(r, res);
        }
    }

    /// Execute CB-prefixed followup for (HL) operations.
    fn execute_cb_followup(&mut self) {
        let op = self.opcode;
        let value = self.data_lo;

        let result = self.execute_cb_operation(op, value);

        // Write back if not BIT operation
        if let Some(res) = result {
            self.data_lo = res;
            self.micro_ops.push(MicroOp::WriteMem);
        }
    }

    /// Execute CB operation, returns Some(result) for write-back or None for BIT.
    fn execute_cb_operation(&mut self, op: u8, value: u8) -> Option<u8> {
        match op & 0xF8 {
            // RLC
            0x00 => {
                let res = alu::rlc8(value);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // RRC
            0x08 => {
                let res = alu::rrc8(value);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // RL
            0x10 => {
                let res = alu::rl8(value, self.regs.f & CF != 0);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // RR
            0x18 => {
                let res = alu::rr8(value, self.regs.f & CF != 0);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // SLA
            0x20 => {
                let res = alu::sla8(value);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // SRA
            0x28 => {
                let res = alu::sra8(value);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // SLL (undocumented)
            0x30 => {
                let res = alu::sll8(value);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // SRL
            0x38 => {
                let res = alu::srl8(value);
                self.regs.f = res.flags;
                Some(res.value)
            }
            // BIT
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                let bit = (op >> 3) & 7;
                let mask = 1 << bit;
                let is_zero = value & mask == 0;

                let mut flags = self.regs.f & CF; // Preserve carry
                flags |= HF; // H is set
                if is_zero {
                    flags |= ZF | PF; // Z and P/V are set if bit is 0
                }
                if bit == 7 && !is_zero {
                    flags |= SF; // S is set if bit 7 is tested and is 1
                }
                // Undocumented: X and Y flags from tested value
                flags |= value & (XF | YF);
                self.regs.f = flags;
                None // BIT doesn't write back
            }
            // RES
            0x80 | 0x88 | 0x90 | 0x98 | 0xA0 | 0xA8 | 0xB0 | 0xB8 => {
                let bit = (op >> 3) & 7;
                Some(value & !(1 << bit))
            }
            // SET
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
                let bit = (op >> 3) & 7;
                Some(value | (1 << bit))
            }
            _ => unreachable!(),
        }
    }

    /// Execute DD/FD-prefixed instruction.
    pub(super) fn execute_dd_fd(&mut self) {
        let op = self.opcode;
        let is_iy = self.prefix == 0xFD;

        match op {
            // POP IX/IY
            0xE1 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // PUSH IX/IY
            0xE5 => {
                self.queue_internal(1);
                // WriteMemHiFirst/WriteMemLoSecond handle SP decrement
                if is_iy {
                    self.data_hi = (self.regs.iy >> 8) as u8;
                    self.data_lo = self.regs.iy as u8;
                } else {
                    self.data_hi = (self.regs.ix >> 8) as u8;
                    self.data_lo = self.regs.ix as u8;
                }
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            // ADD IX/IY, BC
            0x09 => {
                self.queue_internal(7);
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                let (result, flags) = alu::add16(idx, self.regs.bc());
                if is_iy {
                    self.regs.iy = result;
                } else {
                    self.regs.ix = result;
                }
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // ADD IX/IY, DE
            0x19 => {
                self.queue_internal(7);
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                let (result, flags) = alu::add16(idx, self.regs.de());
                if is_iy {
                    self.regs.iy = result;
                } else {
                    self.regs.ix = result;
                }
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // LD IX/IY, nn
            0x21 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LD (nnnn), IX/IY
            0x22 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LD IX/IY, (nnnn)
            0x2A => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // INC IX/IY
            0x23 => {
                self.queue_internal(2);
                if is_iy {
                    self.regs.iy = self.regs.iy.wrapping_add(1);
                } else {
                    self.regs.ix = self.regs.ix.wrapping_add(1);
                }
            }

            // DEC IX/IY
            0x2B => {
                self.queue_internal(2);
                if is_iy {
                    self.regs.iy = self.regs.iy.wrapping_sub(1);
                } else {
                    self.regs.ix = self.regs.ix.wrapping_sub(1);
                }
            }

            // ADD IX/IY, IX/IY
            0x29 => {
                self.queue_internal(7);
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                let (result, flags) = alu::add16(idx, idx);
                if is_iy {
                    self.regs.iy = result;
                } else {
                    self.regs.ix = result;
                }
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // ADD IX/IY, SP
            0x39 => {
                self.queue_internal(7);
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                let (result, flags) = alu::add16(idx, self.regs.sp);
                if is_iy {
                    self.regs.iy = result;
                } else {
                    self.regs.ix = result;
                }
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            // ALU operations with IXH/IXL/IYH/IYL (undocumented)
            // ADD A, IXH/IXL
            0x84 | 0x85 => {
                let value = if op == 0x84 {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                let result = alu::add8(self.regs.a, value, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // ADC A, IXH/IXL
            0x8C | 0x8D => {
                let value = if op == 0x8C {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                let carry = self.regs.f & CF != 0;
                let result = alu::add8(self.regs.a, value, carry);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // SUB IXH/IXL
            0x94 | 0x95 => {
                let value = if op == 0x94 {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                let result = alu::sub8(self.regs.a, value, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // SBC A, IXH/IXL
            0x9C | 0x9D => {
                let value = if op == 0x9C {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                let carry = self.regs.f & CF != 0;
                let result = alu::sub8(self.regs.a, value, carry);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // AND IXH/IXL
            0xA4 | 0xA5 => {
                let value = if op == 0xA4 {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                self.regs.a &= value;
                self.regs.f = sz53p(self.regs.a) | HF;
            }

            // XOR IXH/IXL
            0xAC | 0xAD => {
                let value = if op == 0xAC {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                self.regs.a ^= value;
                self.regs.f = sz53p(self.regs.a);
            }

            // OR IXH/IXL
            0xB4 | 0xB5 => {
                let value = if op == 0xB4 {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                self.regs.a |= value;
                self.regs.f = sz53p(self.regs.a);
            }

            // CP IXH/IXL
            0xBC | 0xBD => {
                let value = if op == 0xBC {
                    if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }
                } else {
                    if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }
                };
                let result = alu::sub8(self.regs.a, value, false);
                self.regs.f = (result.flags & !(YF | XF)) | (value & (YF | XF));
            }

            // INC IXH/IYH (undocumented)
            0x24 => {
                let value = if is_iy {
                    (self.regs.iy >> 8) as u8
                } else {
                    (self.regs.ix >> 8) as u8
                };
                let result = alu::inc8(value);
                if is_iy {
                    self.regs.iy = (self.regs.iy & 0x00FF) | ((result.value as u16) << 8);
                } else {
                    self.regs.ix = (self.regs.ix & 0x00FF) | ((result.value as u16) << 8);
                }
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC IXH/IYH (undocumented)
            0x25 => {
                let value = if is_iy {
                    (self.regs.iy >> 8) as u8
                } else {
                    (self.regs.ix >> 8) as u8
                };
                let result = alu::dec8(value);
                if is_iy {
                    self.regs.iy = (self.regs.iy & 0x00FF) | ((result.value as u16) << 8);
                } else {
                    self.regs.ix = (self.regs.ix & 0x00FF) | ((result.value as u16) << 8);
                }
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // INC IXL/IYL (undocumented)
            0x2C => {
                let value = if is_iy {
                    self.regs.iy as u8
                } else {
                    self.regs.ix as u8
                };
                let result = alu::inc8(value);
                if is_iy {
                    self.regs.iy = (self.regs.iy & 0xFF00) | (result.value as u16);
                } else {
                    self.regs.ix = (self.regs.ix & 0xFF00) | (result.value as u16);
                }
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // DEC IXL/IYL (undocumented)
            0x2D => {
                let value = if is_iy {
                    self.regs.iy as u8
                } else {
                    self.regs.ix as u8
                };
                let result = alu::dec8(value);
                if is_iy {
                    self.regs.iy = (self.regs.iy & 0xFF00) | (result.value as u16);
                } else {
                    self.regs.ix = (self.regs.ix & 0xFF00) | (result.value as u16);
                }
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            // INC (IX+d)/(IY+d)
            0x34 => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            // DEC (IX+d)/(IY+d)
            0x35 => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            // LD (IX+d)/(IY+d), n
            0x36 => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD IXH/IYH, n (undocumented)
            0x26 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD IXL/IYL, n (undocumented)
            0x2E => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD r, (IX+d)/(IY+d) - B, C, D, E, H, L, A
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            // LD (IX+d)/(IY+d), r - B, C, D, E, H, L, A
            0x70 | 0x71 | 0x72 | 0x73 | 0x74 | 0x75 | 0x77 => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            // Undocumented LD r, r' with IXH/IXL/IYH/IYL substitution
            // Excludes: 0x46/4E/56/5E/66/6E/7E (LD r,(IX+d)) and 0x76 (HALT)
            0x40..=0x7F => {
                let src = op & 0x07;
                let dst = (op >> 3) & 0x07;
                // 6 = (HL) which uses indexed addressing (already handled above)
                // This handles all other register-to-register loads with IXH/IXL substitution
                let src_val = match src {
                    0 => self.regs.b,
                    1 => self.regs.c,
                    2 => self.regs.d,
                    3 => self.regs.e,
                    4 => if is_iy { (self.regs.iy >> 8) as u8 } else { (self.regs.ix >> 8) as u8 }, // IXH/IYH
                    5 => if is_iy { self.regs.iy as u8 } else { self.regs.ix as u8 }, // IXL/IYL
                    7 => self.regs.a,
                    _ => unreachable!(), // 6 is handled by other patterns
                };
                match dst {
                    0 => self.regs.b = src_val,
                    1 => self.regs.c = src_val,
                    2 => self.regs.d = src_val,
                    3 => self.regs.e = src_val,
                    4 => {
                        // IXH/IYH
                        if is_iy {
                            self.regs.iy = (self.regs.iy & 0x00FF) | ((src_val as u16) << 8);
                        } else {
                            self.regs.ix = (self.regs.ix & 0x00FF) | ((src_val as u16) << 8);
                        }
                    }
                    5 => {
                        // IXL/IYL
                        if is_iy {
                            self.regs.iy = (self.regs.iy & 0xFF00) | (src_val as u16);
                        } else {
                            self.regs.ix = (self.regs.ix & 0xFF00) | (src_val as u16);
                        }
                    }
                    7 => self.regs.a = src_val,
                    _ => unreachable!(), // 6 is handled by other patterns
                }
            }

            // ALU operations with (IX+d)/(IY+d)
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            _ => {
                panic!(
                    "Unimplemented DD/FD opcode: {:02X} (prefix={:02X}) at PC={:04X}",
                    op,
                    self.prefix,
                    self.regs.pc.wrapping_sub(2)
                );
            }
        }
    }

    /// Execute DD/FD followup.
    fn execute_dd_fd_followup(&mut self) {
        let op = self.opcode;
        let is_iy = self.prefix == 0xFD;

        match op {
            // POP IX/IY
            0xE1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                let value = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                if is_iy {
                    self.regs.iy = value;
                } else {
                    self.regs.ix = value;
                }
            }

            // LD IX/IY, nn
            0x21 => {
                let value = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                if is_iy {
                    self.regs.iy = value;
                } else {
                    self.regs.ix = value;
                }
            }

            // LD IXH/IYH, n (undocumented)
            0x26 => {
                if is_iy {
                    self.regs.iy = (self.regs.iy & 0x00FF) | ((self.data_lo as u16) << 8);
                } else {
                    self.regs.ix = (self.regs.ix & 0x00FF) | ((self.data_lo as u16) << 8);
                }
            }

            // LD IXL/IYL, n (undocumented)
            0x2E => {
                if is_iy {
                    self.regs.iy = (self.regs.iy & 0xFF00) | (self.data_lo as u16);
                } else {
                    self.regs.ix = (self.regs.ix & 0xFF00) | (self.data_lo as u16);
                }
            }

            // LD (nnnn), IX/IY - stage 1: queue writes
            0x22 => {
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                self.addr = addr;
                self.data_lo = idx as u8;
                self.data_hi = (idx >> 8) as u8;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD IX/IY, (nnnn) - stage 2: store data to register
            0x2A if self.followup_stage >= 2 => {
                let value = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                if is_iy {
                    self.regs.iy = value;
                } else {
                    self.regs.ix = value;
                }
            }

            // LD IX/IY, (nnnn) - stage 1: queue memory reads
            0x2A => {
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.addr = addr;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // ALU (IX+d)/(IY+d) - stage 2: perform ALU operation
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE
                if self.followup_stage >= 2 =>
            {
                let value = self.data_lo;
                match op {
                    0x86 => {
                        // ADD A, (IX+d)
                        let result = alu::add8(self.regs.a, value, false);
                        self.regs.a = result.value;
                        self.regs.f = result.flags;
                    }
                    0x8E => {
                        // ADC A, (IX+d)
                        let carry = self.regs.f & CF != 0;
                        let result = alu::add8(self.regs.a, value, carry);
                        self.regs.a = result.value;
                        self.regs.f = result.flags;
                    }
                    0x96 => {
                        // SUB (IX+d)
                        let result = alu::sub8(self.regs.a, value, false);
                        self.regs.a = result.value;
                        self.regs.f = result.flags;
                    }
                    0x9E => {
                        // SBC A, (IX+d)
                        let carry = self.regs.f & CF != 0;
                        let result = alu::sub8(self.regs.a, value, carry);
                        self.regs.a = result.value;
                        self.regs.f = result.flags;
                    }
                    0xA6 => {
                        // AND (IX+d)
                        self.regs.a &= value;
                        self.regs.f = sz53p(self.regs.a) | HF;
                    }
                    0xAE => {
                        // XOR (IX+d)
                        self.regs.a ^= value;
                        self.regs.f = sz53p(self.regs.a);
                    }
                    0xB6 => {
                        // OR (IX+d)
                        self.regs.a |= value;
                        self.regs.f = sz53p(self.regs.a);
                    }
                    0xBE => {
                        // CP (IX+d)
                        let result = alu::sub8(self.regs.a, value, false);
                        self.regs.f = (result.flags & !(YF | XF)) | (value & (YF | XF));
                    }
                    _ => unreachable!(),
                }
            }

            // INC (IX+d)/(IY+d) - stage 2: perform INC, write back
            0x34 if self.followup_stage >= 2 => {
                let result = alu::inc8(self.data_lo);
                self.data_lo = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // INC (IX+d)/(IY+d) - stage 1: calculate address, queue memory read
            0x34 => {
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(1);
                self.queue_execute_followup();
            }

            // DEC (IX+d)/(IY+d) - stage 2: perform DEC, write back
            0x35 if self.followup_stage >= 2 => {
                let result = alu::dec8(self.data_lo);
                self.data_lo = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // DEC (IX+d)/(IY+d) - stage 1: calculate address, queue memory read
            0x35 => {
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(1);
                self.queue_execute_followup();
            }

            // LD (IX+d)/(IY+d), n - calculate address and write immediate byte
            0x36 => {
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.queue_internal(2);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // LD r, (IX+d)/(IY+d) - stage 2: store to register
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E if self.followup_stage >= 2 => {
                let value = self.data_lo;
                match op {
                    0x46 => self.regs.b = value,
                    0x4E => self.regs.c = value,
                    0x56 => self.regs.d = value,
                    0x5E => self.regs.e = value,
                    0x66 => self.regs.h = value,
                    0x6E => self.regs.l = value,
                    0x7E => self.regs.a = value,
                    _ => unreachable!(),
                }
            }

            // LD r, (IX+d)/(IY+d) - stage 1: queue memory read
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LD (IX+d)/(IY+d), r - calculate address and write register
            0x70 | 0x71 | 0x72 | 0x73 | 0x74 | 0x75 | 0x77 => {
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.data_lo = match op {
                    0x70 => self.regs.b,
                    0x71 => self.regs.c,
                    0x72 => self.regs.d,
                    0x73 => self.regs.e,
                    0x74 => self.regs.h,
                    0x75 => self.regs.l,
                    0x77 => self.regs.a,
                    _ => unreachable!(),
                };
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // ALU (IX+d)/(IY+d) - stage 1: calculate address and queue memory read
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                let idx = if is_iy { self.regs.iy } else { self.regs.ix };
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            _ => {
                panic!(
                    "Unimplemented DD/FD followup: opcode={:02X} PC={:04X}",
                    op, self.regs.pc
                );
            }
        }
    }

    /// Execute ED-prefixed instruction.
    pub(super) fn execute_ed(&mut self) {
        let op = self.opcode;

        match op {
            // SBC HL, BC
            0x42 => {
                self.queue_internal(7);
                let (result, flags) = alu::sbc16(self.regs.hl(), self.regs.bc(), self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD (nn), BC
            0x43 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // ADC HL, BC
            0x4A => {
                self.queue_internal(7);
                let (result, flags) = alu::adc16(self.regs.hl(), self.regs.bc(), self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD BC, (nn)
            0x4B => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // SBC HL, DE
            0x52 => {
                self.queue_internal(7);
                let (result, flags) = alu::sbc16(self.regs.hl(), self.regs.de(), self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD (nn), DE
            0x53 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // ADC HL, DE
            0x5A => {
                self.queue_internal(7);
                let (result, flags) = alu::adc16(self.regs.hl(), self.regs.de(), self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD DE, (nn)
            0x5B => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // SBC HL, HL
            0x62 => {
                self.queue_internal(7);
                let (result, flags) = alu::sbc16(self.regs.hl(), self.regs.hl(), self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD (nn), HL (ED version)
            0x63 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // ADC HL, HL
            0x6A => {
                self.queue_internal(7);
                let (result, flags) = alu::adc16(self.regs.hl(), self.regs.hl(), self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD HL, (nn) (ED version)
            0x6B => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // SBC HL, SP
            0x72 => {
                self.queue_internal(7);
                let (result, flags) = alu::sbc16(self.regs.hl(), self.regs.sp, self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD (nn), SP
            0x73 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // ADC HL, SP
            0x7A => {
                self.queue_internal(7);
                let (result, flags) = alu::adc16(self.regs.hl(), self.regs.sp, self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD SP, (nn)
            0x7B => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LDI
            0xA0 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // CPI
            0xA1 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LDD
            0xA8 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // CPD
            0xA9 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LDIR
            0xB0 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // CPIR
            0xB1 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LDDR
            0xB8 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // CPDR
            0xB9 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // NEG - negate accumulator (0 - A)
            0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => {
                // All these opcodes are undocumented NEG variants, but behave the same
                let result = alu::sub8(0, self.regs.a, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // RRD - rotate right digit
            0x67 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // RLD - rotate left digit
            0x6F => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            _ => {
                panic!(
                    "Unimplemented ED opcode: {:02X} at PC={:04X}",
                    op,
                    self.regs.pc.wrapping_sub(2)
                );
            }
        }
    }

    /// Execute follow-up for ED-prefixed instructions.
    fn execute_ed_followup(&mut self) {
        let op = self.opcode;

        match op {
            // LD (nn), BC
            0x43 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.c;
                self.data_hi = self.regs.b;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD BC, (nn) - second stage: data loaded, store in BC
            0x4B if self.followup_stage >= 2 => {
                self.regs.c = self.data_lo;
                self.regs.b = self.data_hi;
            }

            // LD BC, (nn) - first stage: set up memory read
            0x4B => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // LD (nn), DE
            0x53 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.e;
                self.data_hi = self.regs.d;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD DE, (nn) - second stage: data loaded, store in DE
            0x5B if self.followup_stage >= 2 => {
                self.regs.e = self.data_lo;
                self.regs.d = self.data_hi;
            }

            // LD DE, (nn) - first stage: set up memory read
            0x5B => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // LD (nn), HL (ED version)
            0x63 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.l;
                self.data_hi = self.regs.h;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD HL, (nn) (ED version) - second stage: data loaded, store in HL
            0x6B if self.followup_stage >= 2 => {
                self.regs.l = self.data_lo;
                self.regs.h = self.data_hi;
            }

            // LD HL, (nn) (ED version) - first stage: set up memory read
            0x6B => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // LD (nn), SP
            0x73 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.sp as u8;
                self.data_hi = (self.regs.sp >> 8) as u8;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD SP, (nn) - second stage: data loaded, store in SP
            0x7B if self.followup_stage >= 2 => {
                self.regs.sp = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // LD SP, (nn) - first stage: set up memory read
            0x7B => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // LDI
            0xA0 => {
                let value = self.data_lo;
                self.addr = self.regs.de();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.queue_internal(2);

                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                self.regs.set_de(self.regs.de().wrapping_add(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                let n = value.wrapping_add(self.regs.a);
                self.regs.f = (self.regs.f & (SF | ZF | CF))
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };
            }

            // CPI
            0xA1 => {
                let value = self.data_lo;
                self.queue_internal(5);

                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });

                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                self.regs.f = (self.regs.f & CF)
                    | NF
                    | if result == 0 { ZF } else { 0 }
                    | if result & 0x80 != 0 { SF } else { 0 }
                    | if hf { HF } else { 0 }
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };
            }

            // LDD
            0xA8 => {
                let value = self.data_lo;
                self.addr = self.regs.de();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.queue_internal(2);

                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                self.regs.set_de(self.regs.de().wrapping_sub(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                let n = value.wrapping_add(self.regs.a);
                self.regs.f = (self.regs.f & (SF | ZF | CF))
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };
            }

            // CPD
            0xA9 => {
                let value = self.data_lo;
                self.queue_internal(5);

                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });

                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                self.regs.f = (self.regs.f & CF)
                    | NF
                    | if result == 0 { ZF } else { 0 }
                    | if result & 0x80 != 0 { SF } else { 0 }
                    | if hf { HF } else { 0 }
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };
            }

            // LDIR
            0xB0 => {
                let value = self.data_lo;
                self.addr = self.regs.de();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.queue_internal(2);

                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                self.regs.set_de(self.regs.de().wrapping_add(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                let n = value.wrapping_add(self.regs.a);
                self.regs.f = (self.regs.f & (SF | ZF | CF))
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 };

                if self.regs.bc() != 0 {
                    self.regs.f |= PF;
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                }
            }

            // CPIR
            0xB1 => {
                let value = self.data_lo;
                self.queue_internal(5);

                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });

                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                self.regs.f = (self.regs.f & CF)
                    | NF
                    | if result == 0 { ZF } else { 0 }
                    | if result & 0x80 != 0 { SF } else { 0 }
                    | if hf { HF } else { 0 }
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };

                if self.regs.bc() != 0 && result != 0 {
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                }
            }

            // LDDR
            0xB8 => {
                let value = self.data_lo;
                self.addr = self.regs.de();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.queue_internal(2);

                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                self.regs.set_de(self.regs.de().wrapping_sub(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                let n = value.wrapping_add(self.regs.a);
                self.regs.f = (self.regs.f & (SF | ZF | CF))
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 };

                if self.regs.bc() != 0 {
                    self.regs.f |= PF;
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                }
            }

            // CPDR
            0xB9 => {
                let value = self.data_lo;
                self.queue_internal(5);

                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });

                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));

                self.regs.f = (self.regs.f & CF)
                    | NF
                    | if result == 0 { ZF } else { 0 }
                    | if result & 0x80 != 0 { SF } else { 0 }
                    | if hf { HF } else { 0 }
                    | (n & XF)
                    | if n & 0x02 != 0 { YF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };

                if self.regs.bc() != 0 && result != 0 {
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                }
            }

            // RRD - rotate right digit
            0x67 => {
                let mem = self.data_lo;
                self.queue_internal(4);

                // Low nibble of (HL) -> low nibble of A
                // Low nibble of A -> high nibble of (HL)
                // High nibble of (HL) -> low nibble of (HL)
                let new_a = (self.regs.a & 0xF0) | (mem & 0x0F);
                let new_mem = ((self.regs.a & 0x0F) << 4) | ((mem >> 4) & 0x0F);

                self.regs.a = new_a;
                self.data_lo = new_mem;
                self.micro_ops.push(MicroOp::WriteMem);

                self.regs.f = sz53p(self.regs.a) | (self.regs.f & CF);
            }

            // RLD - rotate left digit
            0x6F => {
                let mem = self.data_lo;
                self.queue_internal(4);

                // High nibble of (HL) -> low nibble of A
                // Low nibble of A -> low nibble of (HL)
                // Low nibble of (HL) -> high nibble of (HL)
                let new_a = (self.regs.a & 0xF0) | ((mem >> 4) & 0x0F);
                let new_mem = ((mem & 0x0F) << 4) | (self.regs.a & 0x0F);

                self.regs.a = new_a;
                self.data_lo = new_mem;
                self.micro_ops.push(MicroOp::WriteMem);

                self.regs.f = sz53p(self.regs.a) | (self.regs.f & CF);
            }

            _ => {
                panic!(
                    "Unimplemented ED followup: opcode={:02X} PC={:04X}",
                    op, self.regs.pc
                );
            }
        }
    }

    /// Execute DDCB or FDCB-prefixed instruction.
    /// By this point: prefix=DD/FD, prefix2=CB, displacement and opcode are set.
    pub(super) fn execute_ddcb_fdcb(&mut self) {
        let is_iy = self.prefix == 0xFD;
        let idx = if is_iy { self.regs.iy } else { self.regs.ix };
        self.addr = idx.wrapping_add(self.displacement as i16 as u16);

        // Queue memory read, internal processing, then followup
        self.micro_ops.push(MicroOp::ReadMem);
        self.queue_internal(2);
        self.queue_execute_followup();
    }

    /// Execute DDCB/FDCB followup after memory read.
    fn execute_ddcb_fdcb_followup(&mut self) {
        let op = self.opcode;
        let is_iy = self.prefix == 0xFD;
        let value = self.data_lo;
        let r = op & 7; // Register to optionally copy result to

        // Determine operation type from opcode
        let result = match op {
            // Rotates: 0x00-0x3F
            0x00..=0x07 => {
                // RLC (IX+d)
                let res = alu::rlc8(value);
                self.regs.f = res.flags;
                res.value
            }
            0x08..=0x0F => {
                // RRC (IX+d)
                let res = alu::rrc8(value);
                self.regs.f = res.flags;
                res.value
            }
            0x10..=0x17 => {
                // RL (IX+d)
                let res = alu::rl8(value, self.regs.f & CF != 0);
                self.regs.f = res.flags;
                res.value
            }
            0x18..=0x1F => {
                // RR (IX+d)
                let res = alu::rr8(value, self.regs.f & CF != 0);
                self.regs.f = res.flags;
                res.value
            }
            0x20..=0x27 => {
                // SLA (IX+d)
                let res = alu::sla8(value);
                self.regs.f = res.flags;
                res.value
            }
            0x28..=0x2F => {
                // SRA (IX+d)
                let res = alu::sra8(value);
                self.regs.f = res.flags;
                res.value
            }
            0x30..=0x37 => {
                // SLL (IX+d) - undocumented
                let res = alu::sll8(value);
                self.regs.f = res.flags;
                res.value
            }
            0x38..=0x3F => {
                // SRL (IX+d)
                let res = alu::srl8(value);
                self.regs.f = res.flags;
                res.value
            }

            // BIT: 0x40-0x7F - no write back
            0x40..=0x7F => {
                let bit = (op >> 3) & 7;
                let mask = 1 << bit;
                let is_zero = value & mask == 0;

                let mut flags = self.regs.f & CF; // Preserve carry
                flags |= HF; // H is set
                if is_zero {
                    flags |= ZF | PF; // Z and P/V are set if bit is 0
                }
                if bit == 7 && !is_zero {
                    flags |= SF; // S is set if bit 7 is tested and is 1
                }
                // Undocumented: X and Y flags from high byte of address
                flags |= ((self.addr >> 8) as u8) & (XF | YF);
                self.regs.f = flags;
                return; // BIT doesn't write back
            }

            // RES: 0x80-0xBF
            0x80..=0xBF => {
                let bit = (op >> 3) & 7;
                let mask = !(1 << bit);
                value & mask
            }

            // SET: 0xC0-0xFF
            0xC0..=0xFF => {
                let bit = (op >> 3) & 7;
                let mask = 1 << bit;
                value | mask
            }
        };

        // Write result back to memory
        self.data_lo = result;
        self.micro_ops.push(MicroOp::WriteMem);

        // Undocumented: if r != 6, also copy result to register
        if r != 6 {
            self.set_reg8(r, result);
        }
    }
}
