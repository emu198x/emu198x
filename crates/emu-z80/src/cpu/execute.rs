//! Instruction execution for the Z80.

#![allow(clippy::too_many_lines)] // Instruction decode is inherently large.
#![allow(clippy::match_same_arms)] // Some opcodes intentionally share bodies.
#![allow(clippy::cast_possible_wrap)] // Intentional i8 casts for displacements.
#![allow(clippy::cast_possible_truncation)] // Intentional truncation for low byte.
#![allow(clippy::cast_sign_loss)] // Relative jumps add signed offset to unsigned PC.
#![allow(clippy::cast_lossless)] // Using as casts for clarity in CPU emulation.

use crate::alu;
use crate::flags::{CF, HF, NF, PF, SF, XF, YF, ZF};
use crate::microcode::MicroOp;

use super::Z80;

impl Z80 {
    /// Execute unprefixed instruction.
    pub(super) fn execute_unprefixed(&mut self) {
        let op = self.opcode;

        match op {
            // === 0x00-0x0F ===
            0x00 => {} // NOP

            0x01 => {
                // LD BC, nn (10 T-states: 4+3+3)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x02 => {
                // LD (BC), A (7 T-states: 4+3)
                self.addr = self.regs.bc();
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            0x03 => {
                // INC BC (6 T-states: 4+2 internal)
                self.queue_internal(2);
                self.regs.set_bc(self.regs.bc().wrapping_add(1));
            }

            0x04 => {
                // INC B (4 T-states)
                let result = alu::inc8(self.regs.b);
                self.regs.b = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x05 => {
                // DEC B (4 T-states)
                let result = alu::dec8(self.regs.b);
                self.regs.b = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x06 => {
                // LD B, n (7 T-states: 4+3)
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x07 => {
                // RLCA (4 T-states)
                let carry = self.regs.a >> 7;
                self.regs.a = (self.regs.a << 1) | carry;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if carry != 0 { CF } else { 0 };
            }

            0x08 => {
                // EX AF, AF' (4 T-states)
                core::mem::swap(&mut self.regs.a, &mut self.regs.a_alt);
                core::mem::swap(&mut self.regs.f, &mut self.regs.f_alt);
            }

            0x09 => {
                // ADD HL, BC (11 T-states: 4+4+3 internal)
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.bc());
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            0x0A => {
                // LD A, (BC) (7 T-states: 4+3)
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            0x0B => {
                // DEC BC (6 T-states: 4+2 internal)
                self.queue_internal(2);
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));
            }

            0x0C => {
                // INC C
                let result = alu::inc8(self.regs.c);
                self.regs.c = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x0D => {
                // DEC C
                let result = alu::dec8(self.regs.c);
                self.regs.c = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x0E => {
                // LD C, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x0F => {
                // RRCA
                let carry = self.regs.a & 1;
                self.regs.a = (self.regs.a >> 1) | (carry << 7);
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if carry != 0 { CF } else { 0 };
            }

            // === 0x10-0x1F ===
            0x10 => {
                // DJNZ e (13/8 T-states)
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x11 => {
                // LD DE, nn
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x12 => {
                // LD (DE), A
                self.addr = self.regs.de();
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            0x13 => {
                // INC DE
                self.queue_internal(2);
                self.regs.set_de(self.regs.de().wrapping_add(1));
            }

            0x14 => {
                // INC D
                let result = alu::inc8(self.regs.d);
                self.regs.d = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x15 => {
                // DEC D
                let result = alu::dec8(self.regs.d);
                self.regs.d = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x16 => {
                // LD D, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x17 => {
                // RLA
                let carry = self.regs.a >> 7;
                let old_carry = u8::from(self.regs.f & CF != 0);
                self.regs.a = (self.regs.a << 1) | old_carry;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if carry != 0 { CF } else { 0 };
            }

            0x18 => {
                // JR e (12 T-states)
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x19 => {
                // ADD HL, DE
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.de());
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            0x1A => {
                // LD A, (DE)
                self.addr = self.regs.de();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            0x1B => {
                // DEC DE
                self.queue_internal(2);
                self.regs.set_de(self.regs.de().wrapping_sub(1));
            }

            0x1C => {
                // INC E
                let result = alu::inc8(self.regs.e);
                self.regs.e = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x1D => {
                // DEC E
                let result = alu::dec8(self.regs.e);
                self.regs.e = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x1E => {
                // LD E, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x1F => {
                // RRA
                let carry = self.regs.a & 1;
                let old_carry = if self.regs.f & CF != 0 { 0x80 } else { 0 };
                self.regs.a = (self.regs.a >> 1) | old_carry;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if carry != 0 { CF } else { 0 };
            }

            // === 0x20-0x2F ===
            0x20 => {
                // JR NZ, e
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x21 => {
                // LD HL, nn
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x22 => {
                // LD (nn), HL (16 T-states)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x23 => {
                // INC HL
                self.queue_internal(2);
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
            }

            0x24 => {
                // INC H
                let result = alu::inc8(self.regs.h);
                self.regs.h = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x25 => {
                // DEC H
                let result = alu::dec8(self.regs.h);
                self.regs.h = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x26 => {
                // LD H, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x27 => {
                // DAA
                self.execute_daa();
            }

            0x28 => {
                // JR Z, e
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x29 => {
                // ADD HL, HL
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.hl());
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            0x2A => {
                // LD HL, (nn)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x2B => {
                // DEC HL
                self.queue_internal(2);
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
            }

            0x2C => {
                // INC L
                let result = alu::inc8(self.regs.l);
                self.regs.l = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x2D => {
                // DEC L
                let result = alu::dec8(self.regs.l);
                self.regs.l = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x2E => {
                // LD L, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x2F => {
                // CPL
                self.regs.a = !self.regs.a;
                self.regs.f = (self.regs.f & (SF | ZF | PF | CF))
                    | (self.regs.a & (YF | XF))
                    | HF
                    | NF;
            }

            // === 0x30-0x3F ===
            0x30 => {
                // JR NC, e
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x31 => {
                // LD SP, nn
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x32 => {
                // LD (nn), A
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x33 => {
                // INC SP
                self.queue_internal(2);
                self.regs.sp = self.regs.sp.wrapping_add(1);
            }

            0x34 => {
                // INC (HL) (11 T-states: 4+3+1+3)
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(1);
                self.queue_execute_followup();
            }

            0x35 => {
                // DEC (HL)
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(1);
                self.queue_execute_followup();
            }

            0x36 => {
                // LD (HL), n (10 T-states: 4+3+3)
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x37 => {
                // SCF
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | CF;
            }

            0x38 => {
                // JR C, e
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x39 => {
                // ADD HL, SP
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.regs.hl(), self.regs.sp);
                self.regs.set_hl(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }

            0x3A => {
                // LD A, (nn)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0x3B => {
                // DEC SP
                self.queue_internal(2);
                self.regs.sp = self.regs.sp.wrapping_sub(1);
            }

            0x3C => {
                // INC A
                let result = alu::inc8(self.regs.a);
                self.regs.a = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x3D => {
                // DEC A
                let result = alu::dec8(self.regs.a);
                self.regs.a = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
            }

            0x3E => {
                // LD A, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0x3F => {
                // CCF
                let old_carry = self.regs.f & CF;
                self.regs.f = (self.regs.f & (SF | ZF | PF))
                    | (self.regs.a & (YF | XF))
                    | if old_carry != 0 { HF } else { 0 }
                    | if old_carry == 0 { CF } else { 0 };
            }

            // === 0x40-0x7F: LD r, r' and HALT ===
            0x40..=0x75 | 0x77..=0x7F => {
                let dst = (op >> 3) & 7;
                let src = op & 7;

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
                    // LD r, r'
                    let value = self.get_reg8(src);
                    self.set_reg8(dst, value);
                }
            }

            0x76 => {
                // HALT
                self.regs.halted = true;
            }

            // === 0x80-0xBF: ALU operations ===
            0x80..=0xBF => {
                let alu_op = (op >> 3) & 7;
                let src = op & 7;

                if src == 6 {
                    // ALU A, (HL)
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(src);
                    self.execute_alu(alu_op, value);
                }
            }

            // === 0xC0-0xFF: Misc, jumps, calls, returns ===
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
                // RET cc (11/5 T-states)
                let cc = (op >> 3) & 7;
                self.queue_internal(1);
                if self.condition(cc) {
                    self.micro_ops.push(MicroOp::ReadMem16Lo);
                    self.micro_ops.push(MicroOp::ReadMem16Hi);
                    self.queue_execute_followup();
                    self.addr = self.regs.sp;
                }
            }

            0xC1 => {
                // POP BC
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => {
                // JP cc, nn (10 T-states)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0xC3 => {
                // JP nn
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => {
                // CALL cc, nn (17/10 T-states)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0xC5 => {
                // PUSH BC (11 T-states: 4+1+3+3)
                self.queue_internal(1);
                let val = self.regs.bc();
                self.data_hi = (val >> 8) as u8;
                self.data_lo = val as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            0xC6 => {
                // ADD A, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                // RST p (11 T-states)
                let p = op & 0x38;
                self.queue_internal(1);
                self.data_hi = (self.regs.pc >> 8) as u8;
                self.data_lo = self.regs.pc as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
                self.regs.pc = u16::from(p);
            }

            0xC9 => {
                // RET (10 T-states)
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            0xCD => {
                // CALL nn (17 T-states)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            0xCE => {
                // ADC A, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xD1 => {
                // POP DE
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            0xD3 => {
                // OUT (n), A
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xD5 => {
                // PUSH DE
                self.queue_internal(1);
                let val = self.regs.de();
                self.data_hi = (val >> 8) as u8;
                self.data_lo = val as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            0xD6 => {
                // SUB n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xD9 => {
                // EXX
                core::mem::swap(&mut self.regs.b, &mut self.regs.b_alt);
                core::mem::swap(&mut self.regs.c, &mut self.regs.c_alt);
                core::mem::swap(&mut self.regs.d, &mut self.regs.d_alt);
                core::mem::swap(&mut self.regs.e, &mut self.regs.e_alt);
                core::mem::swap(&mut self.regs.h, &mut self.regs.h_alt);
                core::mem::swap(&mut self.regs.l, &mut self.regs.l_alt);
            }

            0xDB => {
                // IN A, (n)
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xDE => {
                // SBC A, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xE1 => {
                // POP HL
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            0xE3 => {
                // EX (SP), HL (19 T-states)
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_internal(1);
                self.queue_execute_followup();
            }

            0xE5 => {
                // PUSH HL
                self.queue_internal(1);
                let val = self.regs.hl();
                self.data_hi = (val >> 8) as u8;
                self.data_lo = val as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            0xE6 => {
                // AND n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xE9 => {
                // JP (HL)
                self.regs.pc = self.regs.hl();
            }

            0xEB => {
                // EX DE, HL
                let de = self.regs.de();
                let hl = self.regs.hl();
                self.regs.set_de(hl);
                self.regs.set_hl(de);
            }

            0xEE => {
                // XOR n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xF1 => {
                // POP AF
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            0xF3 => {
                // DI
                self.regs.iff1 = false;
                self.regs.iff2 = false;
            }

            0xF5 => {
                // PUSH AF
                self.queue_internal(1);
                let val = self.regs.af();
                self.data_hi = (val >> 8) as u8;
                self.data_lo = val as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            0xF6 => {
                // OR n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            0xF9 => {
                // LD SP, HL (6 T-states)
                self.queue_internal(2);
                self.regs.sp = self.regs.hl();
            }

            0xFB => {
                // EI
                self.regs.iff1 = true;
                self.regs.iff2 = true;
            }

            0xFE => {
                // CP n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            _ => {
                // Unimplemented - treat as NOP
            }
        }
    }

    /// Execute follow-up for instructions that need immediate/memory data.
    pub(super) fn execute_followup(&mut self) {
        let op = self.opcode;

        match op {
            // LD instructions using immediate data
            0x01 => self.regs.set_bc(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8)),
            0x06 => self.regs.b = self.data_lo,
            0x0A => self.regs.a = self.data_lo,
            0x0E => self.regs.c = self.data_lo,
            0x10 => {
                // DJNZ
                self.regs.b = self.regs.b.wrapping_sub(1);
                if self.regs.b != 0 {
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_add(self.data_lo as i8 as u16);
                }
            }
            0x11 => self.regs.set_de(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8)),
            0x16 => self.regs.d = self.data_lo,
            0x18 => {
                // JR e
                self.queue_internal(5);
                self.regs.pc = self.regs.pc.wrapping_add(self.data_lo as i8 as u16);
            }
            0x1E => self.regs.e = self.data_lo,
            0x20 => {
                // JR NZ
                if self.regs.f & ZF == 0 {
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_add(self.data_lo as i8 as u16);
                }
            }
            0x21 => self.regs.set_hl(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8)),
            0x22 => {
                // LD (nn), HL
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.l;
                self.data_hi = self.regs.h;
                self.micro_ops.push(MicroOp::WriteMem);
                self.addr = self.addr.wrapping_add(1);
                let tmp = self.data_hi;
                self.data_lo = tmp;
                self.micro_ops.push(MicroOp::WriteMem);
            }
            0x26 => self.regs.h = self.data_lo,
            0x28 => {
                // JR Z
                if self.regs.f & ZF != 0 {
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_add(self.data_lo as i8 as u16);
                }
            }
            0x2A => {
                // LD HL, (nn)
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }
            0x2E => self.regs.l = self.data_lo,
            0x30 => {
                // JR NC
                if self.regs.f & CF == 0 {
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_add(self.data_lo as i8 as u16);
                }
            }
            0x31 => self.regs.sp = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8),
            0x32 => {
                // LD (nn), A
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::WriteMem);
            }
            0x34 => {
                // INC (HL) - second execute
                let result = alu::inc8(self.data_lo);
                self.data_lo = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
                self.micro_ops.push(MicroOp::WriteMem);
            }
            0x35 => {
                // DEC (HL) - second execute
                let result = alu::dec8(self.data_lo);
                self.data_lo = result.value;
                self.regs.f = (self.regs.f & CF) | result.flags;
                self.micro_ops.push(MicroOp::WriteMem);
            }
            0x36 => {
                // LD (HL), n
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::WriteMem);
            }
            0x38 => {
                // JR C
                if self.regs.f & CF != 0 {
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_add(self.data_lo as i8 as u16);
                }
            }
            0x3A => {
                // LD A, (nn) - first stage
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }
            0x3E => self.regs.a = self.data_lo,

            // LD r, (HL)
            0x46 => self.regs.b = self.data_lo,
            0x4E => self.regs.c = self.data_lo,
            0x56 => self.regs.d = self.data_lo,
            0x5E => self.regs.e = self.data_lo,
            0x66 => self.regs.h = self.data_lo,
            0x6E => self.regs.l = self.data_lo,
            0x7E => self.regs.a = self.data_lo,

            // ALU A, (HL) and ALU A, n
            0x86 | 0xC6 => {
                let result = alu::add8(self.regs.a, self.data_lo, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0x8E | 0xCE => {
                let result = alu::add8(self.regs.a, self.data_lo, self.regs.f & CF != 0);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0x96 | 0xD6 => {
                let result = alu::sub8(self.regs.a, self.data_lo, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0x9E | 0xDE => {
                let result = alu::sub8(self.regs.a, self.data_lo, self.regs.f & CF != 0);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0xA6 | 0xE6 => {
                let result = alu::and8(self.regs.a, self.data_lo);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0xAE | 0xEE => {
                let result = alu::xor8(self.regs.a, self.data_lo);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0xB6 | 0xF6 => {
                let result = alu::or8(self.regs.a, self.data_lo);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            0xBE | 0xFE => {
                let result = alu::cp8(self.regs.a, self.data_lo);
                self.regs.f = result.flags;
            }

            // RET cc - followup after reading return address
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // POP rr
            0xC1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.set_bc(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8));
            }
            0xD1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.set_de(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8));
            }
            0xE1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.set_hl(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8));
            }
            0xF1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.set_af(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8));
            }

            // JP cc, nn and JP nn
            0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => {
                let cc = (op >> 3) & 7;
                if self.condition(cc) {
                    self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                }
            }
            0xC3 => {
                self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // CALL cc, nn and CALL nn
            0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => {
                let cc = (op >> 3) & 7;
                if self.condition(cc) {
                    self.queue_internal(1);
                    self.data_hi = (self.regs.pc >> 8) as u8;
                    let tmp = self.regs.pc as u8;
                    let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                    self.data_hi = (self.regs.pc >> 8) as u8;
                    self.data_lo = tmp;
                    self.micro_ops.push(MicroOp::WriteMemHiFirst);
                    self.micro_ops.push(MicroOp::WriteMemLoSecond);
                    self.regs.pc = target;
                }
            }
            0xCD => {
                self.queue_internal(1);
                let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.data_hi = (self.regs.pc >> 8) as u8;
                self.data_lo = self.regs.pc as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
                self.regs.pc = target;
            }

            // RET
            0xC9 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            }

            // OUT (n), A
            0xD3 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.regs.a) << 8);
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::IoWrite);
            }

            // IN A, (n)
            0xDB => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.regs.a) << 8);
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // EX (SP), HL
            0xE3 => {
                let old_hl = self.regs.hl();
                self.regs.set_hl(u16::from(self.data_lo) | (u16::from(self.data_hi) << 8));
                self.data_lo = old_hl as u8;
                self.data_hi = (old_hl >> 8) as u8;
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::WriteMem);
                self.addr = self.regs.sp.wrapping_add(1);
                let hi = self.data_hi;
                self.data_lo = hi;
                self.micro_ops.push(MicroOp::WriteMem);
                self.queue_internal(2);
            }

            _ => {}
        }
    }

    /// Execute ALU operation.
    fn execute_alu(&mut self, alu_op: u8, value: u8) {
        match alu_op {
            0 => {
                // ADD
                let result = alu::add8(self.regs.a, value, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            1 => {
                // ADC
                let result = alu::add8(self.regs.a, value, self.regs.f & CF != 0);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            2 => {
                // SUB
                let result = alu::sub8(self.regs.a, value, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            3 => {
                // SBC
                let result = alu::sub8(self.regs.a, value, self.regs.f & CF != 0);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            4 => {
                // AND
                let result = alu::and8(self.regs.a, value);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            5 => {
                // XOR
                let result = alu::xor8(self.regs.a, value);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            6 => {
                // OR
                let result = alu::or8(self.regs.a, value);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }
            7 => {
                // CP
                let result = alu::cp8(self.regs.a, value);
                self.regs.f = result.flags;
            }
            _ => unreachable!(),
        }
    }

    /// DAA instruction - decimal adjust accumulator.
    fn execute_daa(&mut self) {
        let a = self.regs.a;
        let flags = self.regs.f;
        let n_flag = flags & NF != 0;
        let c_flag = flags & CF != 0;
        let h_flag = flags & HF != 0;

        let mut correction = 0u8;
        let mut carry = false;

        // Determine correction based on current value and flags
        if h_flag || (!n_flag && (a & 0x0F) > 9) {
            correction |= 0x06;
        }
        if c_flag || (!n_flag && a > 0x99) {
            correction |= 0x60;
            carry = true;
        }

        // Apply correction
        let result = if n_flag {
            a.wrapping_sub(correction)
        } else {
            a.wrapping_add(correction)
        };

        // Build flags
        let mut new_flags = flags & NF; // Preserve N flag

        if result == 0 {
            new_flags |= ZF;
        }
        if result & 0x80 != 0 {
            new_flags |= SF;
        }
        new_flags |= result & (YF | XF);
        if result.count_ones().is_multiple_of(2) {
            new_flags |= PF;
        }
        if carry {
            new_flags |= CF;
        }

        // HF: set if lower nibble adjustment caused borrow/carry
        if n_flag {
            // After subtraction: HF = old_HF AND (lower nibble of result < 6)
            if h_flag && (result & 0x0F) < 6 {
                new_flags |= HF;
            }
        } else {
            // After addition: HF = lower nibble of original > 9
            if (a & 0x0F) > 9 {
                new_flags |= HF;
            }
        }

        self.regs.a = result;
        self.regs.f = new_flags;
    }

    /// Execute CB-prefixed instruction.
    pub(super) fn execute_cb(&mut self) {
        let op = self.opcode;
        let r = op & 7;

        if r == 6 {
            // (HL) operations need memory access
            self.addr = self.regs.hl();
            self.micro_ops.push(MicroOp::ReadMem);
            self.queue_internal(1);
            self.queue_execute_followup();
            return;
        }

        let value = self.get_reg8(r);
        let result = self.execute_cb_op(op, value);
        if let Some(res) = result {
            self.set_reg8(r, res);
        }
    }

    /// Execute CB operation on value.
    fn execute_cb_op(&mut self, op: u8, value: u8) -> Option<u8> {
        let bit = (op >> 3) & 7;

        match op >> 6 {
            0 => {
                // Rotates/shifts
                let result = match bit {
                    0 => alu::rlc8(value),
                    1 => alu::rrc8(value),
                    2 => alu::rl8(value, self.regs.f & CF != 0),
                    3 => alu::rr8(value, self.regs.f & CF != 0),
                    4 => alu::sla8(value),
                    5 => alu::sra8(value),
                    6 => alu::sll8(value), // Undocumented
                    7 => alu::srl8(value),
                    _ => unreachable!(),
                };
                self.regs.f = result.flags;
                Some(result.value)
            }
            1 => {
                // BIT
                let mask = 1 << bit;
                let result = value & mask;
                let mut flags = (self.regs.f & CF) | HF;
                if result == 0 {
                    flags |= ZF | PF;
                }
                if bit == 7 && result != 0 {
                    flags |= SF;
                }
                flags |= value & (YF | XF);
                self.regs.f = flags;
                None
            }
            2 => {
                // RES
                Some(value & !(1 << bit))
            }
            3 => {
                // SET
                Some(value | (1 << bit))
            }
            _ => unreachable!(),
        }
    }

    /// Execute DD/FD-prefixed instruction.
    pub(super) fn execute_dd_fd(&mut self) {
        // Most DD/FD instructions are just HL -> IX/IY substitution
        // For now, delegate to unprefixed with index register awareness
        let op = self.opcode;

        match op {
            // ADD IX/IY, rr
            0x09 => {
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.get_index_reg(), self.regs.bc());
                self.set_index_reg(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }
            0x19 => {
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.get_index_reg(), self.regs.de());
                self.set_index_reg(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }
            0x21 => {
                // LD IX/IY, nn
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }
            0x22 => {
                // LD (nn), IX/IY
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }
            0x23 => {
                // INC IX/IY
                self.queue_internal(2);
                self.set_index_reg(self.get_index_reg().wrapping_add(1));
            }
            0x29 => {
                self.queue_internal(7);
                let idx = self.get_index_reg();
                let (result, flags) = alu::add16(idx, idx);
                self.set_index_reg(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }
            0x2A => {
                // LD IX/IY, (nn)
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }
            0x2B => {
                // DEC IX/IY
                self.queue_internal(2);
                self.set_index_reg(self.get_index_reg().wrapping_sub(1));
            }
            0x39 => {
                self.queue_internal(7);
                let (result, flags) = alu::add16(self.get_index_reg(), self.regs.sp);
                self.set_index_reg(result);
                self.regs.f = (self.regs.f & (SF | ZF | PF)) | flags;
            }
            0xE1 => {
                // POP IX/IY
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }
            0xE3 => {
                // EX (SP), IX/IY
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_internal(1);
                self.queue_execute_followup();
            }
            0xE5 => {
                // PUSH IX/IY
                self.queue_internal(1);
                let val = self.get_index_reg();
                self.data_hi = (val >> 8) as u8;
                self.data_lo = val as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }
            0xE9 => {
                // JP (IX/IY)
                self.regs.pc = self.get_index_reg();
            }
            0xF9 => {
                // LD SP, IX/IY
                self.queue_internal(2);
                self.regs.sp = self.get_index_reg();
            }

            // Undocumented IXH/IXL/IYH/IYL operations
            0x24 => {
                // INC IXH/IYH
                let val = (self.get_index_reg() >> 8) as u8;
                let result = alu::inc8(val);
                self.set_reg8_indexed(4, result.value);
                self.regs.f = (self.regs.f & CF) | result.flags;
            }
            0x25 => {
                // DEC IXH/IYH
                let val = (self.get_index_reg() >> 8) as u8;
                let result = alu::dec8(val);
                self.set_reg8_indexed(4, result.value);
                self.regs.f = (self.regs.f & CF) | result.flags;
            }
            0x26 => {
                // LD IXH/IYH, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }
            0x2C => {
                // INC IXL/IYL
                let val = self.get_index_reg() as u8;
                let result = alu::inc8(val);
                self.set_reg8_indexed(5, result.value);
                self.regs.f = (self.regs.f & CF) | result.flags;
            }
            0x2D => {
                // DEC IXL/IYL
                let val = self.get_index_reg() as u8;
                let result = alu::dec8(val);
                self.set_reg8_indexed(5, result.value);
                self.regs.f = (self.regs.f & CF) | result.flags;
            }
            0x2E => {
                // LD IXL/IYL, n
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD r, IXH/IXL/IYH/IYL (undocumented)
            0x44 => self.regs.b = self.get_reg8_indexed(4), // LD B, IXH
            0x45 => self.regs.b = self.get_reg8_indexed(5), // LD B, IXL
            0x4C => self.regs.c = self.get_reg8_indexed(4), // LD C, IXH
            0x4D => self.regs.c = self.get_reg8_indexed(5), // LD C, IXL
            0x54 => self.regs.d = self.get_reg8_indexed(4), // LD D, IXH
            0x55 => self.regs.d = self.get_reg8_indexed(5), // LD D, IXL
            0x5C => self.regs.e = self.get_reg8_indexed(4), // LD E, IXH
            0x5D => self.regs.e = self.get_reg8_indexed(5), // LD E, IXL
            0x60 => self.set_reg8_indexed(4, self.regs.b),  // LD IXH, B
            0x61 => self.set_reg8_indexed(4, self.regs.c),  // LD IXH, C
            0x62 => self.set_reg8_indexed(4, self.regs.d),  // LD IXH, D
            0x63 => self.set_reg8_indexed(4, self.regs.e),  // LD IXH, E
            0x64 => {} // LD IXH, IXH - no-op
            0x65 => {
                // LD IXH, IXL
                let val = self.get_reg8_indexed(5);
                self.set_reg8_indexed(4, val);
            }
            0x67 => self.set_reg8_indexed(4, self.regs.a), // LD IXH, A
            0x68 => self.set_reg8_indexed(5, self.regs.b), // LD IXL, B
            0x69 => self.set_reg8_indexed(5, self.regs.c), // LD IXL, C
            0x6A => self.set_reg8_indexed(5, self.regs.d), // LD IXL, D
            0x6B => self.set_reg8_indexed(5, self.regs.e), // LD IXL, E
            0x6C => {
                // LD IXL, IXH
                let val = self.get_reg8_indexed(4);
                self.set_reg8_indexed(5, val);
            }
            0x6D => {} // LD IXL, IXL - no-op
            0x6F => self.set_reg8_indexed(5, self.regs.a), // LD IXL, A
            0x7C => self.regs.a = self.get_reg8_indexed(4), // LD A, IXH
            0x7D => self.regs.a = self.get_reg8_indexed(5), // LD A, IXL

            // ALU A, IXH/IXL/IYH/IYL (undocumented)
            0x84 => self.execute_alu(0, self.get_reg8_indexed(4)), // ADD A, IXH
            0x85 => self.execute_alu(0, self.get_reg8_indexed(5)), // ADD A, IXL
            0x8C => self.execute_alu(1, self.get_reg8_indexed(4)), // ADC A, IXH
            0x8D => self.execute_alu(1, self.get_reg8_indexed(5)), // ADC A, IXL
            0x94 => self.execute_alu(2, self.get_reg8_indexed(4)), // SUB IXH
            0x95 => self.execute_alu(2, self.get_reg8_indexed(5)), // SUB IXL
            0x9C => self.execute_alu(3, self.get_reg8_indexed(4)), // SBC A, IXH
            0x9D => self.execute_alu(3, self.get_reg8_indexed(5)), // SBC A, IXL
            0xA4 => self.execute_alu(4, self.get_reg8_indexed(4)), // AND IXH
            0xA5 => self.execute_alu(4, self.get_reg8_indexed(5)), // AND IXL
            0xAC => self.execute_alu(5, self.get_reg8_indexed(4)), // XOR IXH
            0xAD => self.execute_alu(5, self.get_reg8_indexed(5)), // XOR IXL
            0xB4 => self.execute_alu(6, self.get_reg8_indexed(4)), // OR IXH
            0xB5 => self.execute_alu(6, self.get_reg8_indexed(5)), // OR IXL
            0xBC => self.execute_alu(7, self.get_reg8_indexed(4)), // CP IXH
            0xBD => self.execute_alu(7, self.get_reg8_indexed(5)), // CP IXL

            // Instructions with displacement (IX+d)/(IY+d)
            0x34 | 0x35 | 0x36 | 0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x70..=0x77 | 0x7E
            | 0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                // Need to fetch displacement
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            _ => {
                // Unknown DD/FD prefixed - execute as if unprefixed (prefix ignored)
                self.prefix = 0;
                self.execute_unprefixed();
            }
        }
    }

    /// Execute ED-prefixed instruction.
    pub(super) fn execute_ed(&mut self) {
        let op = self.opcode;

        match op {
            // IN r, (C)
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // OUT (C), r
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x71 | 0x79 => {
                let r = (op >> 3) & 7;
                self.addr = self.regs.bc();
                self.data_lo = if r == 6 { 0 } else { self.get_reg8(r) };
                self.micro_ops.push(MicroOp::IoWrite);
            }

            // SBC HL, rr
            0x42 | 0x52 | 0x62 | 0x72 => {
                self.queue_internal(7);
                let rp = (op >> 4) & 3;
                let operand = self.get_reg16(rp);
                let (result, flags) = alu::sbc16(self.regs.hl(), operand, self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD (nn), rr
            0x43 | 0x53 | 0x63 | 0x73 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // NEG
            0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => {
                let result = alu::sub8(0, self.regs.a, false);
                self.regs.a = result.value;
                self.regs.f = result.flags;
            }

            // RETN
            0x45 | 0x55 | 0x5D | 0x65 | 0x6D | 0x75 | 0x7D => {
                self.regs.iff1 = self.regs.iff2;
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // IM 0/1/2
            0x46 | 0x4E | 0x66 | 0x6E => self.regs.im = 0,
            0x56 | 0x76 => self.regs.im = 1,
            0x5E | 0x7E => self.regs.im = 2,

            // LD I, A
            0x47 => {
                self.queue_internal(1);
                self.regs.i = self.regs.a;
            }

            // ADC HL, rr
            0x4A | 0x5A | 0x6A | 0x7A => {
                self.queue_internal(7);
                let rp = (op >> 4) & 3;
                let operand = self.get_reg16(rp);
                let (result, flags) = alu::adc16(self.regs.hl(), operand, self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.regs.f = flags;
            }

            // LD rr, (nn)
            0x4B | 0x5B | 0x6B | 0x7B => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // RETI
            0x4D => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // LD R, A
            0x4F => {
                self.queue_internal(1);
                self.regs.r = self.regs.a;
            }

            // LD A, I
            0x57 => {
                self.queue_internal(1);
                self.regs.a = self.regs.i;
                self.regs.f = (self.regs.f & CF)
                    | if self.regs.a == 0 { ZF } else { 0 }
                    | if self.regs.a & 0x80 != 0 { SF } else { 0 }
                    | (self.regs.a & (YF | XF))
                    | if self.regs.iff2 { PF } else { 0 };
            }

            // LD A, R
            0x5F => {
                self.queue_internal(1);
                self.regs.a = self.regs.r;
                self.regs.f = (self.regs.f & CF)
                    | if self.regs.a == 0 { ZF } else { 0 }
                    | if self.regs.a & 0x80 != 0 { SF } else { 0 }
                    | (self.regs.a & (YF | XF))
                    | if self.regs.iff2 { PF } else { 0 };
            }

            // RRD
            0x67 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(4);
                self.queue_execute_followup();
            }

            // RLD
            0x6F => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(4);
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
                self.queue_internal(5);
                self.queue_execute_followup();
            }

            // INI
            0xA2 => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // OUTI
            0xA3 => {
                self.queue_internal(1);
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
                self.queue_internal(5);
                self.queue_execute_followup();
            }

            // IND
            0xAA => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // OUTD
            0xAB => {
                self.queue_internal(1);
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
                self.queue_internal(5);
                self.queue_execute_followup();
            }

            // INIR
            0xB2 => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // OTIR
            0xB3 => {
                self.queue_internal(1);
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
                self.queue_internal(5);
                self.queue_execute_followup();
            }

            // INDR
            0xBA => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // OTDR
            0xBB => {
                self.queue_internal(1);
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            _ => {
                // Undocumented ED - treat as NOP NOP
            }
        }
    }

    /// Execute DDCB or FDCB-prefixed instruction.
    pub(super) fn execute_ddcb_fdcb(&mut self) {
        let idx = self.get_index_reg();
        self.addr = idx.wrapping_add(self.displacement as i16 as u16);
        self.micro_ops.push(MicroOp::ReadMem);
        self.queue_internal(2);
        self.queue_execute_followup();
    }
}
