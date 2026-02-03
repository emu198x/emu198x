//! Instruction decode and execution for the 68000.
//!
//! The 68000 instruction set is decoded primarily by the top 4 bits of the
//! opcode word, with further decoding based on other bit fields.

#![allow(clippy::too_many_lines)] // Decode functions are intentionally verbose.
#![allow(clippy::single_match_else)] // Match arms will expand with more cases.
#![allow(clippy::if_not_else)] // Privilege check reads more naturally this way.
#![allow(clippy::verbose_bit_mask)] // Explicit bit operations for clarity.
#![allow(clippy::doc_markdown)] // Instruction mnemonics don't need backticks.
#![allow(clippy::manual_swap)] // Will use swap when register access is simpler.
#![allow(clippy::manual_rotate)] // SWAP is conceptually a register swap, not rotation.
#![allow(clippy::manual_range_patterns)] // Explicit values for instruction decode clarity.

use crate::cpu::{AddrMode, M68000, Size};
use crate::flags::{Status, C, X};
use crate::microcode::MicroOp;

impl M68000 {
    /// Decode and execute the current instruction.
    pub(super) fn decode_and_execute(&mut self) {
        use crate::cpu::InstrPhase;

        // If we're in a follow-up phase, continue the current instruction
        if self.instr_phase != InstrPhase::Initial {
            self.continue_instruction();
            return;
        }

        // Extract common fields from opcode
        let op = self.opcode;

        // Top 4 bits determine instruction group
        match op >> 12 {
            0x0 => self.decode_group_0(op),
            0x1 => self.decode_move_byte(op),
            0x2 => self.decode_move_long(op),
            0x3 => self.decode_move_word(op),
            0x4 => self.decode_group_4(op),
            0x5 => self.decode_group_5(op),
            0x6 => self.decode_group_6(op),
            0x7 => self.decode_moveq(op),
            0x8 => self.decode_group_8(op),
            0x9 => self.decode_sub(op),
            0xA => self.decode_line_a(op),
            0xB => self.decode_group_b(op),
            0xC => self.decode_group_c(op),
            0xD => self.decode_add(op),
            0xE => self.decode_shift_rotate(op),
            0xF => self.decode_line_f(op),
            _ => unreachable!(),
        }
    }

    /// Continue executing an instruction that's in a follow-up phase.
    fn continue_instruction(&mut self) {
        let op = self.opcode;

        // Re-dispatch to the same instruction handler
        match op >> 12 {
            0x1 => self.decode_move_byte(op),
            0x2 => self.decode_move_long(op),
            0x3 => self.decode_move_word(op),
            // Add other multi-phase instructions here as needed
            _ => {
                // Instruction doesn't support phases, reset
                self.instr_phase = crate::cpu::InstrPhase::Initial;
            }
        }
    }

    /// Group 0: Bit manipulation, MOVEP, Immediate
    fn decode_group_0(&mut self, op: u16) {
        if op & 0x0100 != 0 {
            // Bit operations with register
            let reg = ((op >> 9) & 7) as u8;
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;

            match (op >> 6) & 3 {
                0 => self.exec_btst_reg(reg, mode, ea_reg),
                1 => self.exec_bchg_reg(reg, mode, ea_reg),
                2 => self.exec_bclr_reg(reg, mode, ea_reg),
                3 => self.exec_bset_reg(reg, mode, ea_reg),
                _ => unreachable!(),
            }
        } else if op & 0x0800 != 0 {
            // Bit operations with immediate
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;

            match (op >> 6) & 3 {
                0 => self.exec_btst_imm(mode, ea_reg),
                1 => self.exec_bchg_imm(mode, ea_reg),
                2 => self.exec_bclr_imm(mode, ea_reg),
                3 => self.exec_bset_imm(mode, ea_reg),
                _ => unreachable!(),
            }
        } else {
            // Immediate operations
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;

            match (op >> 9) & 7 {
                0 => self.exec_ori(size, mode, ea_reg),
                1 => self.exec_andi(size, mode, ea_reg),
                2 => self.exec_subi(size, mode, ea_reg),
                3 => self.exec_addi(size, mode, ea_reg),
                4 => self.exec_btst_imm(mode, ea_reg), // Actually BTST #imm
                5 => self.exec_eori(size, mode, ea_reg),
                6 => self.exec_cmpi(size, mode, ea_reg),
                7 => self.illegal_instruction(),
                _ => unreachable!(),
            }
        }
    }

    /// MOVE.B instruction
    fn decode_move_byte(&mut self, op: u16) {
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;

        self.exec_move(Size::Byte, src_mode, src_reg, dst_mode, dst_reg);
    }

    /// MOVE.L instruction
    fn decode_move_long(&mut self, op: u16) {
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;

        self.exec_move(Size::Long, src_mode, src_reg, dst_mode, dst_reg);
    }

    /// MOVE.W instruction
    fn decode_move_word(&mut self, op: u16) {
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;

        self.exec_move(Size::Word, src_mode, src_reg, dst_mode, dst_reg);
    }

    /// Group 4: Miscellaneous
    fn decode_group_4(&mut self, op: u16) {
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if op & 0x0100 != 0 {
            // CHK, LEA
            if (op >> 6) & 7 == 7 {
                let reg = ((op >> 9) & 7) as u8;
                self.exec_lea(reg, mode, ea_reg);
            } else {
                // CHK
                self.exec_chk(op);
            }
        } else {
            match (op >> 8) & 0xF {
                0x0 => {
                    // NEGX, MOVE from SR
                    if (op >> 6) & 3 == 3 {
                        self.exec_move_from_sr(mode, ea_reg);
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_negx(size, mode, ea_reg);
                    }
                }
                0x2 => {
                    // CLR
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_clr(size, mode, ea_reg);
                }
                0x4 => {
                    // NEG, MOVE to CCR
                    if (op >> 6) & 3 == 3 {
                        self.exec_move_to_ccr(mode, ea_reg);
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_neg(size, mode, ea_reg);
                    }
                }
                0x6 => {
                    // NOT, MOVE to SR
                    if (op >> 6) & 3 == 3 {
                        self.exec_move_to_sr(mode, ea_reg);
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_not(size, mode, ea_reg);
                    }
                }
                0x8 => {
                    // NBCD, SWAP, PEA, EXT, MOVEM
                    match (op >> 6) & 3 {
                        0 => self.exec_nbcd(mode, ea_reg),
                        1 => {
                            if mode == 0 {
                                self.exec_swap(ea_reg);
                            } else {
                                self.exec_pea(mode, ea_reg);
                            }
                        }
                        2 | 3 => {
                            if mode == 0 {
                                let size = if (op >> 6) & 1 == 0 {
                                    Size::Word
                                } else {
                                    Size::Long
                                };
                                self.exec_ext(size, ea_reg);
                            } else {
                                self.exec_movem_to_mem(op);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                0xA => {
                    // TST, TAS, ILLEGAL
                    if (op >> 6) & 3 == 3 {
                        if op == 0x4AFC {
                            self.illegal_instruction();
                        } else {
                            self.exec_tas(mode, ea_reg);
                        }
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_tst(size, mode, ea_reg);
                    }
                }
                0xC => {
                    // MOVEM from memory
                    self.exec_movem_from_mem(op);
                }
                0xE => {
                    // Trap, LINK, UNLK, MOVE USP, etc.
                    match (op >> 4) & 0xF {
                        0x4 => self.exec_trap(op),
                        0x5 => {
                            if op & 8 != 0 {
                                self.exec_unlk((op & 7) as u8);
                            } else {
                                self.exec_link((op & 7) as u8);
                            }
                        }
                        0x6 => self.exec_move_usp(op),
                        0x7 => match op & 0xF {
                            0 => self.exec_reset(),
                            1 => self.exec_nop(),
                            2 => self.exec_stop(),
                            3 => self.exec_rte(),
                            5 => self.exec_rts(),
                            6 => self.exec_trapv(),
                            7 => self.exec_rtr(),
                            _ => self.illegal_instruction(),
                        },
                        _ => self.illegal_instruction(),
                    }
                }
                _ => {
                    // JSR, JMP
                    if (op >> 6) & 3 == 2 {
                        self.exec_jsr(mode, ea_reg);
                    } else if (op >> 6) & 3 == 3 {
                        self.exec_jmp(mode, ea_reg);
                    } else {
                        self.illegal_instruction();
                    }
                }
            }
        }
    }

    /// Group 5: ADDQ, SUBQ, Scc, DBcc
    fn decode_group_5(&mut self, op: u16) {
        let data = ((op >> 9) & 7) as u8;
        let data = if data == 0 { 8 } else { data }; // 0 encodes as 8

        if (op >> 6) & 3 == 3 {
            // Scc, DBcc
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;
            let condition = ((op >> 8) & 0xF) as u8;

            if mode == 1 {
                // DBcc
                self.exec_dbcc(condition, ea_reg);
            } else {
                // Scc
                self.exec_scc(condition, mode, ea_reg);
            }
        } else {
            // ADDQ, SUBQ
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;

            if op & 0x0100 != 0 {
                self.exec_subq(size, data, mode, ea_reg);
            } else {
                self.exec_addq(size, data, mode, ea_reg);
            }
        }
    }

    /// Group 6: Bcc, BSR, BRA
    fn decode_group_6(&mut self, op: u16) {
        let condition = ((op >> 8) & 0xF) as u8;
        let displacement = (op & 0xFF) as i8;

        match condition {
            0 => self.exec_bra(displacement),  // BRA
            1 => self.exec_bsr(displacement),  // BSR
            _ => self.exec_bcc(condition, displacement),
        }
    }

    /// MOVEQ instruction
    fn decode_moveq(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let data = (op & 0xFF) as i8;
        self.exec_moveq(reg, data);
    }

    /// Group 8: OR, DIV, SBCD
    fn decode_group_8(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // DIVU, DIVS
            if op & 0x0100 != 0 {
                self.exec_divs(reg, mode, ea_reg);
            } else {
                self.exec_divu(reg, mode, ea_reg);
            }
        } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            // SBCD
            self.exec_sbcd(op);
        } else {
            // OR
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_or(size, reg, mode, ea_reg, op & 0x0100 != 0);
        }
    }

    /// SUB instruction
    fn decode_sub(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // SUBA
            let size = if op & 0x0100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            self.exec_suba(size, reg, mode, ea_reg);
        } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            // SUBX
            self.exec_subx(op);
        } else {
            // SUB
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_sub(size, reg, mode, ea_reg, op & 0x0100 != 0);
        }
    }

    /// Line A: Unimplemented (used for OS calls on some systems)
    fn decode_line_a(&mut self, _op: u16) {
        self.exception(10); // Line A exception
    }

    /// Group B: CMP, EOR
    fn decode_group_b(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // CMPA
            let size = if op & 0x0100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            self.exec_cmpa(size, reg, mode, ea_reg);
        } else if op & 0x0100 != 0 {
            if mode == 1 {
                // CMPM
                self.exec_cmpm(op);
            } else {
                // EOR
                let size = Size::from_bits(((op >> 6) & 3) as u8);
                self.exec_eor(size, reg, mode, ea_reg);
            }
        } else {
            // CMP
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_cmp(size, reg, mode, ea_reg);
        }
    }

    /// Group C: AND, MUL, ABCD, EXG
    fn decode_group_c(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // MULU, MULS
            if op & 0x0100 != 0 {
                self.exec_muls(reg, mode, ea_reg);
            } else {
                self.exec_mulu(reg, mode, ea_reg);
            }
        } else if op & 0x0100 != 0 {
            // Check opmode field (bits 3-7)
            let opmode = (op >> 3) & 0x1F;
            match opmode {
                0x08 => {
                    // EXG Dx,Dy: opmode = 01000
                    self.exec_exg(op);
                }
                0x09 => {
                    // EXG Ax,Ay: opmode = 01001
                    self.exec_exg(op);
                }
                0x11 => {
                    // EXG Dx,Ay: opmode = 10001
                    self.exec_exg(op);
                }
                0x00 | 0x01 => {
                    // ABCD: opmode = 0000x (bit 3 is R/M)
                    self.exec_abcd(op);
                }
                _ => {
                    // AND Dn,<ea>
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_and(size, reg, mode, ea_reg, true);
                }
            }
        } else {
            // AND <ea>,Dn
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_and(size, reg, mode, ea_reg, false);
        }
    }

    /// ADD instruction
    fn decode_add(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // ADDA
            let size = if op & 0x0100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            self.exec_adda(size, reg, mode, ea_reg);
        } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            // ADDX
            self.exec_addx(op);
        } else {
            // ADD
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_add(size, reg, mode, ea_reg, op & 0x0100 != 0);
        }
    }

    /// Shift and rotate instructions
    fn decode_shift_rotate(&mut self, op: u16) {
        let count_or_reg = ((op >> 9) & 7) as u8;
        let reg = (op & 7) as u8;
        let size = Size::from_bits(((op >> 6) & 3) as u8);

        if (op >> 6) & 3 == 3 {
            // Memory shift/rotate (always word size, shift by 1)
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;
            let direction = (op >> 8) & 1 != 0; // 0=right, 1=left
            let kind = ((op >> 9) & 3) as u8;
            self.exec_shift_mem(kind, direction, mode, ea_reg);
        } else {
            // Register shift/rotate
            let direction = (op >> 8) & 1 != 0;
            let immediate = (op >> 5) & 1 == 0; // 0=count in opcode, 1=count in register
            let kind = ((op >> 3) & 3) as u8;
            self.exec_shift_reg(kind, direction, count_or_reg, reg, size, immediate);
        }
    }

    /// Line F: Unimplemented (used for coprocessor on 68020+)
    fn decode_line_f(&mut self, _op: u16) {
        self.exception(11); // Line F exception
    }

    /// Trigger illegal instruction exception.
    fn illegal_instruction(&mut self) {
        self.exception(4); // Illegal instruction vector
    }

    // === Instruction implementations ===

    fn exec_move(&mut self, size: Size, src_mode: u8, src_reg: u8, dst_mode: u8, dst_reg: u8) {
        use crate::cpu::InstrPhase;

        let src = AddrMode::decode(src_mode, src_reg);
        let dst = AddrMode::decode(dst_mode, dst_reg);

        let Some(src_mode) = src else {
            self.illegal_instruction();
            return;
        };
        let Some(dst_mode) = dst else {
            self.illegal_instruction();
            return;
        };

        // Check for MOVEA (destination is address register)
        let is_movea = matches!(dst_mode, AddrMode::AddrReg(_));

        match self.instr_phase {
            InstrPhase::Initial => {
                // Setup
                self.src_mode = Some(src_mode);
                self.dst_mode = Some(dst_mode);
                self.size = size;
                self.ext_idx = 0;

                // Calculate how many extension words we need for source
                let src_ext = self.ext_words_for_mode(src_mode);
                let dst_ext = self.ext_words_for_mode(dst_mode);

                if src_ext + dst_ext == 0 {
                    // No extension words needed - direct register operations
                    self.exec_move_direct(src_mode, dst_mode, is_movea);
                } else {
                    // Queue fetching all extension words
                    for _ in 0..src_ext {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    for _ in 0..dst_ext {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            InstrPhase::SrcEACalc => {
                // Extension words are fetched, calculate source EA
                let src_mode = self.src_mode.expect("src_mode should be set");
                let dst_mode = self.dst_mode.expect("dst_mode should be set");

                // PC value at start of extension words (after opcode)
                let pc_at_ext = self.regs.pc.wrapping_sub(
                    2 * u32::from(self.ext_count)
                );

                // Handle source
                match src_mode {
                    AddrMode::DataReg(r) => {
                        self.data = self.read_data_reg(r, self.size);
                        self.instr_phase = InstrPhase::DstEACalc;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(r) => {
                        self.data = self.regs.a(r as usize);
                        self.instr_phase = InstrPhase::DstEACalc;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::Immediate => {
                        self.data = self.read_immediate();
                        self.instr_phase = InstrPhase::DstEACalc;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        // Memory source - calculate EA and queue read
                        let (addr, _is_reg) = self.calc_ea(src_mode, pc_at_ext);
                        self.addr = addr;
                        self.queue_read_ops(self.size);
                        self.instr_phase = InstrPhase::SrcRead;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }

                // For destination, check if it needs EA calc
                if !matches!(dst_mode, AddrMode::DataReg(_) | AddrMode::AddrReg(_)) {
                    // Calculate dest EA now (using remaining ext words)
                    let pc_for_dst = pc_at_ext.wrapping_add(
                        2 * u32::from(self.ext_words_for_mode(src_mode))
                    );
                    let (addr, _) = self.calc_ea(dst_mode, pc_for_dst);
                    self.addr2 = addr;
                }
            }
            InstrPhase::SrcRead => {
                // Source data is now in self.data from memory read
                self.instr_phase = InstrPhase::DstEACalc;
                self.micro_ops.push(MicroOp::Execute);
            }
            InstrPhase::DstEACalc => {
                // Write to destination
                let dst_mode = self.dst_mode.expect("dst_mode should be set");
                let value = self.data;

                match dst_mode {
                    AddrMode::DataReg(r) => {
                        self.write_data_reg(r, value, self.size);
                        if !is_movea {
                            self.set_flags_move(value, self.size);
                        }
                        self.instr_phase = InstrPhase::Complete;
                    }
                    AddrMode::AddrReg(r) => {
                        // MOVEA - sign extend word to long, don't affect flags
                        let value = if self.size == Size::Word {
                            value as i16 as i32 as u32
                        } else {
                            value
                        };
                        self.regs.set_a(r as usize, value);
                        self.instr_phase = InstrPhase::Complete;
                    }
                    _ => {
                        // Memory destination
                        self.addr = self.addr2;
                        self.queue_write_ops(self.size);
                        if !is_movea {
                            self.set_flags_move(value, self.size);
                        }
                        self.instr_phase = InstrPhase::Complete;
                    }
                }
            }
            InstrPhase::DstWrite | InstrPhase::Complete => {
                // Done
            }
        }
    }

    /// Execute direct register-to-register MOVE (no extension words).
    fn exec_move_direct(&mut self, src_mode: AddrMode, dst_mode: AddrMode, is_movea: bool) {
        // Read source
        let value = match src_mode {
            AddrMode::DataReg(r) => self.read_data_reg(r, self.size),
            AddrMode::AddrReg(r) => self.regs.a(r as usize),
            AddrMode::AddrInd(r) => {
                // Simple indirect - queue memory read
                self.addr = self.regs.a(r as usize);
                self.dst_mode = Some(dst_mode);
                self.queue_read_ops(self.size);
                self.instr_phase = crate::cpu::InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::AddrIndPostInc(r) => {
                let addr = self.regs.a(r as usize);
                let inc = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                self.regs.set_a(r as usize, addr.wrapping_add(inc));
                self.addr = addr;
                self.dst_mode = Some(dst_mode);
                self.queue_read_ops(self.size);
                self.instr_phase = crate::cpu::InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                let addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, addr);
                self.addr = addr;
                self.dst_mode = Some(dst_mode);
                self.queue_read_ops(self.size);
                self.instr_phase = crate::cpu::InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            _ => {
                // Modes requiring extension words shouldn't reach here
                self.illegal_instruction();
                return;
            }
        };

        // Write destination
        match dst_mode {
            AddrMode::DataReg(r) => {
                self.write_data_reg(r, value, self.size);
                if !is_movea {
                    self.set_flags_move(value, self.size);
                }
            }
            AddrMode::AddrReg(r) => {
                let value = if self.size == Size::Word {
                    value as i16 as i32 as u32
                } else {
                    value
                };
                self.regs.set_a(r as usize, value);
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.data = value;
                self.queue_write_ops(self.size);
                if !is_movea {
                    self.set_flags_move(value, self.size);
                }
            }
            AddrMode::AddrIndPostInc(r) => {
                let addr = self.regs.a(r as usize);
                let inc = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                self.regs.set_a(r as usize, addr.wrapping_add(inc));
                self.addr = addr;
                self.data = value;
                self.queue_write_ops(self.size);
                if !is_movea {
                    self.set_flags_move(value, self.size);
                }
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                let addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, addr);
                self.addr = addr;
                self.data = value;
                self.queue_write_ops(self.size);
                if !is_movea {
                    self.set_flags_move(value, self.size);
                }
            }
            _ => {
                self.illegal_instruction();
            }
        }
    }

    fn exec_moveq(&mut self, reg: u8, data: i8) {
        let value = data as i32 as u32; // Sign extend to 32 bits
        self.regs.d[reg as usize] = value;
        self.set_flags_move(value, Size::Long);
        self.queue_internal(4);
    }

    fn exec_lea(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // LEA calculates effective address and loads it into An
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    self.regs.set_a(reg as usize, self.regs.a(r as usize));
                    self.queue_internal(4);
                }
                AddrMode::AddrIndDisp(_r) => {
                    // Need extension word for displacement
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.dst_mode = Some(AddrMode::AddrReg(reg));
                }
                AddrMode::AbsShort => {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.dst_mode = Some(AddrMode::AddrReg(reg));
                }
                AddrMode::AbsLong => {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.dst_mode = Some(AddrMode::AddrReg(reg));
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_clr(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        if let Some(size) = size {
            if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
                if !addr_mode.is_data_alterable() {
                    self.illegal_instruction();
                    return;
                }

                match addr_mode {
                    AddrMode::DataReg(r) => {
                        self.regs.d[r as usize] = match size {
                            Size::Byte => self.regs.d[r as usize] & 0xFFFF_FF00,
                            Size::Word => self.regs.d[r as usize] & 0xFFFF_0000,
                            Size::Long => 0,
                        };
                        // CLR sets N=0, Z=1, V=0, C=0
                        self.regs.sr = Status::clear_vc(self.regs.sr);
                        self.regs.sr = Status::update_nz_byte(self.regs.sr, 0);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination - queue write
                        self.queue_ea_read(addr_mode, size);
                        self.data = 0;
                    }
                }
            } else {
                self.illegal_instruction();
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_nop(&mut self) {
        self.queue_internal(4);
    }

    fn exec_rts(&mut self) {
        // Pop PC from stack
        self.micro_ops.push(MicroOp::PopLongHi);
        self.micro_ops.push(MicroOp::PopLongLo);
        // PC will be set when pop completes
        self.queue_internal(4);
    }

    fn exec_bra(&mut self, displacement: i8) {
        if displacement == 0 {
            // Word displacement follows
            self.micro_ops.push(MicroOp::FetchExtWord);
        } else {
            // Byte displacement
            let offset = displacement as i32;
            self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
            self.queue_internal(4);
        }
    }

    fn exec_bsr(&mut self, displacement: i8) {
        // Push return address
        self.data = self.regs.pc;
        if displacement == 0 {
            // Word displacement - need to read it first and adjust
            self.micro_ops.push(MicroOp::FetchExtWord);
        }
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        if displacement != 0 {
            // Calculate branch target
            let offset = displacement as i32;
            self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
        }
        self.queue_internal(4);
    }

    fn exec_bcc(&mut self, condition: u8, displacement: i8) {
        if Status::condition(self.regs.sr, condition) {
            if displacement == 0 {
                // Word displacement
                self.micro_ops.push(MicroOp::FetchExtWord);
            } else {
                let offset = displacement as i32;
                self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
            }
            self.queue_internal(10); // Branch taken timing
        } else {
            if displacement == 0 {
                // Skip word displacement
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            self.queue_internal(8); // Branch not taken timing
        }
    }

    // Bit operation implementations
    fn exec_btst_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BTST Dn,<ea> - test bit, set Z if bit was 0
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let value = self.regs.d[r as usize];
                    let was_zero = (value >> bit) & 1 == 0;
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(6);
                }
                _ => {
                    // Memory operand - bit number mod 8, stub for now
                    self.queue_internal(4);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_bchg_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BCHG Dn,<ea> - test and change (toggle) bit
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let mask = 1u32 << bit;
                    let value = self.regs.d[r as usize];
                    let was_zero = value & mask == 0;
                    self.regs.d[r as usize] = value ^ mask; // Toggle bit
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(8);
                }
                _ => {
                    self.queue_internal(4); // Memory stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_bclr_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BCLR Dn,<ea> - test and clear bit
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let mask = 1u32 << bit;
                    let value = self.regs.d[r as usize];
                    let was_zero = value & mask == 0;
                    self.regs.d[r as usize] = value & !mask; // Clear bit
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(10);
                }
                _ => {
                    self.queue_internal(4); // Memory stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_bset_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BSET Dn,<ea> - test and set bit
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let mask = 1u32 << bit;
                    let value = self.regs.d[r as usize];
                    let was_zero = value & mask == 0;
                    self.regs.d[r as usize] = value | mask; // Set bit
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(8);
                }
                _ => {
                    self.queue_internal(4); // Memory stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_btst_imm(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_bchg_imm(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_bclr_imm(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_bset_imm(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_ori(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_andi(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_subi(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_addi(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_eori(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_cmpi(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_move_from_sr(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_negx(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_move_to_ccr(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_neg(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // NEG <ea> - negate (0 - destination)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    let result = 0u32.wrapping_sub(value);
                    self.write_data_reg(r, result, size);
                    // NEG flags: same as SUB 0 - src -> 0
                    self.set_flags_sub(value, 0, result, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory operand - stub
                    self.queue_internal(4);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_sr(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_not(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // NOT <ea> - ones complement
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    let result = !value;
                    self.write_data_reg(r, result, size);
                    self.set_flags_move(result, size); // NOT sets N,Z, clears V,C
                    self.queue_internal(4);
                }
                _ => {
                    // Memory destination - stub
                    self.queue_internal(4);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_nbcd(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_swap(&mut self, reg: u8) {
        let value = self.regs.d[reg as usize];
        let swapped = (value >> 16) | (value << 16);
        self.regs.d[reg as usize] = swapped;
        self.set_flags_move(swapped, Size::Long);
        self.queue_internal(4);
    }

    fn exec_pea(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_ext(&mut self, size: Size, reg: u8) {
        let value = match size {
            Size::Word => {
                // Extend byte to word
                let byte = self.regs.d[reg as usize] as i8 as i16 as u16;
                self.regs.d[reg as usize] =
                    (self.regs.d[reg as usize] & 0xFFFF_0000) | u32::from(byte);
                u32::from(byte)
            }
            Size::Long => {
                // Extend word to long
                let word = self.regs.d[reg as usize] as i16 as i32 as u32;
                self.regs.d[reg as usize] = word;
                word
            }
            Size::Byte => unreachable!(),
        };
        self.set_flags_move(value, size);
        self.queue_internal(4);
    }

    fn exec_movem_to_mem(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_tas(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_tst(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // TST <ea> - test operand (sets N and Z, clears V and C)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    self.set_flags_move(value, size);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    // TST.L An is valid (68020+), but test anyway
                    let value = self.regs.a(r as usize);
                    self.set_flags_move(value, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory operand - stub
                    self.queue_internal(4);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_movem_from_mem(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_trap(&mut self, op: u16) {
        let vector = 32 + (op & 0xF) as u8;
        self.exception(vector);
    }

    fn exec_link(&mut self, _reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_unlk(&mut self, _reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_move_usp(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_reset(&mut self) {
        // RESET asserts RESET signal for 124 clock periods
        // Requires supervisor mode
        if !self.regs.is_supervisor() {
            self.exception(8); // Privilege violation
        } else {
            self.queue_internal(132); // RESET timing
        }
    }

    fn exec_stop(&mut self) {
        // STOP #imm - requires supervisor mode
        if !self.regs.is_supervisor() {
            self.exception(8);
        } else {
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.state = crate::cpu::State::Stopped;
        }
    }

    fn exec_rte(&mut self) {
        // Return from exception - requires supervisor mode
        if !self.regs.is_supervisor() {
            self.exception(8);
        } else {
            // Pop SR, then PC
            self.micro_ops.push(MicroOp::PopWord);
            self.micro_ops.push(MicroOp::PopLongHi);
            self.micro_ops.push(MicroOp::PopLongLo);
            self.queue_internal(4);
        }
    }

    fn exec_trapv(&mut self) {
        if self.regs.sr & crate::flags::V != 0 {
            self.exception(7); // TRAPV exception
        } else {
            self.queue_internal(4);
        }
    }

    fn exec_rtr(&mut self) {
        // Pop CCR, then PC
        self.micro_ops.push(MicroOp::PopWord);
        self.micro_ops.push(MicroOp::PopLongHi);
        self.micro_ops.push(MicroOp::PopLongLo);
        self.queue_internal(4);
    }

    fn exec_jsr(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_jmp(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_chk(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_addq(&mut self, size: Option<Size>, data: u8, mode: u8, ea_reg: u8) {
        // ADDQ #data,<ea> - quick add (data = 1-8, where 0 encodes 8)
        let imm = if data == 0 { 8u32 } else { u32::from(data) };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrReg(r) => {
                    // Address register - always long operation, no flags affected
                    let val = self.regs.a(r as usize);
                    let result = val.wrapping_add(imm);
                    self.regs.set_a(r as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::DataReg(r) => {
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    let dst = self.read_data_reg(r, size);
                    let result = dst.wrapping_add(imm);
                    self.write_data_reg(r, result, size);
                    self.set_flags_add(imm, dst, result, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory destination - requires read-modify-write
                    let Some(_size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    self.queue_internal(4); // Stub for memory operations
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_subq(&mut self, size: Option<Size>, data: u8, mode: u8, ea_reg: u8) {
        // SUBQ #data,<ea> - quick subtract (data = 1-8, where 0 encodes 8)
        let imm = if data == 0 { 8u32 } else { u32::from(data) };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrReg(r) => {
                    // Address register - always long operation, no flags affected
                    let val = self.regs.a(r as usize);
                    let result = val.wrapping_sub(imm);
                    self.regs.set_a(r as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::DataReg(r) => {
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    let dst = self.read_data_reg(r, size);
                    let result = dst.wrapping_sub(imm);
                    self.write_data_reg(r, result, size);
                    self.set_flags_sub(imm, dst, result, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory destination - requires read-modify-write
                    let Some(_size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    self.queue_internal(4); // Stub for memory operations
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_scc(&mut self, _condition: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_dbcc(&mut self, condition: u8, reg: u8) {
        // DBcc Dn, label - Decrement and branch
        // If condition is true, no branch (fall through past displacement word)
        // If condition is false, decrement Dn.W and branch if Dn != -1
        // Note: DBcc ALWAYS has word displacement following the opcode

        // Remember PC before reading displacement (for branch calculation)
        let pc_before_disp = self.regs.pc;

        // Queue extension word fetch - this will advance PC by 2
        // We need to check condition after the word is fetched
        // For now, handle synchronously by reading what will be fetched

        if Status::condition(self.regs.sr, condition) {
            // Condition true - no branch, but we still skip displacement word
            self.regs.pc = self.regs.pc.wrapping_add(2);
            self.queue_internal(12); // Condition true, no loop
        } else {
            // Condition false - decrement and possibly branch
            let val = (self.regs.d[reg as usize] & 0xFFFF) as i16;
            let new_val = val.wrapping_sub(1);
            self.regs.d[reg as usize] =
                (self.regs.d[reg as usize] & 0xFFFF_0000) | (new_val as u16 as u32);

            if new_val == -1 {
                // Counter exhausted (-1) - no branch, skip displacement
                self.regs.pc = self.regs.pc.wrapping_add(2);
                self.queue_internal(14); // Loop terminated
            } else {
                // Counter not exhausted - branch taken
                // We need the displacement word. Store PC location and queue fetch.
                self.addr = pc_before_disp; // Remember where displacement is
                self.micro_ops.push(MicroOp::FetchExtWord);
                // The branch calculation will use the fetched displacement
                // For now, this is incomplete - we'd need continuation logic
                self.queue_internal(10); // Loop continues
            }
        }
    }

    fn exec_divu(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_divs(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_sbcd(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_or(&mut self, size: Option<Size>, reg: u8, mode: u8, ea_reg: u8, to_ea: bool) {
        // OR Dn,<ea> or OR <ea>,Dn
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if to_ea {
                // OR Dn,<ea> - destination is EA
                if let AddrMode::DataReg(r) = addr_mode {
                    let src = self.read_data_reg(reg, size);
                    let dst = self.read_data_reg(r, size);
                    let result = dst | src;
                    self.write_data_reg(r, result, size);
                    self.set_flags_move(result, size); // OR sets N,Z, clears V,C
                    self.queue_internal(4);
                } else {
                    // Memory destination - stub
                    self.queue_internal(4);
                }
            } else {
                // OR <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst | src;
                        self.write_data_reg(reg, result, size);
                        self.set_flags_move(result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory source - stub
                        self.queue_internal(4);
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_sub(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        to_ea: bool,
    ) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if to_ea {
                // SUB Dn,<ea> - destination is EA
                if let AddrMode::DataReg(r) = addr_mode {
                    let src = self.read_data_reg(reg, size);
                    let dst = self.read_data_reg(r, size);
                    let result = dst.wrapping_sub(src);
                    self.write_data_reg(r, result, size);
                    self.set_flags_sub(src, dst, result, size);
                    self.queue_internal(4);
                } else {
                    // Memory destination - more complex, stub for now
                    self.queue_internal(4);
                }
            } else {
                // SUB <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_sub(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::Immediate => {
                        // SUBI - immediate subtract
                        self.queue_internal(4); // Stub
                    }
                    _ => {
                        self.queue_internal(4); // Memory source stub
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_suba(&mut self, size: Size, reg: u8, mode: u8, ea_reg: u8) {
        // SUBA <ea>,An - subtract from address register (no flags affected)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.d[r as usize] as i16 as i32 as u32
                    } else {
                        self.regs.d[r as usize]
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.a(r as usize) as i16 as i32 as u32
                    } else {
                        self.regs.a(r as usize)
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                _ => {
                    self.queue_internal(4); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_subx(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_cmp(&mut self, size: Option<Size>, reg: u8, mode: u8, ea_reg: u8) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        // CMP <ea>,Dn - compare (dst - src, set flags only)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.read_data_reg(r, size);
                    let dst = self.read_data_reg(reg, size);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, size);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = self.regs.a(r as usize);
                    let dst = self.read_data_reg(reg, size);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, size);
                    self.queue_internal(4);
                }
                AddrMode::Immediate => {
                    // CMPI - immediate compare (stub)
                    self.queue_internal(4);
                }
                _ => {
                    self.queue_internal(4); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_cmpa(&mut self, size: Size, reg: u8, mode: u8, ea_reg: u8) {
        // CMPA <ea>,An - compare to address register
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.d[r as usize] as i16 as i32 as u32
                    } else {
                        self.regs.d[r as usize]
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, Size::Long);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.a(r as usize) as i16 as i32 as u32
                    } else {
                        self.regs.a(r as usize)
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, Size::Long);
                    self.queue_internal(4);
                }
                _ => {
                    self.queue_internal(4); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_cmpm(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_eor(&mut self, size: Option<Size>, reg: u8, mode: u8, ea_reg: u8) {
        // EOR Dn,<ea> - exclusive OR (source is always data register)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.read_data_reg(reg, size);
                    let dst = self.read_data_reg(r, size);
                    let result = dst ^ src;
                    self.write_data_reg(r, result, size);
                    self.set_flags_move(result, size); // EOR sets N,Z, clears V,C
                    self.queue_internal(4);
                }
                _ => {
                    // Memory destination - stub
                    self.queue_internal(4);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_mulu(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // MULU <ea>,Dn - unsigned 16x16->32 multiply
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.regs.d[r as usize] & 0xFFFF;
                    let dst = self.regs.d[reg as usize] & 0xFFFF;
                    let result = src * dst;
                    self.regs.d[reg as usize] = result;

                    // Set flags: N based on bit 31, Z if result is 0, V=0, C=0
                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                    // Timing: 38 + 2*number of 1-bits in source (simplified to ~70 cycles)
                    self.queue_internal(70);
                }
                _ => {
                    self.queue_internal(70); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_muls(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // MULS <ea>,Dn - signed 16x16->32 multiply
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = (self.regs.d[r as usize] as i16) as i32;
                    let dst = (self.regs.d[reg as usize] as i16) as i32;
                    let result = (src * dst) as u32;
                    self.regs.d[reg as usize] = result;

                    // Set flags: N based on bit 31, Z if result is 0, V=0, C=0
                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                    // Timing: approximately 70 cycles (varies with operand)
                    self.queue_internal(70);
                }
                _ => {
                    self.queue_internal(70); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_abcd(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_exg(&mut self, op: u16) {
        let rx = ((op >> 9) & 7) as usize;
        let ry = (op & 7) as usize;
        let mode = (op >> 3) & 0x1F;

        match mode {
            0x08 => {
                // Exchange data registers
                let tmp = self.regs.d[rx];
                self.regs.d[rx] = self.regs.d[ry];
                self.regs.d[ry] = tmp;
            }
            0x09 => {
                // Exchange address registers
                let tmp = self.regs.a(rx);
                self.regs.set_a(rx, self.regs.a(ry));
                self.regs.set_a(ry, tmp);
            }
            0x11 => {
                // Exchange data and address registers
                let tmp = self.regs.d[rx];
                self.regs.d[rx] = self.regs.a(ry);
                self.regs.set_a(ry, tmp);
            }
            _ => self.illegal_instruction(),
        }
        self.queue_internal(6);
    }

    fn exec_and(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        to_ea: bool,
    ) {
        // AND Dn,<ea> or AND <ea>,Dn
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if to_ea {
                // AND Dn,<ea> - destination is EA
                if let AddrMode::DataReg(r) = addr_mode {
                    let src = self.read_data_reg(reg, size);
                    let dst = self.read_data_reg(r, size);
                    let result = dst & src;
                    self.write_data_reg(r, result, size);
                    self.set_flags_move(result, size); // AND sets N,Z, clears V,C
                    self.queue_internal(4);
                } else {
                    // Memory destination - stub
                    self.queue_internal(4);
                }
            } else {
                // AND <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst & src;
                        self.write_data_reg(reg, result, size);
                        self.set_flags_move(result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory source - stub
                        self.queue_internal(4);
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_add(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        to_ea: bool,
    ) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if to_ea {
                // ADD Dn,<ea> - destination is EA
                if let AddrMode::DataReg(r) = addr_mode {
                    let src = self.read_data_reg(reg, size);
                    let dst = self.read_data_reg(r, size);
                    let result = dst.wrapping_add(src);
                    self.write_data_reg(r, result, size);
                    self.set_flags_add(src, dst, result, size);
                    self.queue_internal(4);
                } else {
                    // Memory destination - stub for now
                    self.queue_internal(4);
                }
            } else {
                // ADD <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_add(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::Immediate => {
                        // ADDI - immediate add (stub)
                        self.queue_internal(4);
                    }
                    _ => {
                        self.queue_internal(4); // Memory source stub
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_adda(&mut self, size: Size, reg: u8, mode: u8, ea_reg: u8) {
        // ADDA <ea>,An - add to address register (no flags affected)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.d[r as usize] as i16 as i32 as u32
                    } else {
                        self.regs.d[r as usize]
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_add(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.a(r as usize) as i16 as i32 as u32
                    } else {
                        self.regs.a(r as usize)
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_add(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                _ => {
                    self.queue_internal(4); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_addx(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_shift_mem(&mut self, _kind: u8, _direction: bool, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_shift_reg(
        &mut self,
        kind: u8,
        direction: bool, // false=right, true=left
        count_or_reg: u8,
        reg: u8,
        size: Option<Size>,
        immediate: bool, // true=count in opcode, false=count in register
    ) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        // Get shift count (1-8 for immediate, or from register mod 64)
        let count = if immediate {
            if count_or_reg == 0 { 8 } else { count_or_reg as u32 }
        } else {
            self.regs.d[count_or_reg as usize] % 64
        };

        let value = self.read_data_reg(reg, size);
        let (mask, msb_bit) = match size {
            Size::Byte => (0xFF_u32, 0x80_u32),
            Size::Word => (0xFFFF, 0x8000),
            Size::Long => (0xFFFF_FFFF, 0x8000_0000),
        };

        let (result, carry) = match (kind, direction) {
            // ASL - Arithmetic shift left
            (0, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let shifted = (value << count) & mask;
                    let c = if count <= 32 { (value >> (32 - count)) & 1 != 0 } else { false };
                    (shifted, c)
                }
            }
            // ASR - Arithmetic shift right (sign extends)
            (0, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let sign_bit = value & msb_bit != 0;
                    let mut result = value;
                    for _ in 0..count {
                        result = (result >> 1) | if sign_bit { msb_bit } else { 0 };
                    }
                    let c = if count > 0 { (value >> (count - 1)) & 1 != 0 } else { false };
                    (result & mask, c)
                }
            }
            // LSL - Logical shift left
            (1, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let shifted = (value << count) & mask;
                    let c = if count <= 32 { (value >> (32 - count)) & 1 != 0 } else { false };
                    (shifted, c)
                }
            }
            // LSR - Logical shift right
            (1, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let shifted = (value >> count) & mask;
                    let c = if count > 0 { (value >> (count - 1)) & 1 != 0 } else { false };
                    (shifted, c)
                }
            }
            // ROXL - Rotate through X left
            (2, true) => {
                // Stub - complex with X flag involvement
                (value, false)
            }
            // ROXR - Rotate through X right
            (2, false) => {
                // Stub - complex with X flag involvement
                (value, false)
            }
            // ROL - Rotate left
            (3, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let count = count % bits;
                    let rotated = ((value << count) | (value >> (bits - count))) & mask;
                    let c = rotated & 1 != 0;
                    (rotated, c)
                }
            }
            // ROR - Rotate right
            (3, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let count = count % bits;
                    let rotated = ((value >> count) | (value << (bits - count))) & mask;
                    let c = (value >> (count - 1)) & 1 != 0;
                    (rotated, c)
                }
            }
            _ => (value, false),
        };

        self.write_data_reg(reg, result, size);

        // Set flags
        // N and Z based on result
        self.set_flags_move(result, size);

        // C flag is last bit shifted out (or cleared if count=0)
        // X flag is set same as C for shifts, unchanged for rotates
        if count > 0 {
            self.regs.sr = Status::set_if(self.regs.sr, C, carry);
            // X is set for shifts (kind 0,1) but not for rotates (kind 2,3)
            if kind < 2 {
                self.regs.sr = Status::set_if(self.regs.sr, X, carry);
            }
        } else {
            // Count=0: C is cleared, X unchanged
            self.regs.sr &= !C;
        }

        // V is cleared except for ASL where it can be set
        // Simplified: always clear V for now
        self.regs.sr &= !crate::flags::V;

        // Timing: 6 + 2*count for byte/word, 8 + 2*count for long
        let base_cycles = if size == Size::Long { 8 } else { 6 };
        self.queue_internal(base_cycles + 2 * count as u8);
    }
}
