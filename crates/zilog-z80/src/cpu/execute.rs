//! Instruction execution for the Z80.

#![allow(clippy::too_many_lines)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]

use crate::alu;
use crate::flags::{CF, HF, NF, PF, SF, XF, YF, ZF, sz53p};
use crate::microcode::MicroOp;

use super::Z80;

impl Z80 {
    // =========================================================================
    // Unprefixed instructions
    // =========================================================================

    /// Execute unprefixed instruction.
    pub(super) fn execute_unprefixed(&mut self) {
        let op = self.opcode;

        match op {
            // NOP
            0x00 => {}

            // LD rr, nn (01=BC, 11=DE, 21=HL, 31=SP)
            0x01 | 0x11 | 0x21 | 0x31 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LD (BC), A
            0x02 => {
                self.addr = self.regs.bc();
                self.data_lo = self.regs.a;
                self.regs.wz = ((self.regs.a as u16) << 8) | ((self.addr.wrapping_add(1)) & 0xFF);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // INC rr (03=BC, 13=DE, 23=HL, 33=SP)
            0x03 | 0x13 | 0x23 | 0x33 => {
                self.queue_internal(2);
                let rp = (op >> 4) & 3;
                let val = self.get_reg16(rp).wrapping_add(1);
                self.set_reg16(rp, val);
            }

            // INC r (04=B, 0C=C, 14=D, 1C=E, 24=H, 2C=L, 3C=A)
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => {
                let r = (op >> 3) & 7;
                let val = self.get_reg8(r);
                let result = alu::inc8(val);
                self.set_reg8(r, result.value);
                self.set_f((self.regs.f & CF) | result.flags);
            }

            // DEC r (05=B, 0D=C, 15=D, 1D=E, 25=H, 2D=L, 3D=A)
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => {
                let r = (op >> 3) & 7;
                let val = self.get_reg8(r);
                let result = alu::dec8(val);
                self.set_reg8(r, result.value);
                self.set_f((self.regs.f & CF) | result.flags);
            }

            // LD r, n (06=B, 0E=C, 16=D, 1E=E, 26=H, 2E=L, 3E=A)
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x3E => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // RLCA
            0x07 => {
                let carry = self.regs.a >> 7;
                self.regs.a = (self.regs.a << 1) | carry;
                self.set_f(
                    (self.regs.f & (SF | ZF | PF))
                        | (self.regs.a & (YF | XF))
                        | if carry != 0 { CF } else { 0 },
                );
            }

            // EX AF, AF'
            0x08 => {
                let tmp_a = self.regs.a;
                let tmp_f = self.regs.f;
                self.regs.a = self.regs.a_alt;
                self.regs.f = self.regs.f_alt;
                self.regs.a_alt = tmp_a;
                self.regs.f_alt = tmp_f;
            }

            // ADD HL, rr (09=BC, 19=DE, 29=HL, 39=SP)
            0x09 | 0x19 | 0x29 | 0x39 => {
                self.queue_internal(7);
                let rp = (op >> 4) & 3;
                let hl = self.regs.hl();
                let rr = self.get_reg16(rp);
                self.regs.wz = hl.wrapping_add(1);
                let (result, flags) = alu::add16(hl, rr);
                self.regs.set_hl(result);
                self.set_f((self.regs.f & (SF | ZF | PF)) | flags);
            }

            // LD A, (BC)
            0x0A => {
                self.addr = self.regs.bc();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // DEC rr (0B=BC, 1B=DE, 2B=HL, 3B=SP)
            0x0B | 0x1B | 0x2B | 0x3B => {
                self.queue_internal(2);
                let rp = (op >> 4) & 3;
                let val = self.get_reg16(rp).wrapping_sub(1);
                self.set_reg16(rp, val);
            }

            // RRCA
            0x0F => {
                let carry = self.regs.a & 1;
                self.regs.a = (self.regs.a >> 1) | (carry << 7);
                self.set_f(
                    (self.regs.f & (SF | ZF | PF))
                        | (self.regs.a & (YF | XF))
                        | if carry != 0 { CF } else { 0 },
                );
            }

            // DJNZ e
            0x10 => {
                self.queue_internal(1);
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD (DE), A
            0x12 => {
                self.addr = self.regs.de();
                self.data_lo = self.regs.a;
                self.regs.wz = ((self.regs.a as u16) << 8) | ((self.addr.wrapping_add(1)) & 0xFF);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // RLA
            0x17 => {
                let old_carry = if self.regs.f & CF != 0 { 1 } else { 0 };
                let new_carry = self.regs.a >> 7;
                self.regs.a = (self.regs.a << 1) | old_carry;
                self.set_f(
                    (self.regs.f & (SF | ZF | PF))
                        | (self.regs.a & (YF | XF))
                        | if new_carry != 0 { CF } else { 0 },
                );
            }

            // JR e
            0x18 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD A, (DE)
            0x1A => {
                self.addr = self.regs.de();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // RRA
            0x1F => {
                let old_carry = if self.regs.f & CF != 0 { 0x80 } else { 0 };
                let new_carry = self.regs.a & 1;
                self.regs.a = (self.regs.a >> 1) | old_carry;
                self.set_f(
                    (self.regs.f & (SF | ZF | PF))
                        | (self.regs.a & (YF | XF))
                        | if new_carry != 0 { CF } else { 0 },
                );
            }

            // JR cc, e (20=NZ, 28=Z, 30=NC, 38=C)
            0x20 | 0x28 | 0x30 | 0x38 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD (nn), HL
            0x22 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // DAA
            0x27 => {
                let a = self.regs.a;
                let nf = self.regs.f & NF != 0;
                let cf = self.regs.f & CF != 0;
                let hf = self.regs.f & HF != 0;

                let mut correction: u8 = 0;
                let mut new_cf = cf;

                if hf || (a & 0x0F) > 9 {
                    correction |= 0x06;
                }
                if cf || a > 0x99 {
                    correction |= 0x60;
                    new_cf = true;
                }

                let result = if nf {
                    a.wrapping_sub(correction)
                } else {
                    a.wrapping_add(correction)
                };

                let new_hf = if nf {
                    hf && (a & 0x0F) < 6
                } else {
                    (a & 0x0F) > 9
                };

                self.regs.a = result;
                self.set_f(
                    sz53p(result)
                        | if nf { NF } else { 0 }
                        | if new_cf { CF } else { 0 }
                        | if new_hf { HF } else { 0 },
                );
            }

            // LD HL, (nn)
            0x2A => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // CPL
            0x2F => {
                self.regs.a = !self.regs.a;
                self.set_f(
                    (self.regs.f & (SF | ZF | PF | CF)) | HF | NF | (self.regs.a & (XF | YF)),
                );
            }

            // LD (nn), A
            0x32 => {
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

            // LD (HL), n
            0x36 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // SCF
            0x37 => {
                // Undocumented: X/Y flags from (prev_Q XOR F) OR A
                let q_xor_f = self.prev_q ^ self.regs.f;
                self.set_f(
                    (self.regs.f & (SF | ZF | PF)) | CF | ((q_xor_f | self.regs.a) & (XF | YF)),
                );
            }

            // LD A, (nn)
            0x3A => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // CCF
            0x3F => {
                let old_cf = self.regs.f & CF;
                // Undocumented: X/Y flags from (prev_Q XOR F) OR A
                let q_xor_f = self.prev_q ^ self.regs.f;
                self.set_f(
                    (self.regs.f & (SF | ZF | PF))
                        | ((q_xor_f | self.regs.a) & (XF | YF))
                        | if old_cf != 0 { HF } else { 0 }
                        | if old_cf == 0 { CF } else { 0 },
                );
            }

            // LD r, r' (40-7F except 76=HALT)
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
                    let value = self.get_reg8(src);
                    self.set_reg8(dst, value);
                }
            }

            // HALT
            0x76 => {
                self.regs.halted = true;
            }

            // ALU A, r (80-BF)
            0x80..=0xBF => {
                let r = op & 7;
                if r == 6 {
                    self.addr = self.regs.hl();
                    self.micro_ops.push(MicroOp::ReadMem);
                    self.queue_execute_followup();
                } else {
                    let value = self.get_reg8(r);
                    self.alu_a(op, value);
                }
            }

            // RET cc (C0=NZ, C8=Z, D0=NC, D8=C, E0=PO, E8=PE, F0=P, F8=M)
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
                let cc = (op >> 3) & 7;
                self.queue_internal(1);
                if self.condition(cc) {
                    self.addr = self.regs.sp;
                    self.micro_ops.push(MicroOp::ReadMem16Lo);
                    self.micro_ops.push(MicroOp::ReadMem16Hi);
                    self.queue_execute_followup();
                }
            }

            // POP rr (C1=BC, D1=DE, E1=HL, F1=AF)
            0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // JP cc, nn (C2=NZ, CA=Z, D2=NC, DA=C, E2=PO, EA=PE, F2=P, FA=M)
            0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => {
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

            // CALL cc, nn (C4=NZ, CC=Z, D4=NC, DC=C, E4=PO, EC=PE, F4=P, FC=M)
            0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // PUSH rr (C5=BC, D5=DE, E5=HL, F5=AF)
            0xC5 | 0xD5 | 0xE5 | 0xF5 => {
                self.queue_internal(1);
                let rp = (op >> 4) & 3;
                let val = self.get_reg16_af(rp);
                self.data_hi = (val >> 8) as u8;
                self.data_lo = val as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            // ALU A, n (C6=ADD, CE=ADC, D6=SUB, DE=SBC, E6=AND, EE=XOR, F6=OR, FE=CP)
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // RST n (C7=00, CF=08, D7=10, DF=18, E7=20, EF=28, F7=30, FF=38)
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                self.queue_internal(1);
                let target = u16::from(op & 0x38);
                self.regs.wz = target;
                let ret_addr = self.regs.pc;
                self.data_hi = (ret_addr >> 8) as u8;
                self.data_lo = ret_addr as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
                self.regs.pc = target;
            }

            // RET
            0xC9 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // CB prefix — handled by fetch
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

            // OUT (n), A
            0xD3 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // IN A, (n)
            0xDB => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // EXX
            0xD9 => {
                let tmp;
                tmp = self.regs.b;
                self.regs.b = self.regs.b_alt;
                self.regs.b_alt = tmp;
                let tmp2 = self.regs.c;
                self.regs.c = self.regs.c_alt;
                self.regs.c_alt = tmp2;
                let tmp3 = self.regs.d;
                self.regs.d = self.regs.d_alt;
                self.regs.d_alt = tmp3;
                let tmp4 = self.regs.e;
                self.regs.e = self.regs.e_alt;
                self.regs.e_alt = tmp4;
                let tmp5 = self.regs.h;
                self.regs.h = self.regs.h_alt;
                self.regs.h_alt = tmp5;
                let tmp6 = self.regs.l;
                self.regs.l = self.regs.l_alt;
                self.regs.l_alt = tmp6;
            }

            // DD prefix — handled by fetch
            0xDD => {
                self.prefix = 0xDD;
                self.micro_ops.push(MicroOp::FetchOpcode);
            }

            // EX (SP), HL
            0xE3 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
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

            // ED prefix — handled by fetch
            0xED => {
                self.prefix = 0xED;
                self.micro_ops.push(MicroOp::FetchOpcode);
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
                self.ei_delay = true;
            }

            // FD prefix — handled by fetch
            0xFD => {
                self.prefix = 0xFD;
                self.micro_ops.push(MicroOp::FetchOpcode);
            }

            // LD SP, HL
            0xF9 => {
                self.queue_internal(2);
                self.regs.sp = self.regs.hl();
            }

            _ => {
                // Should not happen — all 256 opcodes covered
                panic!(
                    "Unimplemented opcode: {:02X} at PC={:04X}",
                    op,
                    self.regs.pc.wrapping_sub(1)
                );
            }
        }
    }

    /// Perform ALU operation on A register.
    fn alu_a(&mut self, op: u8, value: u8) {
        let alu_op = (op >> 3) & 7;
        match alu_op {
            0 => {
                // ADD
                let result = alu::add8(self.regs.a, value, false);
                self.regs.a = result.value;
                self.set_f(result.flags);
            }
            1 => {
                // ADC
                let carry = self.regs.f & CF != 0;
                let result = alu::add8(self.regs.a, value, carry);
                self.regs.a = result.value;
                self.set_f(result.flags);
            }
            2 => {
                // SUB
                let result = alu::sub8(self.regs.a, value, false);
                self.regs.a = result.value;
                self.set_f(result.flags);
            }
            3 => {
                // SBC
                let carry = self.regs.f & CF != 0;
                let result = alu::sub8(self.regs.a, value, carry);
                self.regs.a = result.value;
                self.set_f(result.flags);
            }
            4 => {
                // AND
                self.regs.a &= value;
                self.set_f(sz53p(self.regs.a) | HF);
            }
            5 => {
                // XOR
                self.regs.a ^= value;
                self.set_f(sz53p(self.regs.a));
            }
            6 => {
                // OR
                self.regs.a |= value;
                self.set_f(sz53p(self.regs.a));
            }
            7 => {
                // CP
                let result = alu::sub8(self.regs.a, value, false);
                self.set_f((result.flags & !(YF | XF)) | (value & (YF | XF)));
            }
            _ => unreachable!(),
        }
    }

    // =========================================================================
    // Unprefixed follow-ups
    // =========================================================================

    /// Execute follow-up for instructions that need immediate/memory data.
    pub(super) fn execute_followup(&mut self) {
        // Dispatch based on prefix
        if self.prefix == 0xED {
            self.execute_ed_followup();
            return;
        }
        if (self.prefix == 0xDD || self.prefix == 0xFD) && self.prefix2 == 0xCB {
            if self.followup_stage <= 1 {
                // Stage 1: ReadImm8 just read the opcode byte into data_lo.
                // Copy it to opcode and dispatch to the DDCB handler which
                // sets up the memory read and queues the real followup.
                //
                // DDCB/FDCB instructions have no non-followup Execute (the
                // prefix fetches chain FetchOpcodes without Execute), so we
                // must clear per-instruction state here — the only Execute
                // in the entire DDCB flow is this followup.
                self.ei_delay = false;
                self.last_was_ld_a_ir = false;
                self.prev_q = self.last_q;
                self.last_q = 0;
                self.opcode = self.data_lo;
                self.execute_ddcb_fdcb();
                return;
            }
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
            // LD rr, nn (01=BC, 11=DE, 21=HL, 31=SP)
            0x01 | 0x11 | 0x21 | 0x31 => {
                let rp = (op >> 4) & 3;
                let val = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.set_reg16(rp, val);
            }

            // LD r, n (06=B, 0E=C, 16=D, 1E=E, 26=H, 2E=L, 3E=A)
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x3E => {
                let r = (op >> 3) & 7;
                self.set_reg8(r, self.data_lo);
            }

            // LD A, (BC)
            0x0A => {
                self.regs.a = self.data_lo;
                self.regs.wz = self.regs.bc().wrapping_add(1);
            }

            // DJNZ e
            0x10 => {
                self.regs.b = self.regs.b.wrapping_sub(1);
                if self.regs.b != 0 {
                    self.queue_internal(5);
                    let displacement = self.data_lo as i8;
                    self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                    self.regs.wz = self.regs.pc;
                }
            }

            // JR e
            0x18 => {
                self.queue_internal(5);
                let displacement = self.data_lo as i8;
                self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                self.regs.wz = self.regs.pc;
            }

            // LD A, (DE)
            0x1A => {
                self.regs.a = self.data_lo;
                self.regs.wz = self.regs.de().wrapping_add(1);
            }

            // JR cc, e (20=NZ, 28=Z, 30=NC, 38=C)
            0x20 | 0x28 | 0x30 | 0x38 => {
                let cc = (op >> 3) & 3; // Map: 20→0(NZ), 28→1(Z), 30→2(NC), 38→3(C)
                let taken = match cc {
                    0 => self.regs.f & ZF == 0, // NZ
                    1 => self.regs.f & ZF != 0, // Z
                    2 => self.regs.f & CF == 0, // NC
                    3 => self.regs.f & CF != 0, // C
                    _ => unreachable!(),
                };
                if taken {
                    self.queue_internal(5);
                    let displacement = self.data_lo as i8;
                    self.regs.pc = self.regs.pc.wrapping_add(displacement as u16);
                    self.regs.wz = self.regs.pc;
                }
            }

            // LD (nn), HL
            0x22 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = self.addr.wrapping_add(1);
                self.data_lo = self.regs.l;
                self.data_hi = self.regs.h;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD HL, (nn) — stage 2: data loaded
            0x2A if self.followup_stage >= 2 => {
                self.regs.l = self.data_lo;
                self.regs.h = self.data_hi;
            }

            // LD HL, (nn) — stage 1: set up memory read
            0x2A => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = self.addr.wrapping_add(1);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // LD (nn), A
            0x32 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = ((self.regs.a as u16) << 8) | ((self.addr.wrapping_add(1)) & 0xFF);
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // INC (HL)
            0x34 => {
                self.queue_internal(1);
                let result = alu::inc8(self.data_lo);
                self.data_lo = result.value;
                self.set_f((self.regs.f & CF) | result.flags);
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // DEC (HL)
            0x35 => {
                self.queue_internal(1);
                let result = alu::dec8(self.data_lo);
                self.data_lo = result.value;
                self.set_f((self.regs.f & CF) | result.flags);
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // LD (HL), n
            0x36 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // LD A, (nn) — stage 2
            0x3A if self.followup_stage >= 2 => {
                self.regs.a = self.data_lo;
            }

            // LD A, (nn) — stage 1
            0x3A => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = self.addr.wrapping_add(1);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LD r, (HL) follow-up
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                let dst = (op >> 3) & 7;
                self.set_reg8(dst, self.data_lo);
            }

            // ALU A, (HL) follow-up
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                self.alu_a(op, self.data_lo);
            }

            // RET cc follow-up (conditional returns)
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = addr;
                self.regs.pc = addr;
            }

            // POP rr (C1=BC, D1=DE, E1=HL, F1=AF)
            0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                let rp = (op >> 4) & 3;
                let val = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.set_reg16_af(rp, val);
            }

            // JP cc, nn follow-up
            0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => {
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = addr;
                let cc = (op >> 3) & 7;
                if self.condition(cc) {
                    self.regs.pc = addr;
                }
            }

            // JP nn
            0xC3 => {
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = addr;
                self.regs.pc = addr;
            }

            // CALL cc, nn follow-up
            0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => {
                let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = target;
                let cc = (op >> 3) & 7;
                if self.condition(cc) {
                    self.queue_internal(1);
                    let ret_addr = self.regs.pc;
                    self.data_hi = (ret_addr >> 8) as u8;
                    self.data_lo = ret_addr as u8;
                    self.micro_ops.push(MicroOp::WriteMemHiFirst);
                    self.micro_ops.push(MicroOp::WriteMemLoSecond);
                    self.regs.pc = target;
                }
            }

            // ALU A, n follow-up (C6=ADD, CE=ADC, D6=SUB, DE=SBC, E6=AND, EE=XOR, F6=OR, FE=CP)
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                self.alu_a(op, self.data_lo);
            }

            // RET
            0xC9 => {
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.sp = self.regs.sp.wrapping_add(2);
                self.regs.wz = addr;
                self.regs.pc = addr;
            }

            // CALL nn
            0xCD => {
                let target = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = target;
                self.queue_internal(1);
                let ret_addr = self.regs.pc;
                self.data_hi = (ret_addr >> 8) as u8;
                self.data_lo = ret_addr as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
                self.regs.pc = target;
            }

            // OUT (n), A — follow-up: port address = (A << 8) | n
            0xD3 => {
                let port = (u16::from(self.regs.a) << 8) | u16::from(self.data_lo);
                self.regs.wz =
                    ((self.regs.a as u16) << 8) | ((self.data_lo.wrapping_add(1)) as u16);
                self.addr = port;
                self.data_lo = self.regs.a;
                self.micro_ops.push(MicroOp::IoWrite);
            }

            // IN A, (n) — stage 2: store read value
            0xDB if self.followup_stage >= 2 => {
                self.regs.a = self.data_lo;
            }

            // IN A, (n) — follow-up: port address = (A << 8) | n
            0xDB => {
                let port = (u16::from(self.regs.a) << 8) | u16::from(self.data_lo);
                self.regs.wz = port.wrapping_add(1);
                self.addr = port;
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // EX (SP), HL — follow-up: read done, write HL to stack
            0xE3 => {
                // data_lo/data_hi have stack values
                let stack_lo = self.data_lo;
                let stack_hi = self.data_hi;
                self.queue_internal(1);
                // Write HL to (SP)
                self.data_hi = self.regs.h;
                self.data_lo = self.regs.l;
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
                self.queue_internal(2);
                // Load HL from stack values
                self.regs.l = stack_lo;
                self.regs.h = stack_hi;
                self.regs.wz = self.regs.hl();
            }

            _ => {
                panic!(
                    "Unimplemented followup: opcode={:02X} PC={:04X}",
                    op, self.regs.pc
                );
            }
        }
    }

    // =========================================================================
    // CB-prefixed instructions
    // =========================================================================

    /// Execute CB-prefixed instruction.
    pub(super) fn execute_cb(&mut self) {
        let op = self.opcode;
        let r = op & 7;

        if r == 6 {
            self.addr = self.regs.hl();
            self.micro_ops.push(MicroOp::ReadMem);
            self.queue_internal(1);
            self.queue_execute_followup();
            return;
        }

        let value = self.get_reg8(r);
        let result = self.execute_cb_operation(op, value, value);

        if let Some(res) = result {
            self.set_reg8(r, res);
        }
    }

    /// Execute CB-prefixed followup for (HL) operations.
    fn execute_cb_followup(&mut self) {
        let op = self.opcode;
        let value = self.data_lo;
        // For BIT n,(HL), X/Y flags come from high byte of WZ
        let flag_source = (self.regs.wz >> 8) as u8;

        let result = self.execute_cb_operation(op, value, flag_source);

        if let Some(res) = result {
            self.data_lo = res;
            self.micro_ops.push(MicroOp::WriteMem);
        }
    }

    /// Execute CB operation, returns Some(result) for write-back or None for BIT.
    fn execute_cb_operation(&mut self, op: u8, value: u8, flag_source: u8) -> Option<u8> {
        match op & 0xF8 {
            0x00 => {
                let res = alu::rlc8(value);
                self.set_f(res.flags);
                Some(res.value)
            }
            0x08 => {
                let res = alu::rrc8(value);
                self.set_f(res.flags);
                Some(res.value)
            }
            0x10 => {
                let res = alu::rl8(value, self.regs.f & CF != 0);
                self.set_f(res.flags);
                Some(res.value)
            }
            0x18 => {
                let res = alu::rr8(value, self.regs.f & CF != 0);
                self.set_f(res.flags);
                Some(res.value)
            }
            0x20 => {
                let res = alu::sla8(value);
                self.set_f(res.flags);
                Some(res.value)
            }
            0x28 => {
                let res = alu::sra8(value);
                self.set_f(res.flags);
                Some(res.value)
            }
            0x30 => {
                let res = alu::sll8(value);
                self.set_f(res.flags);
                Some(res.value)
            }
            0x38 => {
                let res = alu::srl8(value);
                self.set_f(res.flags);
                Some(res.value)
            }
            // BIT
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                let bit = (op >> 3) & 7;
                let mask = 1 << bit;
                let is_zero = value & mask == 0;

                let mut flags = self.regs.f & CF;
                flags |= HF;
                if is_zero {
                    flags |= ZF | PF;
                }
                if bit == 7 && !is_zero {
                    flags |= SF;
                }
                flags |= flag_source & (XF | YF);
                self.set_f(flags);
                None
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

    // =========================================================================
    // DD/FD-prefixed instructions
    // =========================================================================

    /// Execute DD/FD-prefixed instruction.
    pub(super) fn execute_dd_fd(&mut self) {
        let op = self.opcode;
        let _is_iy = self.prefix == 0xFD;

        match op {
            // ADD IX/IY, rr (09=BC, 19=DE, 29=IX/IY, 39=SP)
            0x09 | 0x19 | 0x29 | 0x39 => {
                self.queue_internal(7);
                let idx = self.get_index_reg();
                self.regs.wz = idx.wrapping_add(1);
                let rp = (op >> 4) & 3;
                let rr = self.get_reg16(rp);
                let (result, flags) = alu::add16(idx, rr);
                self.set_index_reg(result);
                self.set_f((self.regs.f & (SF | ZF | PF)) | flags);
            }

            // LD IX/IY, nn
            0x21 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // LD (nn), IX/IY
            0x22 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // INC IX/IY
            0x23 => {
                self.queue_internal(2);
                self.set_index_reg(self.get_index_reg().wrapping_add(1));
            }

            // INC IXH/IYH (undocumented)
            0x24 => {
                let val = (self.get_index_reg() >> 8) as u8;
                let result = alu::inc8(val);
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0x00FF) | ((result.value as u16) << 8));
                self.set_f((self.regs.f & CF) | result.flags);
            }

            // DEC IXH/IYH (undocumented)
            0x25 => {
                let val = (self.get_index_reg() >> 8) as u8;
                let result = alu::dec8(val);
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0x00FF) | ((result.value as u16) << 8));
                self.set_f((self.regs.f & CF) | result.flags);
            }

            // LD IXH/IYH, n (undocumented)
            0x26 => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
            }

            // LD IX/IY, (nn)
            0x2A => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // DEC IX/IY
            0x2B => {
                self.queue_internal(2);
                self.set_index_reg(self.get_index_reg().wrapping_sub(1));
            }

            // INC IXL/IYL (undocumented)
            0x2C => {
                let val = self.get_index_reg() as u8;
                let result = alu::inc8(val);
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0xFF00) | (result.value as u16));
                self.set_f((self.regs.f & CF) | result.flags);
            }

            // DEC IXL/IYL (undocumented)
            0x2D => {
                let val = self.get_index_reg() as u8;
                let result = alu::dec8(val);
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0xFF00) | (result.value as u16));
                self.set_f((self.regs.f & CF) | result.flags);
            }

            // LD IXL/IYL, n (undocumented)
            0x2E => {
                self.micro_ops.push(MicroOp::ReadImm8);
                self.queue_execute_followup();
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

            // LD r, (IX+d)/(IY+d)
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            // LD (IX+d)/(IY+d), r
            0x70 | 0x71 | 0x72 | 0x73 | 0x74 | 0x75 | 0x77 => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            // ALU A, (IX+d)/(IY+d)
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                self.micro_ops.push(MicroOp::FetchDisplacement);
                self.queue_execute_followup();
            }

            // Undocumented LD r, r' with IXH/IXL/IYH/IYL substitution
            0x40..=0x7F => {
                let src = op & 0x07;
                let dst = (op >> 3) & 0x07;
                let src_val = self.get_reg8_indexed(src);
                self.set_reg8_indexed(dst, src_val);
            }

            // ALU A, IXH/IXL/IYH/IYL (undocumented)
            0x84 | 0x85 | 0x8C | 0x8D | 0x94 | 0x95 | 0x9C | 0x9D | 0xA4 | 0xA5 | 0xAC | 0xAD
            | 0xB4 | 0xB5 | 0xBC | 0xBD => {
                let value = self.get_reg8_indexed(op & 7);
                self.alu_a(op, value);
            }

            // POP IX/IY
            0xE1 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // EX (SP), IX/IY
            0xE3 => {
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // PUSH IX/IY
            0xE5 => {
                self.queue_internal(1);
                let idx = self.get_index_reg();
                self.data_hi = (idx >> 8) as u8;
                self.data_lo = idx as u8;
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
            }

            // JP (IX)/(IY)
            0xE9 => {
                self.regs.pc = self.get_index_reg();
            }

            // LD SP, IX/IY
            0xF9 => {
                self.queue_internal(2);
                self.regs.sp = self.get_index_reg();
            }

            // All other DD/FD opcodes execute as unprefixed (prefix has no effect)
            _ => {
                // Reset prefix so unprefixed handlers work
                self.prefix = 0;
                self.execute_unprefixed();
            }
        }
    }

    /// Execute DD/FD followup.
    fn execute_dd_fd_followup(&mut self) {
        let op = self.opcode;
        let _is_iy = self.prefix == 0xFD;

        match op {
            // POP IX/IY
            0xE1 => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                let value = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.set_index_reg(value);
            }

            // EX (SP), IX/IY
            0xE3 => {
                let stack_lo = self.data_lo;
                let stack_hi = self.data_hi;
                self.queue_internal(1);
                let idx = self.get_index_reg();
                self.data_hi = (idx >> 8) as u8;
                self.data_lo = idx as u8;
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
                self.queue_internal(2);
                let new_val = u16::from(stack_lo) | (u16::from(stack_hi) << 8);
                self.set_index_reg(new_val);
                self.regs.wz = new_val;
            }

            // LD IX/IY, nn
            0x21 => {
                let value = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.set_index_reg(value);
            }

            // LD IXH/IYH, n (undocumented)
            0x26 => {
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0x00FF) | ((self.data_lo as u16) << 8));
            }

            // LD IXL/IYL, n (undocumented)
            0x2E => {
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0xFF00) | (self.data_lo as u16));
            }

            // LD (nn), IX/IY
            0x22 => {
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = addr.wrapping_add(1);
                let idx = self.get_index_reg();
                self.addr = addr;
                self.data_lo = idx as u8;
                self.data_hi = (idx >> 8) as u8;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD IX/IY, (nn) — stage 2
            0x2A if self.followup_stage >= 2 => {
                let value = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.set_index_reg(value);
            }

            // LD IX/IY, (nn) — stage 1
            0x2A => {
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = addr.wrapping_add(1);
                self.addr = addr;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // INC (IX+d)/(IY+d) — stage 2
            0x34 if self.followup_stage >= 2 => {
                let result = alu::inc8(self.data_lo);
                self.data_lo = result.value;
                self.set_f((self.regs.f & CF) | result.flags);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // INC (IX+d)/(IY+d) — stage 1
            0x34 => {
                let idx = self.get_index_reg();
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.regs.wz = self.addr;
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(1);
                self.queue_execute_followup();
            }

            // DEC (IX+d)/(IY+d) — stage 2
            0x35 if self.followup_stage >= 2 => {
                let result = alu::dec8(self.data_lo);
                self.data_lo = result.value;
                self.set_f((self.regs.f & CF) | result.flags);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // DEC (IX+d)/(IY+d) — stage 1
            0x35 => {
                let idx = self.get_index_reg();
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.regs.wz = self.addr;
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_internal(1);
                self.queue_execute_followup();
            }

            // LD (IX+d)/(IY+d), n
            0x36 => {
                let idx = self.get_index_reg();
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.regs.wz = self.addr;
                self.queue_internal(2);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // LD r, (IX+d)/(IY+d) — stage 2
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E if self.followup_stage >= 2 => {
                let dst = (op >> 3) & 7;
                self.set_reg8(dst, self.data_lo);
            }

            // LD r, (IX+d)/(IY+d) — stage 1
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                let idx = self.get_index_reg();
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.regs.wz = self.addr;
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // LD (IX+d)/(IY+d), r
            0x70 | 0x71 | 0x72 | 0x73 | 0x74 | 0x75 | 0x77 => {
                let idx = self.get_index_reg();
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.regs.wz = self.addr;
                self.data_lo = self.get_reg8(op & 7);
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::WriteMem);
            }

            // ALU (IX+d)/(IY+d) — stage 2
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE if self.followup_stage >= 2 => {
                self.alu_a(op, self.data_lo);
            }

            // ALU (IX+d)/(IY+d) — stage 1
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                let idx = self.get_index_reg();
                self.addr = idx.wrapping_add(self.displacement as i16 as u16);
                self.regs.wz = self.addr;
                self.queue_internal(5);
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            _ => {
                // Should be handled by fallthrough to unprefixed in execute_dd_fd
                panic!(
                    "Unimplemented DD/FD followup: opcode={:02X} PC={:04X}",
                    op, self.regs.pc
                );
            }
        }
    }

    // =========================================================================
    // ED-prefixed instructions
    // =========================================================================

    /// Execute ED-prefixed instruction.
    pub(super) fn execute_ed(&mut self) {
        let op = self.opcode;

        match op {
            // IN r, (C) (40=B, 48=C, 50=D, 58=E, 60=H, 68=L, 78=A)
            // Also 70 = IN (C) - result discarded but flags set
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_add(1);
                self.micro_ops.push(MicroOp::IoRead);
                self.queue_execute_followup();
            }

            // OUT (C), r (41=B, 49=C, 51=D, 59=E, 61=H, 69=L, 79=A)
            // Also 71 = OUT (C), 0
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x71 | 0x79 => {
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_add(1);
                let r = (op >> 3) & 7;
                self.data_lo = if r == 6 { 0 } else { self.get_reg8(r) };
                self.micro_ops.push(MicroOp::IoWrite);
            }

            // SBC HL, rr (42=BC, 52=DE, 62=HL, 72=SP)
            0x42 | 0x52 | 0x62 | 0x72 => {
                self.queue_internal(7);
                let rp = (op >> 4) & 3;
                let hl = self.regs.hl();
                self.regs.wz = hl.wrapping_add(1);
                let rr = self.get_reg16(rp);
                let (result, flags) = alu::sbc16(hl, rr, self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.set_f(flags);
            }

            // LD (nn), rr (43=BC, 53=DE, 63=HL, 73=SP)
            0x43 | 0x53 | 0x63 | 0x73 => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // NEG (and undocumented mirrors)
            0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => {
                let result = alu::sub8(0, self.regs.a, false);
                self.regs.a = result.value;
                self.set_f(result.flags);
            }

            // RETN (and undocumented mirrors)
            0x45 | 0x55 | 0x65 | 0x75 => {
                self.regs.iff1 = self.regs.iff2;
                self.addr = self.regs.sp;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // IM 0 (and undocumented mirrors)
            0x46 | 0x66 | 0x4E | 0x6E => {
                self.regs.im = 0;
            }

            // LD I, A
            0x47 => {
                self.queue_internal(1);
                self.regs.i = self.regs.a;
            }

            // ADC HL, rr (4A=BC, 5A=DE, 6A=HL, 7A=SP)
            0x4A | 0x5A | 0x6A | 0x7A => {
                self.queue_internal(7);
                let rp = (op >> 4) & 3;
                let hl = self.regs.hl();
                self.regs.wz = hl.wrapping_add(1);
                let rr = self.get_reg16(rp);
                let (result, flags) = alu::adc16(hl, rr, self.regs.f & CF != 0);
                self.regs.set_hl(result);
                self.set_f(flags);
            }

            // LD rr, (nn) (4B=BC, 5B=DE, 6B=HL, 7B=SP)
            0x4B | 0x5B | 0x6B | 0x7B => {
                self.micro_ops.push(MicroOp::ReadImm16Lo);
                self.micro_ops.push(MicroOp::ReadImm16Hi);
                self.queue_execute_followup();
            }

            // RETI (and undocumented mirrors)
            0x4D | 0x5D | 0x6D | 0x7D => {
                self.regs.iff1 = self.regs.iff2;
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

            // IM 1
            0x56 | 0x76 => {
                self.regs.im = 1;
            }

            // LD A, I
            0x57 => {
                self.queue_internal(1);
                self.regs.a = self.regs.i;
                self.set_f(
                    (self.regs.f & CF)
                        | if self.regs.a & 0x80 != 0 { SF } else { 0 }
                        | if self.regs.a == 0 { ZF } else { 0 }
                        | (self.regs.a & (YF | XF))
                        | if self.regs.iff2 { PF } else { 0 },
                );
                self.last_was_ld_a_ir = true;
            }

            // IM 2
            0x5E | 0x7E => {
                self.regs.im = 2;
            }

            // LD A, R
            0x5F => {
                self.queue_internal(1);
                self.regs.a = self.regs.r;
                self.set_f(
                    (self.regs.f & CF)
                        | if self.regs.a & 0x80 != 0 { SF } else { 0 }
                        | if self.regs.a == 0 { ZF } else { 0 }
                        | (self.regs.a & (YF | XF))
                        | if self.regs.iff2 { PF } else { 0 },
                );
                self.last_was_ld_a_ir = true;
            }

            // RRD
            0x67 => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
                self.queue_execute_followup();
            }

            // RLD
            0x6F => {
                self.addr = self.regs.hl();
                self.micro_ops.push(MicroOp::ReadMem);
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

            // INI
            0xA2 => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_add(1);
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
                self.queue_execute_followup();
            }

            // IND
            0xAA => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_sub(1);
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
                self.queue_execute_followup();
            }

            // INIR
            0xB2 => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_add(1);
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
                self.queue_execute_followup();
            }

            // INDR
            0xBA => {
                self.queue_internal(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_sub(1);
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

            // All undefined ED opcodes are NOP (8 T-states total: 4 for ED fetch + 4 for opcode)
            _ => {}
        }
    }

    /// Execute follow-up for ED-prefixed instructions.
    fn execute_ed_followup(&mut self) {
        let op = self.opcode;

        match op {
            // IN r, (C) follow-up
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
                let r = (op >> 3) & 7;
                if r != 6 {
                    self.set_reg8(r, self.data_lo);
                }
                self.set_f(sz53p(self.data_lo) | (self.regs.f & CF));
            }

            // RETN/RETI follow-up
            0x45 | 0x55 | 0x65 | 0x75 | 0x4D | 0x5D | 0x6D | 0x7D => {
                self.regs.sp = self.regs.sp.wrapping_add(2);
                let addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = addr;
                self.regs.pc = addr;
            }

            // LD (nn), rr (43=BC, 53=DE, 63=HL, 73=SP)
            0x43 | 0x53 | 0x63 | 0x73 => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = self.addr.wrapping_add(1);
                let rp = (op >> 4) & 3;
                let val = self.get_reg16(rp);
                self.data_lo = val as u8;
                self.data_hi = (val >> 8) as u8;
                self.micro_ops.push(MicroOp::WriteMem16Lo);
                self.micro_ops.push(MicroOp::WriteMem16Hi);
            }

            // LD rr, (nn) — stage 2
            0x4B | 0x5B | 0x6B | 0x7B if self.followup_stage >= 2 => {
                let rp = (op >> 4) & 3;
                let val = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.set_reg16(rp, val);
            }

            // LD rr, (nn) — stage 1
            0x4B | 0x5B | 0x6B | 0x7B => {
                self.addr = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
                self.regs.wz = self.addr.wrapping_add(1);
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);
                self.queue_execute_followup();
            }

            // RRD
            0x67 => {
                let mem = self.data_lo;
                self.queue_internal(4);
                self.regs.wz = self.regs.hl().wrapping_add(1);
                let new_a = (self.regs.a & 0xF0) | (mem & 0x0F);
                let new_mem = ((self.regs.a & 0x0F) << 4) | ((mem >> 4) & 0x0F);
                self.regs.a = new_a;
                self.data_lo = new_mem;
                self.micro_ops.push(MicroOp::WriteMem);
                self.set_f(sz53p(self.regs.a) | (self.regs.f & CF));
            }

            // RLD
            0x6F => {
                let mem = self.data_lo;
                self.queue_internal(4);
                self.regs.wz = self.regs.hl().wrapping_add(1);
                let new_a = (self.regs.a & 0xF0) | ((mem >> 4) & 0x0F);
                let new_mem = ((mem & 0x0F) << 4) | (self.regs.a & 0x0F);
                self.regs.a = new_a;
                self.data_lo = new_mem;
                self.micro_ops.push(MicroOp::WriteMem);
                self.set_f(sz53p(self.regs.a) | (self.regs.f & CF));
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
                self.set_f(
                    (self.regs.f & (SF | ZF | CF))
                        | (n & XF)
                        | if n & 0x02 != 0 { YF } else { 0 }
                        | if self.regs.bc() != 0 { PF } else { 0 },
                );
            }

            // CPI
            0xA1 => {
                let value = self.data_lo;
                self.queue_internal(5);
                self.regs.wz = self.regs.wz.wrapping_add(1);
                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));
                self.set_f(
                    (self.regs.f & CF)
                        | NF
                        | if result == 0 { ZF } else { 0 }
                        | if result & 0x80 != 0 { SF } else { 0 }
                        | if hf { HF } else { 0 }
                        | (n & XF)
                        | if n & 0x02 != 0 { YF } else { 0 }
                        | if self.regs.bc() != 0 { PF } else { 0 },
                );
            }

            // INI
            0xA2 => {
                let value = self.data_lo;
                self.addr = self.regs.hl();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                let k = u16::from(value) + u16::from(self.regs.c.wrapping_add(1));
                self.set_f(
                    if self.regs.b == 0 { ZF } else { 0 }
                        | (self.regs.b & (SF | YF | XF))
                        | if value & 0x80 != 0 { NF } else { 0 }
                        | if (k & 0xFF) < value as u16 {
                            HF | CF
                        } else {
                            0
                        }
                        | sz53p((k as u8) & 7 ^ self.regs.b) & PF,
                );
            }

            // OUTI
            0xA3 => {
                let value = self.data_lo;
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_add(1);
                self.data_lo = value;
                self.micro_ops.push(MicroOp::IoWrite);
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                let k = u16::from(value) + u16::from(self.regs.l);
                self.set_f(
                    if self.regs.b == 0 { ZF } else { 0 }
                        | (self.regs.b & (SF | YF | XF))
                        | if value & 0x80 != 0 { NF } else { 0 }
                        | if (k & 0xFF) < value as u16 {
                            HF | CF
                        } else {
                            0
                        }
                        | sz53p((k as u8) & 7 ^ self.regs.b) & PF,
                );
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
                self.set_f(
                    (self.regs.f & (SF | ZF | CF))
                        | (n & XF)
                        | if n & 0x02 != 0 { YF } else { 0 }
                        | if self.regs.bc() != 0 { PF } else { 0 },
                );
            }

            // CPD
            0xA9 => {
                let value = self.data_lo;
                self.queue_internal(5);
                self.regs.wz = self.regs.wz.wrapping_sub(1);
                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));
                self.set_f(
                    (self.regs.f & CF)
                        | NF
                        | if result == 0 { ZF } else { 0 }
                        | if result & 0x80 != 0 { SF } else { 0 }
                        | if hf { HF } else { 0 }
                        | (n & XF)
                        | if n & 0x02 != 0 { YF } else { 0 }
                        | if self.regs.bc() != 0 { PF } else { 0 },
                );
            }

            // IND
            0xAA => {
                let value = self.data_lo;
                self.addr = self.regs.hl();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                let k = u16::from(value) + u16::from(self.regs.c.wrapping_sub(1));
                self.set_f(
                    if self.regs.b == 0 { ZF } else { 0 }
                        | (self.regs.b & (SF | YF | XF))
                        | if value & 0x80 != 0 { NF } else { 0 }
                        | if (k & 0xFF) < value as u16 {
                            HF | CF
                        } else {
                            0
                        }
                        | sz53p((k as u8) & 7 ^ self.regs.b) & PF,
                );
            }

            // OUTD
            0xAB => {
                let value = self.data_lo;
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_sub(1);
                self.data_lo = value;
                self.micro_ops.push(MicroOp::IoWrite);
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                let k = u16::from(value) + u16::from(self.regs.l);
                self.set_f(
                    if self.regs.b == 0 { ZF } else { 0 }
                        | (self.regs.b & (SF | YF | XF))
                        | if value & 0x80 != 0 { NF } else { 0 }
                        | if (k & 0xFF) < value as u16 {
                            HF | CF
                        } else {
                            0
                        }
                        | sz53p((k as u8) & 7 ^ self.regs.b) & PF,
                );
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
                if self.regs.bc() != 0 {
                    // Repeat: XF/YF come from PCH after the PC decrement.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    self.set_f((self.regs.f & (SF | ZF | CF)) | PF | (pch & (XF | YF)));
                } else {
                    self.set_f(
                        (self.regs.f & (SF | ZF | CF))
                            | (n & XF)
                            | if n & 0x02 != 0 { YF } else { 0 },
                    );
                }
            }

            // CPIR
            0xB1 => {
                let value = self.data_lo;
                self.queue_internal(5);
                self.regs.wz = self.regs.wz.wrapping_add(1);
                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));
                let base_f = (self.regs.f & CF)
                    | NF
                    | if result == 0 { ZF } else { 0 }
                    | if result & 0x80 != 0 { SF } else { 0 }
                    | if hf { HF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };
                if self.regs.bc() != 0 && result != 0 {
                    // Repeat: XF/YF come from PCH.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    self.set_f(base_f | (pch & (XF | YF)));
                } else {
                    self.set_f(base_f | (n & XF) | if n & 0x02 != 0 { YF } else { 0 });
                }
            }

            // INIR
            0xB2 => {
                let value = self.data_lo;
                self.addr = self.regs.hl();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                let k = u16::from(value) + u16::from(self.regs.c.wrapping_add(1));
                let hcf = k > 255;
                let nf = value & 0x80 != 0;
                let p = ((k as u8) & 7) ^ self.regs.b;
                if self.regs.b != 0 {
                    // Repeat: HF/PF recomputed, XF/YF from PCH, WZ = PC + 1.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    let (hf, pf) = if hcf {
                        if nf {
                            (
                                if self.regs.b & 0x0F == 0 { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_sub(1) & 7)) & PF,
                            )
                        } else {
                            (
                                if self.regs.b & 0x0F == 0x0F { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_add(1) & 7)) & PF,
                            )
                        }
                    } else {
                        (0, sz53p(p ^ (self.regs.b & 7)) & PF)
                    };
                    self.set_f(
                        (self.regs.b & SF)
                            | (pch & (XF | YF))
                            | if nf { NF } else { 0 }
                            | if hcf { CF } else { 0 }
                            | hf
                            | pf,
                    );
                } else {
                    self.set_f(
                        ZF | if nf { NF } else { 0 }
                            | if hcf { HF | CF } else { 0 }
                            | sz53p(p) & PF,
                    );
                }
            }

            // OTIR
            0xB3 => {
                let value = self.data_lo;
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_add(1);
                self.data_lo = value;
                self.micro_ops.push(MicroOp::IoWrite);
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                let k = u16::from(value) + u16::from(self.regs.l);
                let hcf = k > 255;
                let nf = value & 0x80 != 0;
                let p = ((k as u8) & 7) ^ self.regs.b;
                if self.regs.b != 0 {
                    // Repeat: HF/PF recomputed, XF/YF from PCH, WZ = PC + 1.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    let (hf, pf) = if hcf {
                        if nf {
                            (
                                if self.regs.b & 0x0F == 0 { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_sub(1) & 7)) & PF,
                            )
                        } else {
                            (
                                if self.regs.b & 0x0F == 0x0F { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_add(1) & 7)) & PF,
                            )
                        }
                    } else {
                        (0, sz53p(p ^ (self.regs.b & 7)) & PF)
                    };
                    self.set_f(
                        (self.regs.b & SF)
                            | (pch & (XF | YF))
                            | if nf { NF } else { 0 }
                            | if hcf { CF } else { 0 }
                            | hf
                            | pf,
                    );
                } else {
                    self.set_f(
                        ZF | if nf { NF } else { 0 }
                            | if hcf { HF | CF } else { 0 }
                            | sz53p(p) & PF,
                    );
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
                if self.regs.bc() != 0 {
                    // Repeat: XF/YF come from PCH after the PC decrement.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    self.set_f((self.regs.f & (SF | ZF | CF)) | PF | (pch & (XF | YF)));
                } else {
                    self.set_f(
                        (self.regs.f & (SF | ZF | CF))
                            | (n & XF)
                            | if n & 0x02 != 0 { YF } else { 0 },
                    );
                }
            }

            // CPDR
            0xB9 => {
                let value = self.data_lo;
                self.queue_internal(5);
                self.regs.wz = self.regs.wz.wrapping_sub(1);
                let result = self.regs.a.wrapping_sub(value);
                let hf = (self.regs.a & 0x0F) < (value & 0x0F);
                let n = result.wrapping_sub(if hf { 1 } else { 0 });
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));
                let base_f = (self.regs.f & CF)
                    | NF
                    | if result == 0 { ZF } else { 0 }
                    | if result & 0x80 != 0 { SF } else { 0 }
                    | if hf { HF } else { 0 }
                    | if self.regs.bc() != 0 { PF } else { 0 };
                if self.regs.bc() != 0 && result != 0 {
                    // Repeat: XF/YF come from PCH.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    self.set_f(base_f | (pch & (XF | YF)));
                } else {
                    self.set_f(base_f | (n & XF) | if n & 0x02 != 0 { YF } else { 0 });
                }
            }

            // INDR
            0xBA => {
                let value = self.data_lo;
                self.addr = self.regs.hl();
                self.data_lo = value;
                self.micro_ops.push(MicroOp::WriteMem);
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                let k = u16::from(value) + u16::from(self.regs.c.wrapping_sub(1));
                let hcf = k > 255;
                let nf = value & 0x80 != 0;
                let p = ((k as u8) & 7) ^ self.regs.b;
                if self.regs.b != 0 {
                    // Repeat: HF/PF recomputed, XF/YF from PCH, WZ = PC + 1.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    let (hf, pf) = if hcf {
                        if nf {
                            (
                                if self.regs.b & 0x0F == 0 { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_sub(1) & 7)) & PF,
                            )
                        } else {
                            (
                                if self.regs.b & 0x0F == 0x0F { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_add(1) & 7)) & PF,
                            )
                        }
                    } else {
                        (0, sz53p(p ^ (self.regs.b & 7)) & PF)
                    };
                    self.set_f(
                        (self.regs.b & SF)
                            | (pch & (XF | YF))
                            | if nf { NF } else { 0 }
                            | if hcf { CF } else { 0 }
                            | hf
                            | pf,
                    );
                } else {
                    self.set_f(
                        ZF | if nf { NF } else { 0 }
                            | if hcf { HF | CF } else { 0 }
                            | sz53p(p) & PF,
                    );
                }
            }

            // OTDR
            0xBB => {
                let value = self.data_lo;
                self.regs.b = self.regs.b.wrapping_sub(1);
                self.addr = self.regs.bc();
                self.regs.wz = self.addr.wrapping_sub(1);
                self.data_lo = value;
                self.micro_ops.push(MicroOp::IoWrite);
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                let k = u16::from(value) + u16::from(self.regs.l);
                let hcf = k > 255;
                let nf = value & 0x80 != 0;
                let p = ((k as u8) & 7) ^ self.regs.b;
                if self.regs.b != 0 {
                    // Repeat: HF/PF recomputed, XF/YF from PCH, WZ = PC + 1.
                    self.queue_internal(5);
                    self.regs.pc = self.regs.pc.wrapping_sub(2);
                    self.regs.wz = self.regs.pc.wrapping_add(1);
                    let pch = (self.regs.pc >> 8) as u8;
                    let (hf, pf) = if hcf {
                        if nf {
                            (
                                if self.regs.b & 0x0F == 0 { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_sub(1) & 7)) & PF,
                            )
                        } else {
                            (
                                if self.regs.b & 0x0F == 0x0F { HF } else { 0 },
                                sz53p(p ^ (self.regs.b.wrapping_add(1) & 7)) & PF,
                            )
                        }
                    } else {
                        (0, sz53p(p ^ (self.regs.b & 7)) & PF)
                    };
                    self.set_f(
                        (self.regs.b & SF)
                            | (pch & (XF | YF))
                            | if nf { NF } else { 0 }
                            | if hcf { CF } else { 0 }
                            | hf
                            | pf,
                    );
                } else {
                    self.set_f(
                        ZF | if nf { NF } else { 0 }
                            | if hcf { HF | CF } else { 0 }
                            | sz53p(p) & PF,
                    );
                }
            }

            _ => {
                panic!(
                    "Unimplemented ED followup: opcode={:02X} PC={:04X}",
                    op, self.regs.pc
                );
            }
        }
    }

    // =========================================================================
    // DDCB / FDCB instructions
    // =========================================================================

    /// Execute DDCB or FDCB-prefixed instruction.
    pub(super) fn execute_ddcb_fdcb(&mut self) {
        let idx = self.get_index_reg();
        self.addr = idx.wrapping_add(self.displacement as i16 as u16);
        self.regs.wz = self.addr;

        self.micro_ops.push(MicroOp::ReadMem);
        self.queue_internal(2);
        self.queue_execute_followup();
    }

    /// Execute DDCB/FDCB followup after memory read.
    fn execute_ddcb_fdcb_followup(&mut self) {
        let op = self.opcode;
        let value = self.data_lo;
        let r = op & 7;

        let result = match op {
            0x00..=0x07 => {
                let res = alu::rlc8(value);
                self.set_f(res.flags);
                res.value
            }
            0x08..=0x0F => {
                let res = alu::rrc8(value);
                self.set_f(res.flags);
                res.value
            }
            0x10..=0x17 => {
                let res = alu::rl8(value, self.regs.f & CF != 0);
                self.set_f(res.flags);
                res.value
            }
            0x18..=0x1F => {
                let res = alu::rr8(value, self.regs.f & CF != 0);
                self.set_f(res.flags);
                res.value
            }
            0x20..=0x27 => {
                let res = alu::sla8(value);
                self.set_f(res.flags);
                res.value
            }
            0x28..=0x2F => {
                let res = alu::sra8(value);
                self.set_f(res.flags);
                res.value
            }
            0x30..=0x37 => {
                let res = alu::sll8(value);
                self.set_f(res.flags);
                res.value
            }
            0x38..=0x3F => {
                let res = alu::srl8(value);
                self.set_f(res.flags);
                res.value
            }
            // BIT
            0x40..=0x7F => {
                let bit = (op >> 3) & 7;
                let mask = 1 << bit;
                let is_zero = value & mask == 0;
                let mut flags = self.regs.f & CF;
                flags |= HF;
                if is_zero {
                    flags |= ZF | PF;
                }
                if bit == 7 && !is_zero {
                    flags |= SF;
                }
                flags |= ((self.addr >> 8) as u8) & (XF | YF);
                self.set_f(flags);
                return; // BIT doesn't write back
            }
            // RES
            0x80..=0xBF => {
                let bit = (op >> 3) & 7;
                value & !(1 << bit)
            }
            // SET
            0xC0..=0xFF => {
                let bit = (op >> 3) & 7;
                value | (1 << bit)
            }
        };

        self.data_lo = result;
        self.micro_ops.push(MicroOp::WriteMem);

        // Undocumented: if r != 6, also copy result to register
        if r != 6 {
            self.set_reg8(r, result);
        }
    }
}
