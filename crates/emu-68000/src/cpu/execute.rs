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

use crate::cpu::{AddrMode, InstrPhase, M68000, Size};
use crate::flags::{Status, C, N, V, X, Z};
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
            0x0 => {
                // Group 0 - immediate operations continuation
                self.immediate_op_continuation();
            }
            0x1 => self.decode_move_byte(op),
            0x2 => self.decode_move_long(op),
            0x3 => self.decode_move_word(op),
            0x4 => {
                // Group 4 continuations
                // MOVEM to memory: 0x4880-0x48FF (bits 11-8 = 8, bit 7 = 1)
                // MOVEM from memory: 0x4C80-0x4CFF (bits 11-8 = C, bit 7 = 1)
                // LEA: 0x41C0-0x4FFF with bit 8 set and bits 7-6 = 11
                // MOVE to CCR: 0x44C0-0x44FF (bits 11-8 = 4, bits 7-6 = 11)
                // MOVE to SR: 0x46C0-0x46FF (bits 11-8 = 6, bits 7-6 = 11)
                let subfield = (op >> 8) & 0xF;
                if subfield == 0x4 && (op >> 6) & 3 == 3 {
                    // MOVE to CCR continuation
                    self.exec_move_to_ccr_continuation();
                } else if subfield == 0x6 && (op >> 6) & 3 == 3 {
                    // MOVE to SR continuation
                    self.exec_move_to_sr_continuation();
                } else if subfield == 0x8 && op & 0x0080 != 0 {
                    // MOVEM to memory continuation
                    self.exec_movem_to_mem_continuation();
                } else if subfield == 0xC && op & 0x0080 != 0 {
                    // MOVEM from memory continuation
                    self.exec_movem_from_mem_continuation();
                } else if op & 0x01C0 == 0x01C0 {
                    // LEA continuation
                    self.exec_lea_continuation();
                } else {
                    self.instr_phase = crate::cpu::InstrPhase::Initial;
                }
            }
            0x6 => {
                // Branch instructions with word displacement
                self.branch_continuation();
            }
            _ => {
                // Instruction doesn't support phases, reset
                self.instr_phase = crate::cpu::InstrPhase::Initial;
            }
        }
    }

    /// Group 0: Bit manipulation, MOVEP, Immediate
    fn decode_group_0(&mut self, op: u16) {
        // Encoding guide for Group 0:
        // - Bit 8 set: Dynamic bit operations (BTST/BCHG/BCLR/BSET with register)
        // - Bits 11-9 = 100: Static bit operations (BTST/BCHG/BCLR/BSET with immediate)
        // - Otherwise: Immediate arithmetic/logic (ORI, ANDI, SUBI, ADDI, EORI, CMPI)

        if op & 0x0100 != 0 {
            // Bit operations with register (dynamic bit number in Dn)
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
        } else {
            // Check bits 11-9 for operation type
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;

            match (op >> 9) & 7 {
                0 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_ori(size, mode, ea_reg);
                }
                1 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_andi(size, mode, ea_reg);
                }
                2 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_subi(size, mode, ea_reg);
                }
                3 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_addi(size, mode, ea_reg);
                }
                4 => {
                    // Static bit operations with immediate bit number
                    match (op >> 6) & 3 {
                        0 => self.exec_btst_imm(mode, ea_reg),
                        1 => self.exec_bchg_imm(mode, ea_reg),
                        2 => self.exec_bclr_imm(mode, ea_reg),
                        3 => self.exec_bset_imm(mode, ea_reg),
                        _ => unreachable!(),
                    }
                }
                5 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_eori(size, mode, ea_reg);
                }
                6 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_cmpi(size, mode, ea_reg);
                }
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
                    // JSR, JMP, Trap, LINK, UNLK, MOVE USP, etc.
                    // Differentiated by bits 7-6
                    let subop = (op >> 6) & 3;
                    if subop == 2 {
                        // JSR <ea> (0x4E80-0x4EBF)
                        self.exec_jsr(mode, ea_reg);
                    } else if subop == 3 {
                        // JMP <ea> (0x4EC0-0x4EFF)
                        self.exec_jmp(mode, ea_reg);
                    } else {
                        // TRAP, LINK, UNLK, MOVE USP, misc (0x4E00-0x4E7F)
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
                }
                _ => self.illegal_instruction(),
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
        // LEA does NOT access memory - it only calculates the address
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // LEA (An),Am - just copy address register
                    self.regs.set_a(reg as usize, self.regs.a(r as usize));
                    self.queue_internal(4);
                }
                AddrMode::AddrIndDisp(r) => {
                    // LEA d16(An),Am - need extension word
                    // Store info for continuation
                    self.addr = self.regs.a(r as usize);
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    // After fetch, calculate and store
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::AddrIndIndex(r) => {
                    // LEA d8(An,Xn),Am
                    self.addr = self.regs.a(r as usize);
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::AbsShort => {
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::AbsLong => {
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::PcDisp => {
                    self.addr = self.regs.pc; // PC at extension word
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::PcIndex => {
                    self.addr = self.regs.pc;
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_lea_continuation(&mut self) {
        // Called after extension words are fetched for LEA
        let Some(src_mode) = self.src_mode else {
            return;
        };
        let dest_reg = self.addr2 as usize;

        let addr = match src_mode {
            AddrMode::AddrIndDisp(_) => {
                let disp = self.ext_words[0] as i16 as i32;
                (self.addr as i32).wrapping_add(disp) as u32
            }
            AddrMode::AddrIndIndex(_) => {
                let ext = self.ext_words[0];
                let disp = (ext & 0xFF) as i8 as i32;
                let xn = ((ext >> 12) & 7) as usize;
                let is_addr = ext & 0x8000 != 0;
                let is_long = ext & 0x0800 != 0;
                let idx_val = if is_addr {
                    self.regs.a(xn)
                } else {
                    self.regs.d[xn]
                };
                let idx_val = if is_long {
                    idx_val as i32
                } else {
                    idx_val as i16 as i32
                };
                (self.addr as i32)
                    .wrapping_add(disp)
                    .wrapping_add(idx_val) as u32
            }
            AddrMode::AbsShort => self.ext_words[0] as i16 as i32 as u32,
            AddrMode::AbsLong => {
                (u32::from(self.ext_words[0]) << 16) | u32::from(self.ext_words[1])
            }
            AddrMode::PcDisp => {
                let disp = self.ext_words[0] as i16 as i32;
                (self.addr as i32).wrapping_add(disp) as u32
            }
            AddrMode::PcIndex => {
                let ext = self.ext_words[0];
                let disp = (ext & 0xFF) as i8 as i32;
                let xn = ((ext >> 12) & 7) as usize;
                let is_addr = ext & 0x8000 != 0;
                let is_long = ext & 0x0800 != 0;
                let idx_val = if is_addr {
                    self.regs.a(xn)
                } else {
                    self.regs.d[xn]
                };
                let idx_val = if is_long {
                    idx_val as i32
                } else {
                    idx_val as i16 as i32
                };
                (self.addr as i32)
                    .wrapping_add(disp)
                    .wrapping_add(idx_val) as u32
            }
            _ => return,
        };

        self.regs.set_a(dest_reg, addr);
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal(8);
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
            // Word displacement follows - need continuation after fetch
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.instr_phase = InstrPhase::SrcEACalc; // Signal word branch
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // Byte displacement
            let offset = displacement as i32;
            self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
            self.queue_internal(4);
        }
    }

    fn exec_bsr(&mut self, displacement: i8) {
        if displacement == 0 {
            // Word displacement - fetch first, then continue
            // Store that this is BSR in data2 (1 = BSR)
            self.data2 = 1;
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.instr_phase = InstrPhase::SrcRead; // Signal word BSR
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // Byte displacement - push return address then branch
            self.data = self.regs.pc;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            // Calculate branch target
            let offset = displacement as i32;
            self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
            self.queue_internal(4);
        }
    }

    fn exec_bcc(&mut self, condition: u8, displacement: i8) {
        if Status::condition(self.regs.sr, condition) {
            if displacement == 0 {
                // Word displacement - fetch and continue
                // Store condition in data2 for continuation
                self.data2 = u32::from(condition) | 0x100; // Mark as conditional, taken
                self.micro_ops.push(MicroOp::FetchExtWord);
                self.instr_phase = InstrPhase::SrcEACalc;
                self.micro_ops.push(MicroOp::Execute);
            } else {
                let offset = displacement as i32;
                self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
                self.queue_internal(10); // Branch taken timing
            }
        } else {
            if displacement == 0 {
                // Skip word displacement
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            self.queue_internal(8); // Branch not taken timing
        }
    }

    /// Continuation for word displacement branches (BRA.W, BSR.W, Bcc.W).
    fn branch_continuation(&mut self) {
        // Extension word was fetched, apply word displacement
        let disp = self.ext_words[0] as i16 as i32;

        match self.instr_phase {
            InstrPhase::SrcEACalc => {
                // BRA.W or Bcc.W (taken)
                // PC already advanced past extension word, adjust from there
                // But displacement is relative to the start of the extension word
                // PC was at ext word, then advanced by 2, so: PC-2 + disp
                self.regs.pc = ((self.regs.pc as i32) - 2 + disp) as u32;
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal(10);
            }
            InstrPhase::SrcRead => {
                // BSR.W - push return address (after ext word) then branch
                // Return address is current PC (after ext word)
                self.data = self.regs.pc;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                // Branch: PC-2 + disp
                self.regs.pc = ((self.regs.pc as i32) - 2 + disp) as u32;
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal(4);
            }
            _ => {
                self.instr_phase = InstrPhase::Initial;
            }
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

    fn exec_btst_imm(&mut self, mode: u8, ea_reg: u8) {
        // BTST #imm,<ea> - test bit (immediate bit number)
        // Need to fetch the immediate bit number first
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.dst_mode = AddrMode::decode(mode, ea_reg);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 0; // Mark as BTST (0)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bchg_imm(&mut self, mode: u8, ea_reg: u8) {
        // BCHG #imm,<ea> - test and change bit
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.dst_mode = AddrMode::decode(mode, ea_reg);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 1; // Mark as BCHG (1)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bclr_imm(&mut self, mode: u8, ea_reg: u8) {
        // BCLR #imm,<ea> - test and clear bit
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.dst_mode = AddrMode::decode(mode, ea_reg);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 2; // Mark as BCLR (2)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bset_imm(&mut self, mode: u8, ea_reg: u8) {
        // BSET #imm,<ea> - test and set bit
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.dst_mode = AddrMode::decode(mode, ea_reg);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 3; // Mark as BSET (3)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bit_imm_continuation(&mut self) {
        // Continuation for BTST/BCHG/BCLR/BSET with immediate bit number
        let bit_num = (self.ext_words[0] & 0xFF) as u32;
        let op_type = self.data2; // 0=BTST, 1=BCHG, 2=BCLR, 3=BSET

        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                // For data registers, bit number is mod 32
                let bit = (bit_num % 32) as u8;
                let value = self.regs.d[r as usize];
                let mask = 1u32 << bit;
                let was_zero = (value & mask) == 0;

                // Set Z flag based on original bit
                self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);

                // Modify bit if not BTST
                match op_type {
                    0 => {} // BTST - test only
                    1 => self.regs.d[r as usize] ^= mask,  // BCHG - toggle
                    2 => self.regs.d[r as usize] &= !mask, // BCLR - clear
                    3 => self.regs.d[r as usize] |= mask,  // BSET - set
                    _ => {}
                }

                self.queue_internal(if op_type == 0 { 10 } else { 12 });
            }
            _ => {
                // Memory operand - bit number is mod 8
                self.queue_internal(12); // Stub for memory
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_ori(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // ORI #imm,<ea> - Inclusive OR Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            // Queue fetching the immediate value
            let ext_count = if size == Size::Long { 2 } else { 1 };
            for _ in 0..ext_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.size = size;
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_ori_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Get immediate value from extension words
        let imm = if self.size == Size::Long {
            (u32::from(self.ext_words[0]) << 16) | u32::from(self.ext_words[1])
        } else if self.size == Size::Word {
            u32::from(self.ext_words[0])
        } else {
            u32::from(self.ext_words[0] & 0xFF)
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst | imm;
                self.write_data_reg(r, result, self.size);
                self.set_flags_move(result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
            }
            _ => {
                // Memory operands - stub
                self.queue_internal(12);
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_andi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // ANDI #imm,<ea> - AND Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let ext_count = if size == Size::Long { 2 } else { 1 };
            for _ in 0..ext_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.size = size;
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_andi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let imm = if self.size == Size::Long {
            (u32::from(self.ext_words[0]) << 16) | u32::from(self.ext_words[1])
        } else if self.size == Size::Word {
            u32::from(self.ext_words[0])
        } else {
            u32::from(self.ext_words[0] & 0xFF)
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst & imm;
                self.write_data_reg(r, result, self.size);
                self.set_flags_move(result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
            }
            _ => {
                self.queue_internal(12);
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_subi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // SUBI #imm,<ea> - Subtract Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let ext_count = if size == Size::Long { 2 } else { 1 };
            for _ in 0..ext_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.size = size;
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_subi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let src = if self.size == Size::Long {
            (u32::from(self.ext_words[0]) << 16) | u32::from(self.ext_words[1])
        } else if self.size == Size::Word {
            u32::from(self.ext_words[0])
        } else {
            u32::from(self.ext_words[0] & 0xFF)
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst.wrapping_sub(src);
                self.write_data_reg(r, result, self.size);
                self.set_flags_sub(src, dst, result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
            }
            _ => {
                self.queue_internal(12);
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_addi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // ADDI #imm,<ea> - Add Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let ext_count = if size == Size::Long { 2 } else { 1 };
            for _ in 0..ext_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.size = size;
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_addi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let src = if self.size == Size::Long {
            (u32::from(self.ext_words[0]) << 16) | u32::from(self.ext_words[1])
        } else if self.size == Size::Word {
            u32::from(self.ext_words[0])
        } else {
            u32::from(self.ext_words[0] & 0xFF)
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst.wrapping_add(src);
                self.write_data_reg(r, result, self.size);
                self.set_flags_add(src, dst, result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
            }
            _ => {
                self.queue_internal(12);
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_eori(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // EORI #imm,<ea> - Exclusive OR Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let ext_count = if size == Size::Long { 2 } else { 1 };
            for _ in 0..ext_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.size = size;
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_eori_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let imm = if self.size == Size::Long {
            (u32::from(self.ext_words[0]) << 16) | u32::from(self.ext_words[1])
        } else if self.size == Size::Word {
            u32::from(self.ext_words[0])
        } else {
            u32::from(self.ext_words[0] & 0xFF)
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst ^ imm;
                self.write_data_reg(r, result, self.size);
                self.set_flags_move(result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
            }
            _ => {
                self.queue_internal(12);
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_cmpi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // CMPI #imm,<ea> - Compare Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let ext_count = if size == Size::Long { 2 } else { 1 };
            for _ in 0..ext_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.size = size;
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_cmpi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let src = if self.size == Size::Long {
            (u32::from(self.ext_words[0]) << 16) | u32::from(self.ext_words[1])
        } else if self.size == Size::Word {
            u32::from(self.ext_words[0])
        } else {
            u32::from(self.ext_words[0] & 0xFF)
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst.wrapping_sub(src);
                // CMP only sets flags, doesn't store result
                self.set_flags_sub(src, dst, result, self.size);
                self.queue_internal(if self.size == Size::Long { 14 } else { 8 });
            }
            _ => {
                self.queue_internal(12);
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    /// Dispatch continuation for group 0 immediate operations.
    fn immediate_op_continuation(&mut self) {
        let op = self.opcode;
        // Immediate ops: top 3 bits of bits 11-9 determine operation
        match (op >> 9) & 7 {
            0 => self.exec_ori_continuation(),
            1 => self.exec_andi_continuation(),
            2 => self.exec_subi_continuation(),
            3 => self.exec_addi_continuation(),
            4 => self.exec_bit_imm_continuation(), // Static bit operations
            5 => self.exec_eori_continuation(),
            6 => self.exec_cmpi_continuation(),
            _ => {
                self.instr_phase = InstrPhase::Initial;
            }
        }
    }

    fn exec_move_from_sr(&mut self, mode: u8, ea_reg: u8) {
        // MOVE SR,<ea> - Copy status register to destination
        // On 68000, this is NOT privileged (unlike 68010+)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Store SR in low word of Dn
                    let sr = u32::from(self.regs.sr);
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_0000) | sr;
                    self.queue_internal(6);
                }
                _ => {
                    // Memory destination - stub
                    self.queue_internal(8);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_negx(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // NEGX <ea> - Negate with extend (0 - dst - X)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    let x = if self.regs.sr & X != 0 { 1u32 } else { 0 };
                    let result = 0u32.wrapping_sub(value).wrapping_sub(x);
                    self.write_data_reg(r, result, size);

                    // NEGX flags: like NEG but X affects result and Z is only cleared, never set
                    let (src_masked, result_masked, msb) = match size {
                        Size::Byte => (value & 0xFF, result & 0xFF, 0x80u32),
                        Size::Word => (value & 0xFFFF, result & 0xFFFF, 0x8000),
                        Size::Long => (value, result, 0x8000_0000),
                    };

                    let mut sr = self.regs.sr;
                    // N: set if result is negative
                    sr = Status::set_if(sr, N, result_masked & msb != 0);
                    // Z: cleared if result is non-zero, unchanged otherwise
                    if result_masked != 0 {
                        sr &= !Z;
                    }
                    // V: set if overflow
                    let overflow = (src_masked & msb) != 0 && (result_masked & msb) != 0;
                    sr = Status::set_if(sr, V, overflow);
                    // C and X: set if borrow
                    let carry = src_masked != 0 || x != 0;
                    sr = Status::set_if(sr, C, carry);
                    sr = Status::set_if(sr, X, carry);

                    self.regs.sr = sr;
                    self.queue_internal(if size == Size::Long { 6 } else { 4 });
                }
                _ => {
                    self.queue_internal(8); // Memory stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_ccr(&mut self, mode: u8, ea_reg: u8) {
        // MOVE <ea>,CCR - Copy source to condition code register (low byte of SR)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Only low 5 bits are CCR (XNZVC)
                    let ccr = (self.regs.d[r as usize] & 0x1F) as u16;
                    self.regs.sr = (self.regs.sr & 0xFF00) | ccr;
                    self.queue_internal(12);
                }
                AddrMode::Immediate => {
                    // MOVE #imm,CCR - need to fetch immediate
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::Immediate);
                    self.micro_ops.push(MicroOp::Execute);
                }
                _ => {
                    self.queue_internal(12); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_ccr_continuation(&mut self) {
        // Continuation for MOVE #imm,CCR
        let ccr = (self.ext_words[0] & 0x1F) as u16;
        self.regs.sr = (self.regs.sr & 0xFF00) | ccr;
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal(12);
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

    fn exec_move_to_sr(&mut self, mode: u8, ea_reg: u8) {
        // MOVE <ea>,SR - Copy source to status register (privileged)
        if !self.regs.is_supervisor() {
            self.exception(8); // Privilege violation
            return;
        }

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    self.regs.sr = self.regs.d[r as usize] as u16;
                    self.queue_internal(12);
                }
                AddrMode::Immediate => {
                    // MOVE #imm,SR - need to fetch immediate
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::Immediate);
                    self.micro_ops.push(MicroOp::Execute);
                }
                _ => {
                    self.queue_internal(12); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_sr_continuation(&mut self) {
        // Continuation for MOVE #imm,SR
        self.regs.sr = self.ext_words[0];
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal(12);
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

    fn exec_pea(&mut self, mode: u8, ea_reg: u8) {
        // PEA <ea> - Push Effective Address onto stack
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // PEA (An) - push address register value
                    self.data = self.regs.a(r as usize);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.queue_internal(12);
                }
                AddrMode::AddrIndDisp(r) => {
                    // PEA d16(An) - need extension word
                    self.addr2 = self.regs.a(r as usize);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    // After fetch, calculate and push - simplified for now
                    self.queue_internal(16);
                }
                AddrMode::AbsShort => {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(16);
                }
                AddrMode::AbsLong => {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(20);
                }
                AddrMode::PcDisp => {
                    self.addr2 = self.regs.pc; // PC at extension word
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(16);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
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

    fn exec_movem_to_mem(&mut self, op: u16) {
        // MOVEM registers to memory
        // Format: 0100 1000 1s ea (s=0: word, s=1: long)
        // Register mask follows in extension word
        let size = if op & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        // Fetch the register mask
        self.size = size;
        self.addr2 = u32::from(mode);
        self.data2 = u32::from(ea_reg);
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.instr_phase = InstrPhase::SrcEACalc;
        self.src_mode = AddrMode::decode(mode, ea_reg);
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_movem_to_mem_continuation(&mut self) {
        let mask = self.ext_words[0];
        let Some(addr_mode) = self.src_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // For predecrement mode, registers are stored in reverse order (A7-A0, D7-D0)
        // For other modes, registers are stored in forward order (D0-D7, A0-A7)
        let is_predec = matches!(addr_mode, AddrMode::AddrIndPreDec(_));

        let mut addr = match addr_mode {
            AddrMode::AddrIndPreDec(r) => self.regs.a(r as usize),
            AddrMode::AddrInd(r) => self.regs.a(r as usize),
            _ => {
                // Other modes not implemented yet
                self.queue_internal(8);
                self.instr_phase = InstrPhase::Complete;
                return;
            }
        };

        // Count registers to transfer
        let count = mask.count_ones() as u32;
        let inc = if self.size == Size::Long { 4 } else { 2 };

        if is_predec {
            // Predecrement: write in reverse, highest register first
            // For predecrement, the order is A7,A6,...,A0,D7,D6,...,D0
            for i in (0..16).rev() {
                if mask & (1 << i) != 0 {
                    addr = addr.wrapping_sub(inc);
                    let value = if i < 8 {
                        self.regs.d[i]
                    } else {
                        self.regs.a(i - 8)
                    };
                    // Queue write - simplified synchronous for now
                    self.addr = addr;
                    self.data = value;
                }
            }
            // Update address register
            if let AddrMode::AddrIndPreDec(r) = addr_mode {
                self.regs.set_a(r as usize, addr);
            }
        }

        // Timing: 8 + 4n (word) or 8 + 8n (long)
        let cycles = 8 + count * (if self.size == Size::Long { 8 } else { 4 });
        self.queue_internal(cycles as u8);
        self.instr_phase = InstrPhase::Complete;
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

    fn exec_movem_from_mem(&mut self, op: u16) {
        // MOVEM memory to registers
        // Format: 0100 1100 1s ea (s=0: word, s=1: long)
        // Register mask follows in extension word
        let size = if op & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        // Fetch the register mask
        self.size = size;
        self.addr2 = u32::from(mode);
        self.data2 = u32::from(ea_reg);
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.instr_phase = InstrPhase::DstEACalc;
        self.dst_mode = AddrMode::decode(mode, ea_reg);
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_movem_from_mem_continuation(&mut self) {
        let mask = self.ext_words[0];
        let Some(addr_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // For postincrement and other modes, registers are loaded D0-D7, A0-A7
        let is_postinc = matches!(addr_mode, AddrMode::AddrIndPostInc(_));

        let mut addr = match addr_mode {
            AddrMode::AddrIndPostInc(r) => self.regs.a(r as usize),
            AddrMode::AddrInd(r) => self.regs.a(r as usize),
            _ => {
                // Other modes not implemented yet
                self.queue_internal(12);
                self.instr_phase = InstrPhase::Complete;
                return;
            }
        };

        // Count registers to transfer
        let count = mask.count_ones() as u32;
        let inc = if self.size == Size::Long { 4 } else { 2 };

        // Load registers in order D0-D7, A0-A7
        for i in 0..16 {
            if mask & (1 << i) != 0 {
                // Simplified - actual implementation needs proper bus access
                self.addr = addr;
                addr = addr.wrapping_add(inc);
            }
        }

        if is_postinc {
            // Update address register
            if let AddrMode::AddrIndPostInc(r) = addr_mode {
                self.regs.set_a(r as usize, addr);
            }
        }

        // Timing: 12 + 4n (word) or 12 + 8n (long)
        let cycles = 12 + count * (if self.size == Size::Long { 8 } else { 4 });
        self.queue_internal(cycles as u8);
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_trap(&mut self, op: u16) {
        let vector = 32 + (op & 0xF) as u8;
        self.exception(vector);
    }

    fn exec_link(&mut self, reg: u8) {
        // LINK An,#displacement
        // 1. Push An onto stack
        // 2. Copy SP to An (An becomes frame pointer)
        // 3. Add signed displacement to SP (allocate stack space)

        // We need the displacement word - queue its fetch
        // For now, handle the register operations synchronously
        // and assume displacement is already in ext_words[0]

        // Push An
        let an_value = self.regs.a(reg as usize);
        self.data = an_value;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // Copy SP to An (after push, so it points to saved value)
        // This happens after the push completes
        // For proper implementation, we'd need continuation logic
        // Simplified: do it now with adjusted SP
        let sp = self.regs.active_sp().wrapping_sub(4);
        self.regs.set_a(reg as usize, sp);

        // Fetch and apply displacement
        self.micro_ops.push(MicroOp::FetchExtWord);

        self.queue_internal(16);
    }

    fn exec_unlk(&mut self, reg: u8) {
        // UNLK An
        // 1. Copy An to SP (restore stack to frame pointer)
        // 2. Pop An from stack (restore old frame pointer)

        // Copy An to SP
        let an_value = self.regs.a(reg as usize);
        self.regs.set_a(7, an_value);

        // Pop An from stack
        self.micro_ops.push(MicroOp::PopLongHi);
        self.micro_ops.push(MicroOp::PopLongLo);

        // The popped value goes to An - need continuation
        // For now, store which register to restore
        self.addr2 = u32::from(reg);

        self.queue_internal(12);
    }

    fn exec_move_usp(&mut self, op: u16) {
        // MOVE USP - requires supervisor mode
        if !self.regs.is_supervisor() {
            self.exception(8); // Privilege violation
            return;
        }

        let reg = (op & 7) as usize;
        if op & 0x0008 != 0 {
            // USP -> An (bit 3 = 1)
            let usp = self.regs.usp;
            self.regs.set_a(reg, usp);
        } else {
            // An -> USP (bit 3 = 0)
            self.regs.usp = self.regs.a(reg);
        }
        self.queue_internal(4);
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

    fn exec_jsr(&mut self, mode: u8, ea_reg: u8) {
        // JSR <ea> - Jump to Subroutine (push return address, then jump)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // JSR (An) - push PC, jump to (An)
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.regs.pc = self.regs.a(r as usize);
                    self.queue_internal(16);
                }
                AddrMode::AddrIndDisp(r) => {
                    // JSR d16(An) - need extension word
                    let pc_at_ext = self.regs.pc;
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    // After fetch, we need to calculate address and jump
                    // Store base for calculation
                    self.addr = self.regs.a(r as usize);
                    self.addr2 = pc_at_ext.wrapping_add(2); // Return address
                    self.queue_internal(18);
                }
                AddrMode::AbsShort => {
                    let return_addr = self.regs.pc.wrapping_add(2);
                    self.data = return_addr;
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.queue_internal(18);
                }
                AddrMode::AbsLong => {
                    let return_addr = self.regs.pc.wrapping_add(4);
                    self.data = return_addr;
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.queue_internal(20);
                }
                AddrMode::PcDisp => {
                    let pc_at_ext = self.regs.pc;
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.addr2 = pc_at_ext.wrapping_add(2); // Return address
                    self.queue_internal(18);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_jmp(&mut self, mode: u8, ea_reg: u8) {
        // JMP <ea> - Jump to address
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // JMP (An) - jump to address in An
                    self.regs.pc = self.regs.a(r as usize);
                    self.queue_internal(8);
                }
                AddrMode::AddrIndDisp(r) => {
                    // JMP d16(An) - need extension word
                    let base = self.regs.a(r as usize);
                    self.addr = base;
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(10);
                }
                AddrMode::AbsShort => {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(10);
                }
                AddrMode::AbsLong => {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(12);
                }
                AddrMode::PcDisp => {
                    self.addr = self.regs.pc; // PC at extension
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(10);
                }
                AddrMode::PcIndex => {
                    self.addr = self.regs.pc;
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.queue_internal(14);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
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

    fn exec_divu(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // DIVU <ea>,Dn - unsigned 32/16 -> 16r:16q division
        // Result: Dn = remainder(high word) : quotient(low word)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let divisor = self.regs.d[r as usize] & 0xFFFF;
                    let dividend = self.regs.d[reg as usize];

                    if divisor == 0 {
                        // Division by zero - trap
                        self.exception(5);
                        return;
                    }

                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;

                    // Check for overflow (quotient > 16 bits)
                    if quotient > 0xFFFF {
                        // Overflow - set V flag, don't store result
                        self.regs.sr |= crate::flags::V;
                        self.regs.sr &= !crate::flags::C; // C always cleared
                        // N and Z are undefined on overflow
                    } else {
                        // Store result: remainder:quotient
                        self.regs.d[reg as usize] = (remainder << 16) | quotient;

                        // Set flags
                        self.regs.sr &= !(crate::flags::V | crate::flags::C);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::N, quotient & 0x8000 != 0);
                    }

                    // Timing: ~140 cycles (varies with operand)
                    self.queue_internal(140);
                }
                _ => {
                    self.queue_internal(140); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_divs(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // DIVS <ea>,Dn - signed 32/16 -> 16r:16q division
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let divisor = (self.regs.d[r as usize] as i16) as i32;
                    let dividend = self.regs.d[reg as usize] as i32;

                    if divisor == 0 {
                        // Division by zero - trap
                        self.exception(5);
                        return;
                    }

                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;

                    // Check for overflow (quotient doesn't fit in signed 16-bit)
                    if !(-32768..=32767).contains(&quotient) {
                        // Overflow
                        self.regs.sr |= crate::flags::V;
                        self.regs.sr &= !crate::flags::C;
                    } else {
                        // Store result: remainder:quotient (both as 16-bit values)
                        let q = quotient as i16 as u16 as u32;
                        let r = remainder as i16 as u16 as u32;
                        self.regs.d[reg as usize] = (r << 16) | q;

                        // Set flags
                        self.regs.sr &= !(crate::flags::V | crate::flags::C);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::N, quotient < 0);
                    }

                    // Timing: ~158 cycles (varies with operand)
                    self.queue_internal(158);
                }
                _ => {
                    self.queue_internal(158); // Memory source stub
                }
            }
        } else {
            self.illegal_instruction();
        }
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
