//! Miscellaneous instructions (group 0x4).
//!
//! This is the largest instruction group on the 68000:
//! NOP, CLR, NEG, NEGX, NOT, TST, JMP, JSR, RTS, RTE, RTR,
//! SWAP, EXT, PEA, LINK, UNLK, MOVEM, MOVE to/from SR/CCR/USP,
//! TAS, CHK, TRAP, TRAPV, RESET, STOP.
//!
//! Implementation is incremental — not all instructions are here yet.

use crate::addressing::AddrMode;
use crate::alu::{self, Size};
use crate::cpu::Cpu68000;
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Decode group 0x4 instructions.
    pub(crate) fn exec_group4(&mut self) {
        let op = self.ir;

        // LEA: 0100 RRR 111 MMMRRR (checked early so LEA followups work)
        if (op >> 6) & 7 == 7 {
            self.exec_lea();
            return;
        }

        // Handle followups for misc group 4 instructions
        if self.in_followup {
            match self.followup_tag {
                90 => { self.misc_rmw_alu(); return; }
                91 => { self.misc_abslong_ext2(); return; }
                92 => { self.jsr_push_pc(); return; }
                93 => { self.rts_read_hi(); return; }
                94 => { self.rts_jump(); return; }
                95 => { self.rte_read_pc_hi(); return; }
                96 => { self.rte_read_pc_lo(); return; }
                97 => { self.rte_jump(); return; }
                120 => { self.link_write_disp(); return; }
                121 => { self.pea_abslong_ext2(); return; }
                122 => { self.move_to_ccr_sr_complete(); return; }
                123 => { self.move_to_ccr_sr_abslong_ext2(); return; }
                124 => { self.rtr_read_pc_hi(); return; }
                125 => { self.rtr_read_pc_lo(); return; }
                126 => { self.rtr_jump(); return; }
                127 => { self.unlk_read_hi(); return; }
                128 => { self.unlk_read_lo(); return; }
                130 => { self.movem_reg_to_mem_abslong_ext2(); return; }
                131 => { self.movem_reg_to_mem_transfer(); return; }
                132 => { self.movem_mem_to_reg_abslong_ext2(); return; }
                133 => { self.movem_mem_to_reg_transfer(); return; }
                140 => { self.movem_reg_to_mem_resolve_ea(); return; }
                141 => { self.movem_mem_to_reg_resolve_ea(); return; }
                134 => { self.chk_ea_complete(); return; }
                135 => { self.chk_abslong_ext2(); return; }
                136 => { self.tas_rmw_complete(); return; }
                137 => { self.tas_abslong_ext2(); return; }
                138 => { self.move_from_sr_abslong_ext2(); return; }
                139 => { self.move_from_sr_rmw_write(); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        // CHK: 0100 RRR 110 MMMRRR (word size)
        if (op >> 6) & 7 == 6 {
            self.exec_chk();
            return;
        }

        // Decode by bits 11-8
        let sub = ((op >> 8) & 0xF) as u8;
        let size_bits = ((op >> 6) & 3) as u8;

        match sub {
            // 0100 0000 SS MMMRRR = NEGX
            0b0000 => {
                if size_bits == 3 {
                    // MOVE from SR: 0100 0000 11 MMMRRR
                    self.exec_move_from_sr();
                } else {
                    self.exec_unary_rmw(UnaryOp::Negx);
                }
            }
            // 0100 0010 SS MMMRRR = CLR
            0b0010 => {
                if size_bits == 3 {
                    // 0100 0010 11 = undefined on 68000 (MOVE from CCR on 68010+)
                    self.illegal_instruction();
                } else {
                    self.exec_clr();
                }
            }
            // 0100 0100 SS MMMRRR = NEG
            0b0100 => {
                if size_bits == 3 {
                    // MOVE to CCR: 0100 0100 11 MMMRRR
                    self.exec_move_to_ccr();
                } else {
                    self.exec_unary_rmw(UnaryOp::Neg);
                }
            }
            // 0100 0110 SS MMMRRR = NOT
            0b0110 => {
                if size_bits == 3 {
                    // MOVE to SR: 0100 0110 11 MMMRRR (privileged)
                    self.exec_move_to_sr();
                } else {
                    self.exec_unary_rmw(UnaryOp::Not);
                }
            }
            // 0100 1000 SS MMMRRR = various
            0b1000 => {
                if size_bits == 0 {
                    // NBCD: 0100 1000 00 MMMRRR
                    self.exec_nbcd();
                } else if size_bits == 1 {
                    let ea_mode = ((op >> 3) & 7) as u8;
                    if ea_mode == 0 {
                        // SWAP: 0100 1000 01 000 RRR
                        self.exec_swap();
                    } else {
                        // PEA: 0100 1000 01 MMMRRR
                        self.exec_pea();
                    }
                } else if size_bits == 2 {
                    let ea_mode = ((op >> 3) & 7) as u8;
                    if ea_mode == 0 {
                        // EXT.w: 0100 1000 10 000 RRR
                        self.exec_ext(Size::Word);
                    } else {
                        // MOVEM.w register-to-memory
                        self.exec_movem_reg_to_mem(Size::Word);
                    }
                } else {
                    // size_bits == 3
                    let ea_mode = ((op >> 3) & 7) as u8;
                    if ea_mode == 0 {
                        // EXT.l: 0100 1000 11 000 RRR
                        self.exec_ext(Size::Long);
                    } else {
                        // MOVEM.l register-to-memory
                        self.exec_movem_reg_to_mem(Size::Long);
                    }
                }
            }
            // 0100 1010 SS MMMRRR = TST / TAS / ILLEGAL
            0b1010 => {
                if size_bits == 3 {
                    let ea_mode = ((op >> 3) & 7) as u8;
                    let ea_reg = (op & 7) as u8;
                    if ea_mode == 7 && ea_reg == 4 {
                        // ILLEGAL: 0100 1010 1111 1100 = $4AFC
                        self.illegal_instruction();
                    } else {
                        // TAS: 0100 1010 11 MMMRRR
                        self.exec_tas();
                    }
                } else {
                    self.exec_tst();
                }
            }
            // 0100 1100 SS MMMRRR = MOVEM (memory-to-register)
            0b1100 => {
                if size_bits == 0 || size_bits == 1 {
                    self.illegal_instruction();
                } else if size_bits == 2 {
                    // MOVEM.w memory-to-register
                    self.exec_movem_mem_to_reg(Size::Word);
                } else {
                    // MOVEM.l memory-to-register
                    self.exec_movem_mem_to_reg(Size::Long);
                }
            }
            // 0100 1110 ... = misc control
            0b1110 => {
                self.exec_group4_misc_control();
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Decode 0100 1110 xx xxxxxx instructions.
    ///
    /// bits 7-6 split:
    ///   00 = reserved/unused on 68000
    ///   01 = TRAP, LINK, UNLK, MOVE USP, RESET, NOP, STOP, RTE, RTS, TRAPV, RTR
    ///   10 = JSR ($4E80-$4EBF)
    ///   11 = JMP ($4EC0-$4EFF)
    fn exec_group4_misc_control(&mut self) {
        let op = self.ir;
        let bits_7_6 = ((op >> 6) & 3) as u8;

        match bits_7_6 {
            0 => self.illegal_instruction(),
            1 => {
                // $4E40-$4E7F: TRAP, LINK, UNLK, MOVE USP, misc control
                match op & 0xFFF8 {
                    0x4E40 | 0x4E48 => {
                        // TRAP #vector ($4E40-$4E4F)
                        let vector = (op & 0xF) as u8;
                        // PC pushed = next instruction (past TRAP opcode)
                        self.exception_pc_override =
                            Some(self.instr_start_pc.wrapping_add(2));
                        self.exception(32 + vector, 0);
                    }
                    0x4E50 => {
                        // LINK An,#d16 ($4E50-$4E57)
                        self.exec_link();
                    }
                    0x4E58 => {
                        // UNLK An ($4E58-$4E5F)
                        self.exec_unlk();
                    }
                    0x4E60 | 0x4E68 => {
                        // MOVE USP ($4E60-$4E6F)
                        self.exec_move_usp();
                    }
                    0x4E70 => {
                        // $4E70-$4E77: individual special instructions
                        match op & 7 {
                            0 => self.exec_reset(),
                            1 => self.exec_nop(),
                            2 => {
                                // STOP: privileged, load immediate into SR, halt
                                self.exec_stop();
                            }
                            3 => self.exec_rte(),
                            5 => self.exec_rts(),
                            6 => {
                                // TRAPV: if V set, trap vector 7
                                self.exec_trapv();
                            }
                            7 => {
                                // RTR
                                self.exec_rtr();
                            }
                            _ => self.illegal_instruction(),
                        }
                    }
                    0x4E78 => self.illegal_instruction(),
                    _ => self.illegal_instruction(),
                }
            }
            2 => self.exec_jsr(),
            3 => self.exec_jmp(),
            _ => unreachable!(),
        }
    }

    // ================================================================
    // NOP
    // ================================================================
    // 0100 1110 0111 0001 = $4E71
    // 4 cycles

    fn exec_nop(&mut self) {
        // Nothing to do — just consumes 4 cycles (the FetchIRC for next instr)
    }

    // ================================================================
    // RESET
    // ================================================================
    // 0100 1110 0111 0000 = $4E70
    // 132 cycles (124 internal + 4 read + 4 read)

    fn exec_reset(&mut self) {
        if self.check_supervisor() { return; }
        // 132 total cycles: 128 internal + 4 for start_next_instruction FetchIRC.
        // The 124-cycle RESET line assertion plus 4 cycles of internal pipeline
        // recovery before the prefetch resumes.
        self.micro_ops.push(MicroOp::Internal(128));
    }

    // ================================================================
    // CLR  — set EA to zero with flags
    // ================================================================
    // Encoding: 0100 0010 SS MMMRRR
    // Timing: Dn=4(byte/word) 6(long), memory=8+EA (byte/word), 12+EA (long)
    // Note: CLR on 68000 does a read-then-write for memory, even though
    //       the read value is discarded. This matters for bus traces.

    fn exec_clr(&mut self) {
        let op = self.ir;
        let size_bits = ((op >> 6) & 3) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let size = Size::from_bits(size_bits).unwrap();

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = size;

        match ea {
            AddrMode::DataReg(r) => {
                // Set flags: N=0, Z=1, V=0, C=0 (X unchanged)
                self.regs.sr = (self.regs.sr & 0xFFF0) | 0x0004;
                self.write_data_reg(r, 0, size);
                if size == Size::Long {
                    self.micro_ops.push(MicroOp::Internal(2));
                }
            }
            _ => {
                // Memory: RMW (read then write zero).
                // Flags are set AFTER the read completes (in misc_rmw_alu),
                // not here — if the read triggers AE, the pushed SR must
                // have the pre-instruction flags, not the CLR result flags.
                self.addr2 = unary_op_code(UnaryOp::Clr) as u32;
                self.resolve_unary_ea_rmw(&ea, size);
            }
        }
    }

    // ================================================================
    // TST  — test an operand
    // ================================================================
    // Encoding: 0100 1010 SS MMMRRR
    // Timing: Dn=4, memory=4+EA
    // Sets N,Z; clears V,C. X unchanged.

    fn exec_tst(&mut self) {
        let op = self.ir;
        let size_bits = ((op >> 6) & 3) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let size = Size::from_bits(size_bits).unwrap();

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = size;

        match ea {
            AddrMode::DataReg(r) => {
                let val = self.read_data_reg(r, size);
                self.set_flags_move(val, size);
            }
            AddrMode::AddrReg(r) => {
                let val = self.regs.a(r as usize);
                self.set_flags_move(val, size);
            }
            // Memory: read, set flags, no writeback
            _ => {
                self.addr2 = unary_op_code(UnaryOp::Tst) as u32;
                self.resolve_unary_ea_read(&ea, size);
            }
        }
    }

    // ================================================================
    // NEG / NOT / NEGX (unary read-modify-write)
    // ================================================================

    fn exec_unary_rmw(&mut self, unary_op: UnaryOp) {
        let op = self.ir;
        let size_bits = ((op >> 6) & 3) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let size = Size::from_bits(size_bits).unwrap();

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = size;
        self.addr2 = unary_op_code(unary_op) as u32;

        match ea {
            AddrMode::DataReg(r) => {
                let val = self.read_data_reg(r, size);
                let (result, new_sr) = match unary_op {
                    UnaryOp::Neg => alu::neg(val, size, self.regs.sr),
                    UnaryOp::Negx => alu::negx(val, size, self.regs.sr),
                    UnaryOp::Not => {
                        let r = !val & size.mask();
                        let sr = self.set_flags_logic_sr(r, size);
                        (r, sr)
                    }
                    _ => unreachable!(),
                };
                self.regs.sr = new_sr;
                self.write_data_reg(r, result, size);
                if size == Size::Long {
                    self.micro_ops.push(MicroOp::Internal(2));
                }
            }
            _ => {
                self.resolve_unary_ea_rmw(&ea, size);
            }
        }
    }

    /// Resolve EA for unary read-modify-write (NEG, NOT, NEGX, CLR).
    fn resolve_unary_ea_rmw(&mut self, ea: &AddrMode, size: Size) {
        match ea {
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(*r as usize);
                let inc = if size == Size::Byte && *r == 7 { 2 } else { size.bytes() };
                self.regs.set_a(*r as usize, a.wrapping_add(inc));
                if size == Size::Long {
                    self.src_postinc_undo = Some((*r, inc));
                }
                self.addr = a;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if size == Size::Byte && *r == 7 { 2 } else { size.bytes() };
                let a = self.regs.a(*r as usize).wrapping_sub(dec);
                self.regs.set_a(*r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 91;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Resolve EA for unary read-only (TST).
    fn resolve_unary_ea_read(&mut self, ea: &AddrMode, size: Size) {
        match ea {
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(*r as usize);
                let inc = if size == Size::Byte && *r == 7 { 2 } else { size.bytes() };
                self.regs.set_a(*r as usize, a.wrapping_add(inc));
                if size == Size::Long {
                    self.src_postinc_undo = Some((*r, inc));
                }
                self.addr = a;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if size == Size::Byte && *r == 7 { 2 } else { size.bytes() };
                let a = self.regs.a(*r as usize).wrapping_sub(dec);
                self.regs.set_a(*r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 90;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 91;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 91: AbsLong second address word for unary operations.
    fn misc_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(self.size);
        self.followup_tag = 90;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 90: Read complete, perform unary ALU + writeback.
    fn misc_rmw_alu(&mut self) {
        let size = self.size;
        let val = self.data;
        let unary_op = unary_op_from_code(self.addr2 as u8);

        self.in_followup = false;
        self.followup_tag = 0;

        match unary_op {
            UnaryOp::Tst => {
                self.set_flags_move(val, size);
                // No writeback
            }
            UnaryOp::Clr => {
                // Set flags here (after read) so AE during read preserves old SR.
                // N=0, Z=1, V=0, C=0 (X unchanged)
                self.regs.sr = (self.regs.sr & 0xFFF0) | 0x0004;
                self.data = 0;
                self.queue_write_ops(size);
            }
            UnaryOp::Neg => {
                let (result, new_sr) = alu::neg(val, size, self.regs.sr);
                self.regs.sr = new_sr;
                self.data = result;
                self.queue_write_ops(size);
            }
            UnaryOp::Negx => {
                let (result, new_sr) = alu::negx(val, size, self.regs.sr);
                self.regs.sr = new_sr;
                self.data = result;
                self.queue_write_ops(size);
            }
            UnaryOp::Not => {
                let result = !val & size.mask();
                let new_sr = self.set_flags_logic_sr(result, size);
                self.regs.sr = new_sr;
                self.data = result;
                self.queue_write_ops(size);
            }
            UnaryOp::Nbcd => {
                let x = self.x_flag();
                let (result, carry, overflow) = self.nbcd(val as u8, x);
                self.set_nbcd_flags(result, carry, overflow);
                self.data = u32::from(result);
                self.queue_write_ops(size);
            }
        }
    }

    /// Compute logic flags (N,Z,V=0,C=0) and return new SR. Preserves X.
    fn set_flags_logic_sr(&self, result: u32, size: Size) -> u16 {
        use crate::flags::{C, N, V, Z};
        let mask = size.mask();
        let msb = size.msb_mask();
        let r = result & mask;
        let mut sr = self.regs.sr & !(C | V | Z | N);
        if r == 0 { sr |= Z; }
        if r & msb != 0 { sr |= N; }
        sr
    }

    // ================================================================
    // SWAP
    // ================================================================
    // 0100 1000 0100 0RRR
    // Swap halves of Dn. 4 cycles.

    fn exec_swap(&mut self) {
        let reg = (self.ir & 7) as u8;
        let val = self.regs.d[reg as usize];
        let result = (val >> 16) | (val << 16);
        self.regs.d[reg as usize] = result;
        self.set_flags_move(result, Size::Long);
    }

    // ================================================================
    // EXT
    // ================================================================
    // EXT.w: 0100 1000 1000 0RRR — sign extend byte to word
    // EXT.l: 0100 1000 1100 0RRR — sign extend word to long
    // 4 cycles.

    fn exec_ext(&mut self, size: Size) {
        let reg = (self.ir & 7) as usize;
        let val = self.regs.d[reg];
        let result = match size {
            Size::Word => {
                // Sign extend byte to word, keep upper 16 bits
                let ext = (val as u8 as i8 as i16) as u16;
                (val & 0xFFFF0000) | u32::from(ext)
            }
            Size::Long => {
                // Sign extend word to long
                (val as u16 as i16 as i32) as u32
            }
            _ => unreachable!(),
        };
        self.regs.d[reg] = result;
        self.set_flags_move(result, size);
    }

    // ================================================================
    // MOVE from SR
    // ================================================================
    // 0100 0000 11 MMMRRR
    // Move SR to EA. On 68000 this is NOT privileged (unlike 68010+).
    // Timing: Dn=6, memory=8+EA

    fn exec_move_from_sr(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = Size::Word;

        match ea {
            AddrMode::DataReg(r) => {
                self.write_data_reg(r, u32::from(self.regs.sr), Size::Word);
                self.micro_ops.push(MicroOp::Internal(2));
            }
            _ => {
                // Memory: the 68000 does a read-modify-write for MOVEfromSR,
                // same as CLR. The read is a dummy (value discarded), then
                // the SR value is written. This adds 4 cycles to the timing.
                // data2 stores the SR value, followup tag 139 writes it.
                self.data2 = u32::from(self.regs.sr);
                self.resolve_move_from_sr_ea(&ea);
            }
        }
    }

    /// Resolve EA for MOVEfromSR memory (read-modify-write pattern).
    /// Queues ReadWord (dummy) + Execute(tag 139 → write SR value).
    fn resolve_move_from_sr_ea(&mut self, ea: &AddrMode) {
        let size = Size::Word;
        match ea {
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(*r as usize);
                let inc = size.bytes();
                self.regs.set_a(*r as usize, a.wrapping_add(inc));
                self.addr = a;
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = size.bytes();
                let a = self.regs.a(*r as usize).wrapping_sub(dec);
                self.regs.set_a(*r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
            }
            AddrMode::AbsLong => {
                // Need two ext words. Store SR in addr2, address hi in data2.
                // addr2 is a bit overloaded here but tag 138 handles it.
                self.addr2 = self.data2; // Stash SR value
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 138;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            _ => { self.illegal_instruction(); return; }
        }
        // Queue dummy read + write via tag 139
        self.micro_ops.push(MicroOp::ReadWord);
        self.in_followup = true;
        self.followup_tag = 139;
        self.micro_ops.push(MicroOp::Execute);
    }

    // ================================================================
    // JMP
    // ================================================================
    // 0100 1110 11 MMMRRR
    // Jump to effective address.
    // Timing: (An)=8, d16(An)=10, d8(An,Xn)=14, xxx.W=10, xxx.L=12,
    //         d16(PC)=10, d8(PC,Xn)=14

    fn exec_jmp(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = true;

        // For branch/jump instructions, IRC consumption for displacement/address
        // uses consume_irc_deferred() to avoid queuing a wasted FetchIRC at the
        // old PC position (the branch invalidates the pipeline). An Internal(2)
        // accounts for the EA computation time. Exception: d8(An,Xn) and
        // d8(PC,Xn) use consume_irc() because the FetchIRC timing is needed
        // for the longer index EA calculation.
        let target = match ea {
            AddrMode::AddrInd(r) => self.regs.a(r as usize),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc_deferred() as i16;
                let t = (self.regs.a(r as usize) as i32).wrapping_add(i32::from(disp)) as u32;
                self.micro_ops.push(MicroOp::Internal(2));
                t
            }
            AddrMode::AddrIndIndex(r) => {
                // Use deferred: branch invalidates pipeline, FetchIRC at old PC is wasted.
                // Add Internal(6) to account for ext word consumption + index calc time.
                let ext = self.consume_irc_deferred();
                let t = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(6));
                t
            }
            AddrMode::AbsShort => {
                let t = self.consume_irc_deferred() as i16 as i32 as u32;
                self.micro_ops.push(MicroOp::Internal(2));
                t
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 92;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc_deferred() as i16;
                let t = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.micro_ops.push(MicroOp::Internal(2));
                t
            }
            AddrMode::PcIndex => {
                // Use deferred: branch invalidates pipeline, same as AddrIndIndex.
                let base = self.irc_addr;
                let ext = self.consume_irc_deferred();
                let t = self.calc_index_ea(base, ext);
                self.micro_ops.push(MicroOp::Internal(6));
                t
            }
            _ => { self.illegal_instruction(); return; }
        };

        self.jump_to(target);
    }

    // ================================================================
    // JSR
    // ================================================================
    // 0100 1110 10 MMMRRR
    // Jump to subroutine: push return PC, then jump.
    // Timing: (An)=16, d16(An)=18, d8(An,Xn)=22, xxx.W=18, xxx.L=20,
    //         d16(PC)=18, d8(PC,Xn)=22

    fn exec_jsr(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = true;

        // Same consume_irc_deferred() pattern as JMP — avoid wasted FetchIRC
        // at the old PC position for displacement/address consumption before
        // a branch. d8(An,Xn) and d8(PC,Xn) keep consume_irc() for timing.
        let (target, return_pc) = match ea {
            AddrMode::AddrInd(r) => {
                (self.regs.a(r as usize), self.instr_start_pc.wrapping_add(2))
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc_deferred() as i16;
                let t = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.micro_ops.push(MicroOp::Internal(2));
                (t, self.instr_start_pc.wrapping_add(4))
            }
            AddrMode::AddrIndIndex(r) => {
                // Use deferred: branch invalidates pipeline, FetchIRC at old PC is wasted.
                let ext = self.consume_irc_deferred();
                let t = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(6));
                (t, self.instr_start_pc.wrapping_add(4))
            }
            AddrMode::AbsShort => {
                let t = self.consume_irc_deferred() as i16 as i32 as u32;
                self.micro_ops.push(MicroOp::Internal(2));
                (t, self.instr_start_pc.wrapping_add(4))
            }
            AddrMode::AbsLong => {
                // Two ext words for address. data=return_pc, data2=addr_hi.
                // First word uses consume_irc() (needs FetchIRC to refill IRC
                // with the second address word). Second word in tag 92 uses
                // consume_irc_deferred (branch invalidates pipeline).
                self.data = self.instr_start_pc.wrapping_add(6);
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 92;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc_deferred() as i16;
                let t = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.micro_ops.push(MicroOp::Internal(2));
                (t, self.instr_start_pc.wrapping_add(4))
            }
            AddrMode::PcIndex => {
                // Use deferred: branch invalidates pipeline, same as AddrIndIndex.
                let base = self.irc_addr;
                let ext = self.consume_irc_deferred();
                let t = self.calc_index_ea(base, ext);
                self.micro_ops.push(MicroOp::Internal(6));
                (t, self.instr_start_pc.wrapping_add(4))
            }
            _ => { self.illegal_instruction(); return; }
        };

        self.push_and_jump(return_pc, target);
    }

    /// Tag 92: AbsLong second word for JMP/JSR.
    fn jsr_push_pc(&mut self) {
        // Use consume_irc_deferred: the branch invalidates the pipeline,
        // so the FetchIRC refill would read a wasted word at the old PC.
        let lo = self.consume_irc_deferred();
        let target = self.data2 | u32::from(lo);

        // Is this JMP or JSR? Check bit 7 of the opcode.
        // JMP = 0100 1110 11 = bits 7-6 = 11
        // JSR = 0100 1110 10 = bits 7-6 = 10
        if self.ir & 0x0040 != 0 {
            // JMP AbsLong
            self.in_followup = false;
            self.followup_tag = 0;
            self.jump_to(target);
        } else {
            // JSR AbsLong: self.data = return_pc (stashed earlier)
            let return_pc = self.data;
            self.in_followup = false;
            self.followup_tag = 0;
            self.push_and_jump(return_pc, target);
        }
    }

    /// Jump to a target address: set PC and refill prefetch.
    fn jump_to(&mut self, target: u32) {
        self.regs.pc = target;
        self.in_followup = false;
        self.followup_tag = 0;
        self.refill_prefetch_branch();
    }

    /// Push return PC and jump to target.
    ///
    /// The real 68000 prefetches at the target BEFORE pushing the return PC
    /// to the stack. This matches the YACHT timing "np nS ns" and is critical
    /// for address error behavior: if the target is odd, the FetchIRC triggers
    /// AE before any push happens, so no push undo is needed.
    fn push_and_jump(&mut self, return_pc: u32, target: u32) {
        self.data = return_pc;
        self.regs.pc = target;
        // Prefetch at target first, then push return PC
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
        // After PushLongLo, queue is empty → start_next_instruction:
        // IR ← IRC (first word at target), FetchIRC reads target+2
    }

    // ================================================================
    // RTS
    // ================================================================
    // 0100 1110 0111 0101 = $4E75
    // Pop return address from stack, jump there.
    // 16 cycles: ReadHi(4) + ReadLo(4) + FetchIRC-target(4) + FetchIRC-next(4)

    fn exec_rts(&mut self) {
        // Read high word from stack
        self.addr = self.regs.a(7);
        self.micro_ops.push(MicroOp::ReadWord);
        self.in_followup = true;
        self.followup_tag = 93;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 93: RTS high word read, now read low word.
    fn rts_read_hi(&mut self) {
        let hi = self.data;
        self.data2 = hi << 16;
        self.regs.set_a(7, self.regs.a(7).wrapping_add(2));
        self.addr = self.regs.a(7);
        self.micro_ops.push(MicroOp::ReadWord);
        self.followup_tag = 94;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 94: RTS low word read, jump to return address.
    fn rts_jump(&mut self) {
        let target = self.data2 | (self.data & 0xFFFF);
        self.regs.set_a(7, self.regs.a(7).wrapping_add(2));
        self.regs.pc = target;
        self.irc_addr = target;
        self.in_followup = false;
        self.followup_tag = 0;
        self.refill_prefetch_branch();
    }

    // ================================================================
    // RTE
    // ================================================================
    // 0100 1110 0111 0011 = $4E73
    // Pop SR, then pop return address, jump there.
    // 20 cycles: ReadSR(4) + ReadPChi(4) + ReadPClo(4) + FetchIRC(4) + FetchIRC(4)
    // Privileged instruction.

    fn exec_rte(&mut self) {
        if self.check_supervisor() { return; }
        // Save SSP for frame reading — SR restore may switch to user mode,
        // but we need to keep reading from the supervisor stack.
        self.addr2 = self.regs.ssp;
        self.addr = self.addr2;
        self.micro_ops.push(MicroOp::ReadWord);
        self.in_followup = true;
        self.followup_tag = 95;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 95: RTE SR read, now read PC high.
    fn rte_read_pc_hi(&mut self) {
        // Restore SR — this may change S bit (supervisor → user), switching
        // which register A7 maps to. But we must keep reading from SSP.
        let new_sr = (self.data as u16) & crate::flags::SR_MASK;
        self.addr2 = self.addr2.wrapping_add(2);
        self.regs.sr = new_sr;
        // Update SSP directly (not via A7 which may now be USP)
        self.regs.ssp = self.addr2;
        self.addr = self.addr2;
        self.micro_ops.push(MicroOp::ReadWord);
        self.followup_tag = 96;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 96: RTE PC high read, now read PC low.
    fn rte_read_pc_lo(&mut self) {
        self.data2 = self.data << 16;
        self.addr2 = self.addr2.wrapping_add(2);
        self.regs.ssp = self.addr2;
        self.addr = self.addr2;
        self.micro_ops.push(MicroOp::ReadWord);
        self.followup_tag = 97;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 97: RTE PC low read, jump.
    fn rte_jump(&mut self) {
        let target = self.data2 | (self.data & 0xFFFF);
        self.addr2 = self.addr2.wrapping_add(2);
        self.regs.ssp = self.addr2;
        self.regs.pc = target;
        self.irc_addr = target;
        self.in_followup = false;
        self.followup_tag = 0;
        self.refill_prefetch_branch();
    }

    // ================================================================
    // NBCD
    // ================================================================
    // 0100 1000 00 MMMRRR
    // Negate BCD with extend: 0 - dst - X
    // Timing: Dn=6, memory=8+EA (same as NEG)

    fn exec_nbcd(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = Size::Byte;
        self.addr2 = unary_op_code(UnaryOp::Nbcd) as u32;

        match ea {
            AddrMode::DataReg(r) => {
                let val = (self.regs.d[r as usize] & 0xFF) as u8;
                let x = self.x_flag();
                let (result, carry, overflow) = self.nbcd(val, x);
                self.regs.d[r as usize] =
                    (self.regs.d[r as usize] & 0xFFFFFF00) | u32::from(result);
                self.set_nbcd_flags(result, carry, overflow);
                self.micro_ops.push(MicroOp::Internal(2));
            }
            _ => {
                // Memory: RMW pattern — read, negate, write
                self.resolve_unary_ea_rmw(&ea, Size::Byte);
            }
        }
    }

    fn set_nbcd_flags(&mut self, result: u8, carry: bool, overflow: bool) {
        use crate::flags::{C, V, X, Z};
        // X and C: set to borrow
        self.regs.sr = if carry {
            self.regs.sr | X | C
        } else {
            self.regs.sr & !(X | C)
        };
        // Z: only cleared, never set
        if result != 0 {
            self.regs.sr &= !Z;
        }
        // N: undefined, but set from MSB
        self.regs.sr = if result & 0x80 != 0 {
            self.regs.sr | 0x0008
        } else {
            self.regs.sr & !0x0008
        };
        // V: "undefined" per spec but real hardware sets it
        self.regs.sr = if overflow {
            self.regs.sr | V
        } else {
            self.regs.sr & !V
        };
    }

    // ================================================================
    // TRAP
    // ================================================================
    // 0100 1110 0100 VVVV = $4E40-$4E4F
    // TRAP #vector: exception with vector 32+V
    // Timing: 34 cycles
    // (Routing is in exec_group4_misc_control)

    // ================================================================
    // TRAPV
    // ================================================================
    // 0100 1110 0111 0110 = $4E76
    // If V flag set, trap vector 7. Otherwise NOP.
    // Timing: 4 (no trap), 34 (trap)

    fn exec_trapv(&mut self) {
        use crate::flags::V;
        if self.regs.sr & V != 0 {
            self.exception_pc_override =
                Some(self.instr_start_pc.wrapping_add(2));
            self.exception(7, 0);
        }
        // V clear: 4 cycles (just the next FetchIRC)
    }

    // ================================================================
    // STOP
    // ================================================================
    // 0100 1110 0111 0010 = $4E72
    // Load immediate into SR, then halt. Privileged.
    // Timing: 4 (privilege violation) or immediate halt

    fn exec_stop(&mut self) {
        if self.regs.sr & 0x2000 == 0 {
            // Not supervisor: privilege violation
            self.exception(8, 0);
            return;
        }
        // Read immediate directly from IRC — STOP doesn't refill the pipeline.
        // The CPU halts immediately after loading SR.
        let imm = self.irc;
        self.regs.sr = imm & crate::flags::SR_MASK;
        self.state = crate::cpu::State::Stopped;
        // Internal(4) idle before halting
        self.micro_ops.push(MicroOp::Internal(4));
    }

    // ================================================================
    // TAS — Test And Set
    // ================================================================
    // 0100 1010 11 MMMRRR
    // Read byte, set N/Z/clear V/C, set bit 7, write back.
    // Register: 4 cycles. Memory: read-modify-write (RMW bus cycle).
    // Followup tags: 136 = RMW complete, 137 = AbsLong ext2

    fn exec_tas(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;

        match ea {
            AddrMode::DataReg(r) => {
                let val = (self.regs.d[r as usize] & 0xFF) as u8;
                self.set_flags_move(u32::from(val), Size::Byte);
                self.regs.d[r as usize] =
                    (self.regs.d[r as usize] & 0xFFFFFF00) | u32::from(val | 0x80);
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 136;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                let inc = if r == 7 { 2 } else { 1 };
                self.regs.set_a(r as usize, a.wrapping_add(inc));
                self.addr = a;
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 136;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if r == 7 { 2 } else { 1 };
                let a = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 136;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 136;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 136;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 136;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 137;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 136: TAS RMW complete — set flags and write back with bit 7 set.
    ///
    /// The 68000 TAS instruction uses an indivisible read-modify-write bus cycle
    /// that takes 10 cycles (not 8 = 4 read + 4 write). The extra 2 cycles are
    /// internal processing between read and write phases.
    fn tas_rmw_complete(&mut self) {
        let val = (self.data & 0xFF) as u8;
        self.set_flags_move(u32::from(val), Size::Byte);
        self.data = u32::from(val | 0x80);
        self.in_followup = false;
        self.followup_tag = 0;
        // TAS RMW bus cycle: 2 extra internal cycles between read and write
        self.micro_ops.push(MicroOp::Internal(2));
        self.queue_write_ops(Size::Byte);
    }

    /// Tag 137: TAS AbsLong second address word.
    fn tas_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(Size::Byte);
        self.followup_tag = 136;
        self.micro_ops.push(MicroOp::Execute);
    }

    // ================================================================
    // MOVE from SR AbsLong — Tag 138, RMW write — Tag 139
    // ================================================================

    /// Tag 138: MOVE from SR AbsLong second address word.
    fn move_from_sr_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.data2 = self.addr2; // Restore SR value from stash
        // Dummy read + tag 139 write (same RMW pattern as other modes)
        self.micro_ops.push(MicroOp::ReadWord);
        self.followup_tag = 139;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 139: MOVE from SR RMW write — dummy read done, now write SR value.
    fn move_from_sr_rmw_write(&mut self) {
        self.data = self.data2;
        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(Size::Word);
    }

    // ================================================================
    // CHK — Check Register Against Bounds
    // ================================================================
    // 0100 RRR 110 MMMRRR
    // Compare Dn.w against EA.w: if Dn < 0 or Dn > EA, trap vector 6
    // N flag: set if Dn < 0, cleared if Dn > source
    // Timing: no trap = 10, trap = 40+EA
    // Followup tags: 134 = EA read complete, 135 = AbsLong ext2

    fn exec_chk(&mut self) {
        let op = self.ir;
        let dn = ((op >> 9) & 7) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = Size::Word;
        self.addr2 = dn as u32; // stash Dn register number

        match ea {
            AddrMode::DataReg(r) => {
                self.data = self.regs.d[r as usize] & 0xFFFF;
                self.chk_ea_complete();
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                self.regs.set_a(r as usize, a.wrapping_add(2));
                self.addr = a;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let a = self.regs.a(r as usize).wrapping_sub(2);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                // CHK: predecrement is NOT undone on read AE (unlike MOVE).
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                // Stash Dn in high bits of some field — we'll use addr2 lower byte
                // (addr2 was already set to dn)
                self.in_followup = true;
                self.followup_tag = 135;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::Immediate => {
                let imm = u32::from(self.consume_irc());
                self.data = imm;
                // Use followup so the FetchIRC from consume_irc runs before
                // the exception fires. Without this, exception() clears the
                // queue and the FetchIRC is lost (4 missing cycles).
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcIndex => {
                let base = self.irc_addr;
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(base, ext);
                self.program_space_access = true;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 134;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 134: CHK EA read complete — check bounds.
    fn chk_ea_complete(&mut self) {
        let dn_idx = self.addr2 as usize;
        let dn_val = (self.regs.d[dn_idx] & 0xFFFF) as u16;
        let bound = (self.data & 0xFFFF) as u16;

        self.in_followup = false;
        self.followup_tag = 0;

        let dn_signed = dn_val as i16;
        let bound_signed = bound as i16;

        // Frame PC = past opcode + extension words consumed before EA read.
        let frame_pc = self.instr_start_pc
            .wrapping_add(2 + u32::from(self.irc_consumed_count) * 2);

        // The real 68000 always computes Dn.w - src.w internally.
        // When Dn<0 triggers the trap, the ALU takes 2 extra internal cycles
        // if the subtraction result is negative or zero without overflow —
        // i.e., Dn <= src (signed, no overflow). The fast path (no extra)
        // fires when Dn > src (positive result, no overflow) or when the
        // subtraction overflows (V=1).
        let sub_result = dn_val.wrapping_sub(bound);
        let sub_n = sub_result & 0x8000 != 0;
        let sub_z = sub_result == 0;
        let sub_v = ((dn_val ^ bound) & (dn_val ^ sub_result)) & 0x8000 != 0;

        if dn_signed < 0 {
            // Dn < 0: N=1, clear ZVC, preserve X, trap
            // Extra 2 cycles when subtraction shows Dn<=src without overflow:
            // (N=1 OR Z=1) AND V=0
            let extra = if (sub_n || sub_z) && !sub_v { 2 } else { 0 };
            self.regs.sr = (self.regs.sr & 0xFFF0) | 0x0008; // N=1, ZVC=0
            self.exception_pc_override = Some(frame_pc);
            self.exception(6, extra);
        } else if dn_signed > bound_signed {
            // Dn > upper bound: N=0, clear ZVC, preserve X, trap
            self.regs.sr &= 0xFFF0; // N=0, ZVC=0
            self.exception_pc_override = Some(frame_pc);
            self.exception(6, 0);
        } else {
            // In bounds: no trap. The 68000 clears NZVC (X preserved).
            // Motorola documents these as "undefined" but DL tests show
            // the hardware consistently clears them.
            self.regs.sr &= 0xFFF0;
            self.micro_ops.push(MicroOp::Internal(6));
        }
    }

    /// Tag 135: CHK AbsLong second address word.
    fn chk_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(Size::Word);
        self.followup_tag = 134;
        self.micro_ops.push(MicroOp::Execute);
    }

    // ================================================================
    // LINK
    // ================================================================
    // 0100 1110 0101 0 RRR = $4E50-$4E57
    // LINK An,#d16: Push An, An <- SP, SP += d16 (signed)
    // 16 cycles: PushHi(4) + PushLo(4) + FetchDisp(4) + FetchIRC(4)
    // Tag 120: displacement consumed, update SP

    fn exec_link(&mut self) {
        let an = (self.ir & 7) as usize;
        let an_val = self.regs.a(an);

        // Push An onto stack
        self.data = an_val;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // After push, An gets SP value, then add signed displacement
        self.in_followup = true;
        self.followup_tag = 120;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 120: LINK push complete. Set An=SP, consume displacement, adjust SP.
    fn link_write_disp(&mut self) {
        let an = (self.ir & 7) as usize;
        let disp = self.consume_irc() as i16;

        // An <- current SP (after push)
        self.regs.set_a(an, self.regs.a(7));
        // SP += signed displacement (usually negative)
        let sp = (self.regs.a(7) as i32).wrapping_add(i32::from(disp)) as u32;
        self.regs.set_a(7, sp);

        self.in_followup = false;
        self.followup_tag = 0;
    }

    // ================================================================
    // UNLK
    // ================================================================
    // 0100 1110 0101 1 RRR = $4E58-$4E5F
    // UNLK An: SP <- An, An <- (SP)+
    // 12 cycles: ReadHi(4) + ReadLo(4) + FetchIRC(4)

    fn exec_unlk(&mut self) {
        let an = (self.ir & 7) as usize;

        // Save original SP for AE undo (UNLK sets A7 ← An, and if the
        // subsequent stack read faults, the A7 modification must be rolled back).
        let was_supervisor = self.regs.is_supervisor();
        let original_sp = self.regs.a(7);
        self.sp_undo = Some((was_supervisor, original_sp));

        // SP <- An
        self.regs.set_a(7, self.regs.a(an));

        // Read high word of saved An from stack
        self.addr = self.regs.a(7);
        self.micro_ops.push(MicroOp::ReadWord);
        self.in_followup = true;
        self.followup_tag = 127;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 127: UNLK high word read. Now read low word.
    fn unlk_read_hi(&mut self) {
        self.data2 = self.data << 16;
        self.regs.set_a(7, self.regs.a(7).wrapping_add(2));
        self.addr = self.regs.a(7);
        self.micro_ops.push(MicroOp::ReadWord);
        self.followup_tag = 128;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 128: UNLK low word read. Set An to popped value.
    fn unlk_read_lo(&mut self) {
        let an = (self.ir & 7) as usize;
        let value = self.data2 | (self.data & 0xFFFF);
        self.regs.set_a(7, self.regs.a(7).wrapping_add(2));
        self.regs.set_a(an, value);
        self.in_followup = false;
        self.followup_tag = 0;
        self.sp_undo = None; // UNLK completed successfully, no undo needed
    }

    // ================================================================
    // MOVE USP
    // ================================================================
    // 0100 1110 0110 D RRR
    // D=0: An -> USP, D=1: USP -> An
    // 4 cycles. Privileged.

    fn exec_move_usp(&mut self) {
        if self.check_supervisor() { return; }
        let op = self.ir;
        let reg = (op & 7) as usize;
        let dir = op & 0x0008; // bit 3: 0=An->USP, 1=USP->An

        if dir != 0 {
            // USP -> An
            self.regs.set_a(reg, self.regs.usp);
        } else {
            // An -> USP
            self.regs.usp = self.regs.a(reg);
        }
    }

    // ================================================================
    // PEA
    // ================================================================
    // 0100 1000 01 MMMRRR
    // Push effective address onto stack.
    // Timing: (An)=12, d16(An)=16, d8(An,Xn)=20, xxx.W=16, xxx.L=20,
    //         d16(PC)=16, d8(PC,Xn)=20
    // Tag 121: AbsLong ext2

    fn exec_pea(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = true;

        let addr = match ea {
            AddrMode::AddrInd(r) => self.regs.a(r as usize),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                (self.regs.a(r as usize) as i32).wrapping_add(i32::from(disp)) as u32
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                let a = self.calc_index_ea(self.regs.a(r as usize), ext);
                // Index EA needs 4 internal cycles (2 more than d16 base)
                self.micro_ops.push(MicroOp::Internal(4));
                a
            }
            AddrMode::AbsShort => {
                self.consume_irc() as i16 as i32 as u32
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 121;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                (base as i32).wrapping_add(i32::from(disp)) as u32
            }
            AddrMode::PcIndex => {
                let base = self.irc_addr;
                let ext = self.consume_irc();
                let a = self.calc_index_ea(base, ext);
                // Index EA needs 4 internal cycles (2 more than d16 base)
                self.micro_ops.push(MicroOp::Internal(4));
                a
            }
            _ => { self.illegal_instruction(); return; }
        };

        self.data = addr;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
    }

    /// Tag 121: PEA AbsLong second word.
    fn pea_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        let addr = self.data2 | u32::from(lo);
        self.data = addr;
        self.in_followup = false;
        self.followup_tag = 0;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
    }

    // ================================================================
    // MOVE to CCR / MOVE to SR
    // ================================================================
    // MOVE to CCR: 0100 0100 11 MMMRRR — word EA, only low byte affects CCR
    //   Timing: Dn=12, mem=12+EA
    // MOVE to SR: 0100 0110 11 MMMRRR — word EA, full SR (privileged)
    //   Timing: Dn=12, mem=12+EA
    //
    // Tags: 122 = read complete, apply to CCR/SR
    //        123 = AbsLong ext2

    fn exec_move_to_ccr(&mut self) {
        self.addr2 = 0; // 0 = CCR
        self.resolve_move_to_ccr_sr_ea();
    }

    fn exec_move_to_sr(&mut self) {
        if self.check_supervisor() { return; }
        self.addr2 = 1; // 1 = SR
        self.resolve_move_to_ccr_sr_ea();
    }

    fn resolve_move_to_ccr_sr_ea(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;
        self.size = Size::Word;

        match ea {
            AddrMode::DataReg(r) => {
                self.data = self.read_data_reg(r, Size::Word);
                self.move_to_ccr_sr_complete();
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                self.regs.set_a(r as usize, a.wrapping_add(2));
                self.addr = a;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let a = self.regs.a(r as usize).wrapping_sub(2);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 123;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcIndex => {
                let base = self.irc_addr;
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(base, ext);
                self.program_space_access = true;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 122;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::Immediate => {
                self.data = u32::from(self.consume_irc());
                self.move_to_ccr_sr_complete();
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 123: MOVE to CCR/SR AbsLong ext2.
    fn move_to_ccr_sr_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(Size::Word);
        self.followup_tag = 122;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 122: MOVE to CCR/SR read complete.
    fn move_to_ccr_sr_complete(&mut self) {
        let val = self.data as u16;
        let is_sr = self.addr2 == 1;

        self.in_followup = false;
        self.followup_tag = 0;

        if is_sr {
            self.regs.sr = val & crate::flags::SR_MASK;
        } else {
            // CCR: only bits 0-4 (XNZVC)
            self.regs.sr = (self.regs.sr & 0xFF00) | (val & 0x001F);
        }

        // 12 cycles total for Dn: Internal(8) covers it
        // (Execute(0) + Internal(8) + FetchIRC(4) = 12)
        self.micro_ops.push(MicroOp::Internal(8));
    }

    // ================================================================
    // RTR
    // ================================================================
    // 0100 1110 0111 0111 = $4E77
    // Pop CCR, then pop return address, jump there.
    // 20 cycles: ReadCCR(4) + ReadPChi(4) + ReadPClo(4) + FetchIRC(4) + FetchIRC(4)
    //
    // Tags: 124=read PC hi, 125=read PC lo, 126=jump

    fn exec_rtr(&mut self) {
        // Read CCR from stack (SP)
        let sp = self.regs.a(7);
        self.addr = sp;
        self.micro_ops.push(MicroOp::ReadWord);
        self.in_followup = true;
        self.followup_tag = 124;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 124: RTR CCR read, now read PC high.
    fn rtr_read_pc_hi(&mut self) {
        // Apply to CCR only (bits 0-4 of SR)
        let ccr = self.data as u16;
        self.regs.sr = (self.regs.sr & 0xFFE0) | (ccr & 0x001F);
        // Read PC high from SP+2
        self.addr = self.addr.wrapping_add(2);
        self.micro_ops.push(MicroOp::ReadWord);
        self.followup_tag = 125;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 125: RTR PC high read, now read PC low.
    fn rtr_read_pc_lo(&mut self) {
        self.data2 = self.data << 16;
        // Read PC low from SP+4
        self.addr = self.addr.wrapping_add(2);
        self.micro_ops.push(MicroOp::ReadWord);
        self.followup_tag = 126;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 126: RTR PC low read, jump.
    fn rtr_jump(&mut self) {
        let target = self.data2 | (self.data & 0xFFFF);
        // Advance SP by 6 (CCR word + PC long)
        self.regs.set_a(7, self.regs.a(7).wrapping_add(6));
        self.regs.pc = target;
        self.irc_addr = target;
        self.in_followup = false;
        self.followup_tag = 0;
        self.refill_prefetch_branch();
    }

    // ================================================================
    // MOVEM — Move Multiple Registers
    // ================================================================
    // Register-to-memory: 0100 1000 1S MMMRRR (sub=0b1000, size_bits=2/3)
    // Memory-to-register: 0100 1100 1S MMMRRR (sub=0b1100, size_bits=2/3)
    // Extension word = register mask (16 bits)
    //
    // Register mask bits:
    //   For reg-to-mem (normal):  D0-D7 in bits 0-7, A0-A7 in bits 8-15
    //   For reg-to-mem predec:    A7-A0 in bits 0-7, D7-D0 in bits 8-15 (reversed!)
    //   For mem-to-reg:           D0-D7 in bits 0-7, A0-A7 in bits 8-15
    //
    // Timing: reg-to-mem: 8+4n (word) or 8+8n (long)
    //         mem-to-reg: 12+4n (word) or 12+8n (long)
    //
    // Tags: 130 = reg-to-mem AbsLong ext2
    //       131 = reg-to-mem transfer loop
    //       132 = mem-to-reg AbsLong ext2
    //       133 = mem-to-reg transfer loop

    fn exec_movem_reg_to_mem(&mut self, size: Size) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        // MOVEM reg-to-mem doesn't support all modes
        match ea {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) |
            AddrMode::AddrIndPostInc(_) | AddrMode::PcDisp | AddrMode::PcIndex |
            AddrMode::Immediate => {
                self.illegal_instruction();
                return;
            }
            _ => {}
        }

        self.program_space_access = false;
        self.size = size;

        // Consume register mask from IRC — queues FetchIRC to refill IRC.
        // For modes needing EA extension words, we MUST wait for FetchIRC
        // before consuming those words (staged via tag 140).
        let mask = self.consume_irc();
        self.data2 = u32::from(mask);

        let is_predec = matches!(ea, AddrMode::AddrIndPreDec(_));

        match ea {
            // Modes with no EA extension words: addr from registers, start now
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.start_movem_reg_to_mem_transfer(is_predec);
            }
            AddrMode::AddrIndPreDec(r) => {
                self.addr = self.regs.a(r as usize);
                self.addr2 = r as u32;
                self.start_movem_reg_to_mem_transfer(is_predec);
            }
            // Modes with EA extension words: wait for FetchIRC, then resolve EA
            _ => {
                self.in_followup = true;
                self.followup_tag = 140;
                self.micro_ops.push(MicroOp::Execute);
            }
        }
    }

    /// Tag 140: MOVEM reg-to-mem EA resolution (after mask FetchIRC completes).
    fn movem_reg_to_mem_resolve_ea(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let ea = AddrMode::decode(ea_mode, ea_reg).expect("valid MOVEM EA mode");

        match ea {
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.start_movem_reg_to_mem_transfer(false);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.start_movem_reg_to_mem_transfer(false);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.start_movem_reg_to_mem_transfer(false);
            }
            AddrMode::AbsLong => {
                // Need two more words: consume addr_hi now, addr_lo in tag 130
                self.data = u32::from(self.consume_irc()) << 16;
                self.followup_tag = 130;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => unreachable!(),
        }
    }

    /// Tag 130: MOVEM reg-to-mem AbsLong second address word.
    fn movem_reg_to_mem_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data | u32::from(lo);
        self.start_movem_reg_to_mem_transfer(false);
    }

    fn start_movem_reg_to_mem_transfer(&mut self, is_predec: bool) {
        let mask = self.data2 as u16;

        // If mask is zero, we're done immediately
        if mask == 0 {
            self.in_followup = false;
            self.followup_tag = 0;
            return;
        }

        // Store whether we're in predec mode in the high bit of data2
        // (mask is only 16 bits, so we have room)
        if is_predec {
            self.data2 |= 0x10000;
        }

        self.in_followup = true;
        self.followup_tag = 131;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 131: MOVEM reg-to-mem transfer one register.
    fn movem_reg_to_mem_transfer(&mut self) {
        let mask = (self.data2 & 0xFFFF) as u16;
        let is_predec = (self.data2 & 0x10000) != 0;
        let needs_advance = (self.data2 & 0x20000) != 0;
        let size = self.size;

        // Advance address from previous write (normal mode only).
        // We defer this because self.addr must not change while write ops
        // are still in the queue — they read self.addr when they execute.
        if needs_advance {
            self.addr = self.addr.wrapping_add(size.bytes());
            self.data2 &= !0x20000;
        }

        if mask == 0 {
            // All transfers complete
            if is_predec {
                let an = self.addr2 as usize;
                self.regs.set_a(an, self.addr);
            }
            self.in_followup = false;
            self.followup_tag = 0;
            return;
        }

        // Find next register to transfer
        let reg_index = mask.trailing_zeros() as usize;

        // Get register value
        let reg_val = if is_predec {
            // Reversed mapping: bit 0 = A7, bit 7 = A0, bit 8 = D7, bit 15 = D0
            if reg_index < 8 {
                self.regs.a(7 - reg_index)
            } else {
                self.regs.d[15 - reg_index]
            }
        } else {
            // Normal mapping: bit 0 = D0, bit 7 = D7, bit 8 = A0, bit 15 = A7
            if reg_index < 8 {
                self.regs.d[reg_index]
            } else {
                self.regs.a(reg_index - 8)
            }
        };

        // For predecrement, decrement address before write
        if is_predec {
            self.addr = self.addr.wrapping_sub(size.bytes());
            // Long predecrement: the real 68000 decrements by 2 first, writes
            // the low word, then decrements by 2 more. Our code decrements by 4
            // at once. Signal the AE checker to adjust the fault address by +2.
            if size == Size::Long {
                self.predec_long_read = true;
            }
        }

        // Write register to memory
        self.data = reg_val;
        self.queue_write_ops(size);

        // Clear this bit from mask
        let new_mask = mask & !(1 << reg_index);
        self.data2 = (self.data2 & 0xFFFF0000) | u32::from(new_mask);

        // For normal modes, mark deferred advance for next Execute
        if !is_predec {
            self.data2 |= 0x20000;
        }

        // Continue with next register
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_movem_mem_to_reg(&mut self, size: Size) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        // MOVEM mem-to-reg doesn't support some modes
        match ea {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) |
            AddrMode::AddrIndPreDec(_) | AddrMode::Immediate => {
                self.illegal_instruction();
                return;
            }
            _ => {}
        }

        self.program_space_access = false;
        self.size = size;

        // Consume register mask from IRC — queues FetchIRC.
        // For modes needing EA extension words, wait for FetchIRC via tag 141.
        let mask = self.consume_irc();
        self.data2 = u32::from(mask);

        let postinc_reg = if let AddrMode::AddrIndPostInc(r) = ea { Some(r) } else { None };

        match ea {
            // Modes with no EA extension words: addr from registers, start now
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.start_movem_mem_to_reg_transfer(postinc_reg);
            }
            AddrMode::AddrIndPostInc(r) => {
                self.addr = self.regs.a(r as usize);
                self.start_movem_mem_to_reg_transfer(postinc_reg);
            }
            // Modes with EA extension words: wait for FetchIRC, then resolve EA
            _ => {
                self.in_followup = true;
                self.followup_tag = 141;
                self.micro_ops.push(MicroOp::Execute);
            }
        }
    }

    /// Tag 141: MOVEM mem-to-reg EA resolution (after mask FetchIRC completes).
    fn movem_mem_to_reg_resolve_ea(&mut self) {
        let op = self.ir;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let ea = AddrMode::decode(ea_mode, ea_reg).expect("valid MOVEM EA mode");

        match ea {
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.start_movem_mem_to_reg_transfer(None);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.start_movem_mem_to_reg_transfer(None);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.start_movem_mem_to_reg_transfer(None);
            }
            AddrMode::AbsLong => {
                self.data = u32::from(self.consume_irc()) << 16;
                self.followup_tag = 132;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                self.start_movem_mem_to_reg_transfer(None);
            }
            AddrMode::PcIndex => {
                let base = self.irc_addr;
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(base, ext);
                self.program_space_access = true;
                self.micro_ops.push(MicroOp::Internal(2));
                self.start_movem_mem_to_reg_transfer(None);
            }
            _ => unreachable!(),
        }
    }

    /// Tag 132: MOVEM mem-to-reg AbsLong second address word.
    fn movem_mem_to_reg_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data | u32::from(lo);
        self.start_movem_mem_to_reg_transfer(None);
    }

    fn start_movem_mem_to_reg_transfer(&mut self, postinc_reg: Option<u8>) {
        let mask = self.data2 as u16;

        // If mask is zero, we're done immediately
        if mask == 0 {
            self.in_followup = false;
            self.followup_tag = 0;
            return;
        }

        // Store postinc register in bits 24-31 (if any), otherwise 0xFF
        let postinc_val = postinc_reg.unwrap_or(0xFF);
        self.data2 = (self.data2 & 0x00FFFFFF) | (u32::from(postinc_val) << 24);

        // Mark that we haven't read anything yet (bits 16-23 = 0xFF)
        self.data2 = (self.data2 & 0xFF00FFFF) | 0x00FF0000;

        self.in_followup = true;
        self.followup_tag = 133;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 133: MOVEM mem-to-reg transfer one register.
    fn movem_mem_to_reg_transfer(&mut self) {
        let mask = (self.data2 & 0xFFFF) as u16;
        let size = self.size;
        let pending_reg = ((self.data2 >> 16) & 0xFF) as usize;

        // If pending_reg != 0xFF, we just completed a read
        if pending_reg != 0xFF {
            // Write value to register
            let value = if size == Size::Word {
                // Sign-extend word to long
                (self.data as u16 as i16 as i32) as u32
            } else {
                self.data
            };

            if pending_reg < 8 {
                self.regs.d[pending_reg] = value;
            } else {
                self.regs.set_a(pending_reg - 8, value);
            }

            // Advance address past the data we just read.
            // Deferred from queue_read_ops — self.addr must be stable
            // while read micro-ops are in the queue.
            self.addr = self.addr.wrapping_add(size.bytes());

            // Mark no pending read
            self.data2 = (self.data2 & 0xFF00FFFF) | 0x00FF0000;
        }

        // Check if more registers to transfer
        if mask == 0 {
            // Update postinc register — points past the last transferred data.
            let postinc_reg = ((self.data2 >> 24) & 0xFF) as u8;
            if postinc_reg < 8 {
                self.regs.set_a(postinc_reg as usize, self.addr);
            }
            // The 68000 does a dummy word read from the next address.
            self.micro_ops.push(MicroOp::ReadWord);
            self.in_followup = false;
            self.followup_tag = 0;
            return;
        }

        // Find next register to transfer (normal order: D0-D7, A0-A7)
        let reg_index = mask.trailing_zeros() as usize;

        // Clear this bit from mask
        let new_mask = mask & !(1 << reg_index);
        self.data2 = (self.data2 & 0xFFFF0000) | u32::from(new_mask);

        // Mark this register as pending read
        self.data2 = (self.data2 & 0xFF00FFFF) | ((reg_index as u32) << 16);

        // Queue read from current address
        self.queue_read_ops(size);

        // Continue after read completes
        self.micro_ops.push(MicroOp::Execute);
    }
}

#[derive(Debug, Clone, Copy)]
enum UnaryOp {
    Neg,
    Negx,
    Not,
    Clr,
    Tst,
    Nbcd,
}

fn unary_op_code(op: UnaryOp) -> u8 {
    match op {
        UnaryOp::Neg => 0,
        UnaryOp::Negx => 1,
        UnaryOp::Not => 2,
        UnaryOp::Clr => 3,
        UnaryOp::Tst => 4,
        UnaryOp::Nbcd => 5,
    }
}

fn unary_op_from_code(code: u8) -> UnaryOp {
    match code {
        0 => UnaryOp::Neg,
        1 => UnaryOp::Negx,
        2 => UnaryOp::Not,
        3 => UnaryOp::Clr,
        4 => UnaryOp::Tst,
        5 => UnaryOp::Nbcd,
        _ => UnaryOp::Neg, // shouldn't happen
    }
}
