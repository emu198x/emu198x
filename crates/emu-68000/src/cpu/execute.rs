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
                // Group 0 - check for MOVEP (bit 8 set + mode 1) vs immediate ops
                let mode = ((op >> 3) & 7) as u8;
                if op & 0x0100 != 0 && mode == 1 {
                    // MOVEP continuation
                    self.movep_continuation();
                } else {
                    // Immediate operations continuation
                    self.immediate_op_continuation();
                }
            }
            0x1 => self.decode_move_byte(op),
            0x2 => self.decode_move_long(op),
            0x3 => self.decode_move_word(op),
            0x4 => {
                // Group 4 continuations
                // PEA: 0x4840-0x487F (bits 11-8 = 8, bits 7-6 = 01)
                // MOVEM to memory: 0x4880-0x48FF (bits 11-8 = 8, bit 7 = 1)
                // MOVEM from memory: 0x4C80-0x4CFF (bits 11-8 = C, bit 7 = 1)
                // LEA: 0x41C0-0x4FFF with bit 8 set and bits 7-6 = 11
                // MOVE to CCR: 0x44C0-0x44FF (bits 11-8 = 4, bits 7-6 = 11)
                // MOVE to SR: 0x46C0-0x46FF (bits 11-8 = 6, bits 7-6 = 11)
                // RTS: 0x4E75 (bits 11-8 = E, bits 7-6 = 01)
                // JSR: 0x4E80-0x4EBF (bits 11-8 = E, bits 7-6 = 10)
                // JMP: 0x4EC0-0x4EFF (bits 11-8 = E, bits 7-6 = 11)
                let subfield = (op >> 8) & 0xF;
                let bits_7_6 = (op >> 6) & 3;
                if subfield == 0x4 && bits_7_6 == 3 {
                    // MOVE to CCR continuation
                    self.exec_move_to_ccr_continuation();
                } else if subfield == 0x6 && bits_7_6 == 3 {
                    // MOVE to SR continuation
                    self.exec_move_to_sr_continuation();
                } else if subfield == 0x8 && bits_7_6 == 1 {
                    // PEA continuation (0x4840-0x487F)
                    self.exec_pea_continuation();
                } else if subfield == 0x8 && op & 0x0080 != 0 {
                    // MOVEM to memory continuation
                    self.exec_movem_to_mem_continuation();
                } else if subfield == 0xC && op & 0x0080 != 0 {
                    // MOVEM from memory continuation
                    self.exec_movem_from_mem_continuation();
                } else if subfield == 0xE && bits_7_6 == 1 && op & 0x3F == 0x35 {
                    // RTS continuation (0x4E75)
                    self.exec_rts_continuation();
                } else if subfield == 0xE && bits_7_6 == 1 && op & 0x3F == 0x37 {
                    // RTR continuation (0x4E77)
                    self.exec_rtr_continuation();
                } else if subfield == 0xE && (op >> 4) & 0xF == 5 && op & 8 == 0 {
                    // LINK continuation (0x4E50-0x4E57)
                    self.exec_link_continuation();
                } else if subfield == 0xE && (op >> 4) & 0xF == 5 && op & 8 != 0 {
                    // UNLK continuation (0x4E58-0x4E5F)
                    self.exec_unlk_continuation();
                } else if subfield == 0xE && bits_7_6 == 2 {
                    // JSR continuation
                    self.exec_jsr_continuation();
                } else if subfield == 0xE && bits_7_6 == 3 {
                    // JMP continuation
                    self.exec_jmp_continuation();
                } else if op & 0x01C0 == 0x01C0 {
                    // LEA continuation
                    self.exec_lea_continuation();
                } else {
                    self.instr_phase = crate::cpu::InstrPhase::Initial;
                }
            }
            0x5 => {
                // DBcc/Scc continuations
                let mode = ((op >> 3) & 7) as u8;
                if mode == 1 {
                    // DBcc continuation
                    self.dbcc_continuation();
                } else {
                    // Scc doesn't have continuations
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
        // - Bit 8 set + mode=001: MOVEP (move peripheral data)
        // - Bit 8 set + mode!=001: Dynamic bit operations (BTST/BCHG/BCLR/BSET with register)
        // - Bits 11-9 = 100: Static bit operations (BTST/BCHG/BCLR/BSET with immediate)
        // - Otherwise: Immediate arithmetic/logic (ORI, ANDI, SUBI, ADDI, EORI, CMPI)

        let mode = ((op >> 3) & 7) as u8;

        if op & 0x0100 != 0 {
            if mode == 1 {
                // MOVEP - Move Peripheral Data
                // Encoding: 0000_rrr1_0sD0_1aaa
                // rrr = data register, s = size (0=word, 1=long)
                // D = direction (0=mem to reg, 1=reg to mem), aaa = address register
                self.exec_movep(op);
            } else {
                // Bit operations with register (dynamic bit number in Dn)
                let reg = ((op >> 9) & 7) as u8;
                let ea_reg = (op & 7) as u8;

                match (op >> 6) & 3 {
                    0 => self.exec_btst_reg(reg, mode, ea_reg),
                    1 => self.exec_bchg_reg(reg, mode, ea_reg),
                    2 => self.exec_bclr_reg(reg, mode, ea_reg),
                    3 => self.exec_bset_reg(reg, mode, ea_reg),
                    _ => unreachable!(),
                }
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

    /// MOVEP - Move Peripheral Data
    /// Transfers data between a data register and alternate bytes in memory.
    /// Used for accessing 8-bit peripherals on the 16-bit bus.
    fn exec_movep(&mut self, op: u16) {
        // Encoding: 0000_rrr1_0sD0_1aaa + 16-bit displacement
        // rrr = data register, s = size (0=word, 1=long)
        // D = direction (0=mem to reg, 1=reg to mem), aaa = address register
        let data_reg = ((op >> 9) & 7) as u8;
        let addr_reg = (op & 7) as u8;
        let is_long = op & 0x0040 != 0;
        let to_memory = op & 0x0080 != 0;

        if self.prefetch_only {
            // In prefetch_only mode, use preloaded ext_words directly
            let displacement = self.next_ext_word() as i16 as i32;
            self.exec_movep_with_disp(data_reg, addr_reg, is_long, to_memory, displacement);
        } else {
            // Need to fetch the displacement extension word first
            // Store fields in data2: bits 0-2=addr_reg, bits 4-6=data_reg, bit 8=is_long, bit 9=to_memory
            self.data2 = u32::from(addr_reg)
                | (u32::from(data_reg) << 4)
                | (if is_long { 0x100 } else { 0 })
                | (if to_memory { 0x200 } else { 0 });
            self.movem_long_phase = 0;
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.micro_ops.push(MicroOp::Execute); // Continue after fetching displacement
            self.instr_phase = InstrPhase::SrcRead;
        }
    }

    fn exec_movep_with_disp(&mut self, data_reg: u8, addr_reg: u8, is_long: bool, to_memory: bool, displacement: i32) {
        let base_addr = (self.regs.a(addr_reg as usize) as i32).wrapping_add(displacement) as u32;

        // Update data2 for phase handler
        self.data2 = ((data_reg as u32) << 4) | (if to_memory { 0x200 } else { 0 });

        if to_memory {
            // Register to memory: write bytes to alternate addresses
            let value = self.regs.d[data_reg as usize];
            if is_long {
                // MOVEP.L Dn,d(An): 4 byte writes
                self.addr = base_addr;
                self.addr2 = value;
                self.data = (value >> 24) & 0xFF;
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W Dn,d(An): 2 byte writes
                self.addr = base_addr;
                self.addr2 = value;
                self.data = (value >> 8) & 0xFF;
                self.movem_long_phase = 4;
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            }
        } else {
            // Memory to register: read bytes from alternate addresses
            if is_long {
                // MOVEP.L d(An),Dn: 4 byte reads
                self.addr = base_addr;
                self.addr2 = 0;
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W d(An),Dn: 2 byte reads
                self.addr = base_addr;
                self.addr2 = 0;
                self.movem_long_phase = 4;
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            }
        }
    }

    /// Continuation for MOVEP after fetching displacement.
    fn exec_movep_continuation(&mut self) {
        // Extract fields from data2
        let addr_reg = (self.data2 & 7) as usize;
        let data_reg = ((self.data2 >> 4) & 7) as usize;
        let is_long = self.data2 & 0x100 != 0;
        let to_memory = self.data2 & 0x200 != 0;
        // Displacement is always in ext_words[0] - it's the first (and only) extension word
        let displacement = self.ext_words[0] as i16 as i32;

        let base_addr = (self.regs.a(addr_reg) as i32).wrapping_add(displacement) as u32;

        // Update data2 for phase handler
        self.data2 = (data_reg << 4) as u32 | (if to_memory { 0x200 } else { 0 });

        if to_memory {
            // Register to memory: write bytes to alternate addresses
            let value = self.regs.d[data_reg];
            if is_long {
                // MOVEP.L Dn,d(An): 4 byte writes
                self.addr = base_addr;
                self.addr2 = value; // Store full value for later phases
                self.data = (value >> 24) & 0xFF; // Byte 3 (MSB) to write first
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after write
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W Dn,d(An): 2 byte writes
                self.addr = base_addr;
                self.addr2 = value; // Store full value for later phases
                self.data = (value >> 8) & 0xFF; // High byte of word to write first
                self.movem_long_phase = 4; // Start at phase 4 for word
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after write
                self.instr_phase = InstrPhase::DstWrite;
            }
        } else {
            // Memory to register: read bytes from alternate addresses
            if is_long {
                // MOVEP.L d(An),Dn: 4 byte reads
                self.addr = base_addr;
                self.addr2 = 0; // Will accumulate result
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after read
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W d(An),Dn: 2 byte reads
                self.addr = base_addr;
                self.addr2 = 0; // Will accumulate result
                self.movem_long_phase = 4; // Start at phase 4 for word
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after read
                self.instr_phase = InstrPhase::DstWrite;
            }
        }
    }

    /// Handle MOVEP read/write phases.
    fn exec_movep_phase(&mut self) {
        // Extract fields from data2: bits 4-6=data_reg, bit 9=to_memory
        let data_reg = ((self.data2 >> 4) & 7) as usize;
        let to_memory = self.data2 & 0x200 != 0;

        if to_memory {
            // Writing: advance to next byte
            match self.movem_long_phase {
                0 => {
                    // Just wrote byte 3, write byte 2
                    self.addr = self.addr.wrapping_add(2);
                    self.data = (self.addr2 >> 16) & 0xFF; // Byte 2
                    self.movem_long_phase = 1;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                1 => {
                    // Just wrote byte 2, write byte 1
                    self.addr = self.addr.wrapping_add(2);
                    self.data = (self.addr2 >> 8) & 0xFF; // Byte 1
                    self.movem_long_phase = 2;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                2 => {
                    // Just wrote byte 1, write byte 0
                    self.addr = self.addr.wrapping_add(2);
                    self.data = self.addr2 & 0xFF; // Byte 0 (LSB)
                    self.movem_long_phase = 3;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                3 => {
                    // Done with long write
                    self.instr_phase = InstrPhase::Complete;
                }
                4 => {
                    // Word: just wrote high byte (bits 15-8), write low byte (bits 7-0)
                    self.addr = self.addr.wrapping_add(2);
                    self.data = self.addr2 & 0xFF; // Low byte of word
                    self.movem_long_phase = 5;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                5 => {
                    // Done with word write
                    self.instr_phase = InstrPhase::Complete;
                }
                _ => {
                    self.instr_phase = InstrPhase::Complete;
                }
            }
        } else {
            // Reading: accumulate bytes and advance
            let byte_read = self.data as u8;
            match self.movem_long_phase {
                0 => {
                    // Read byte 3 (MSB), read byte 2
                    self.addr2 = u32::from(byte_read) << 24;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                1 => {
                    // Read byte 2, read byte 1
                    self.addr2 |= u32::from(byte_read) << 16;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 2;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                2 => {
                    // Read byte 1, read byte 0
                    self.addr2 |= u32::from(byte_read) << 8;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 3;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                3 => {
                    // Read byte 0 (LSB), store to register
                    self.addr2 |= u32::from(byte_read);
                    self.regs.d[data_reg] = self.addr2;
                    self.instr_phase = InstrPhase::Complete;
                }
                4 => {
                    // Word: read high byte, read low byte
                    self.addr2 = u32::from(byte_read) << 8;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 5;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                5 => {
                    // Word: read low byte, store to register (low word only)
                    self.addr2 |= u32::from(byte_read);
                    // MOVEP.W only affects low word of Dn
                    self.regs.d[data_reg] = (self.regs.d[data_reg] & 0xFFFF_0000) | self.addr2;
                    self.instr_phase = InstrPhase::Complete;
                }
                _ => {
                    self.instr_phase = InstrPhase::Complete;
                }
            }
        }
    }

    /// MOVEP continuation dispatcher.
    fn movep_continuation(&mut self) {
        use crate::cpu::InstrPhase;
        match self.instr_phase {
            InstrPhase::SrcRead => {
                // Just fetched the displacement - set up the actual transfers
                self.exec_movep_continuation();
            }
            InstrPhase::DstWrite => {
                // In the middle of read/write phases
                self.exec_movep_phase();
            }
            _ => {
                // Complete
                self.instr_phase = InstrPhase::Complete;
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
                        // Memory destination - write zero
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = 0;
                        // Set flags: N=0, Z=1, V=0, C=0
                        self.regs.sr = Status::clear_vc(self.regs.sr);
                        self.regs.sr = Status::update_nz_byte(self.regs.sr, 0);
                        // Queue write
                        self.queue_write_ops(size);
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
        // RTS = 16 cycles: 8 (pop) + 8 (prefetch at return address)
        // Pop return address from stack into self.data
        self.micro_ops.push(MicroOp::PopLongHi);
        self.micro_ops.push(MicroOp::PopLongLo);
        // Continue to set PC from popped value
        self.instr_phase = InstrPhase::SrcRead;
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_rts_continuation(&mut self) {
        // After pop, data contains the return address
        // Set PC = return address + 4 (for prefetch)
        self.regs.pc = self.data.wrapping_add(4);
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal_no_pc(8); // Prefetch cycles
    }

    fn exec_bra(&mut self, displacement: i8) {
        if displacement == 0 {
            // Word displacement follows - need continuation after fetch
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.instr_phase = InstrPhase::SrcEACalc; // Signal word branch
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // Byte displacement - PC is set directly
            let offset = displacement as i32;
            self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
            self.queue_internal_no_pc(4);
        }
    }

    fn exec_bsr(&mut self, displacement: i8) {
        // BSR = 18 cycles: 8 (push) + 2 (internal) + 8 (prefetch at target)
        // With prefetch model: PC already points past opcode.
        // Displacement is relative to PC - 2 (the opcode position).
        // Return address is PC - 2 (address of BSR instruction).
        // After BSR, PC should be target + 4 for prefetch.
        if displacement == 0 {
            // Word displacement - use prefetched extension word
            let disp = self.next_ext_word() as i16 as i32;
            // Return address is PC - 2 (opcode position) + 4 (instruction size) = PC + 2
            // Wait, for BSR.W the instruction is 4 bytes, so return = opcode + 4 = PC + 2
            // But PC is past the prefetched extension, so return = PC
            self.data = self.regs.pc;
            // Displacement is relative to extension word position (PC - 2)
            let pc_at_ext = (self.regs.pc as i32).wrapping_sub(2);
            let target = pc_at_ext.wrapping_add(disp) as u32;
            self.internal_cycles = 2;
            self.internal_advances_pc = false;
            self.micro_ops.push(MicroOp::Internal);
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
            self.queue_internal_no_pc(8);
        } else {
            // Byte displacement - instruction is 2 bytes (opcode only)
            // Return address is opcode + 2 = PC (already past opcode)
            // But tests show return should be PC - 2 (opcode address)
            // This suggests the 68000 pushes PC BEFORE the prefetch increment
            self.data = self.regs.pc.wrapping_sub(2); // Return to BSR instruction address
            // Displacement is relative to opcode position (PC - 2)
            let pc_base = (self.regs.pc as i32).wrapping_sub(2);
            let target = pc_base.wrapping_add(displacement as i32) as u32;
            self.internal_cycles = 2;
            self.internal_advances_pc = false;
            self.micro_ops.push(MicroOp::Internal);
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
            self.queue_internal_no_pc(8);
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
                // PC is set directly
                let offset = displacement as i32;
                self.regs.pc = (self.regs.pc as i32).wrapping_add(offset) as u32;
                self.queue_internal_no_pc(10); // Branch taken timing
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
                // PC is set directly, so don't advance during internal cycles
                self.regs.pc = ((self.regs.pc as i32) - 2 + disp) as u32;
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal_no_pc(10);
            }
            InstrPhase::SrcRead => {
                // BSR.W - push return address (after ext word) then branch
                // Return address is current PC (after ext word)
                self.data = self.regs.pc;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                // Branch: PC-2 + disp - PC is set directly
                self.regs.pc = ((self.regs.pc as i32) - 2 + disp) as u32;
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal_no_pc(4);
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
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 0; // 0 = BTST
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
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
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 1; // 1 = BCHG
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
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
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 2; // 2 = BCLR
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
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
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 3; // 3 = BSET
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
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
        // Special case: mode=7, reg=4 means CCR (byte) or SR (word)
        if mode == 7 && ea_reg == 4 {
            match size {
                Some(Size::Byte) => {
                    // ORI #xx,CCR - 20 cycles
                    // Get immediate from prefetch queue (only low byte used)
                    let imm = (self.next_ext_word() & 0x1F) as u8; // Mask to valid CCR bits
                    let ccr = self.regs.ccr() & 0x1F; // CCR bits 5-7 always read as 0
                    self.regs.set_ccr(ccr | imm);
                    // In normal mode, advance PC for prefetch refill
                    if !self.prefetch_only {
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                    }
                    self.queue_internal_no_pc(20);
                }
                Some(Size::Word) => {
                    // ORI #xx,SR - privileged, 20 cycles
                    if !self.regs.is_supervisor() {
                        self.exception(8); // Privilege violation
                        return;
                    }
                    let imm = self.next_ext_word();
                    self.regs.sr |= imm;
                    if !self.prefetch_only {
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                    }
                    self.queue_internal_no_pc(20);
                }
                _ => self.illegal_instruction(),
            }
            return;
        }

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
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.data = imm; // Immediate as source
                self.data2 = 3;  // 3 = OR
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
                // Don't set Complete yet - AluMemRmw will handle it
            }
        }
    }

    fn exec_andi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // ANDI #imm,<ea> - AND Immediate
        // Special case: mode=7, reg=4 means CCR (byte) or SR (word)
        if mode == 7 && ea_reg == 4 {
            match size {
                Some(Size::Byte) => {
                    // ANDI #xx,CCR - 20 cycles
                    // Get immediate from prefetch queue (low byte, masked to CCR bits)
                    let imm = (self.next_ext_word() & 0x1F) as u8;
                    let ccr = self.regs.ccr() & 0x1F;
                    self.regs.set_ccr(ccr & imm);
                    // In normal mode, advance PC for prefetch refill
                    // In prefetch_only mode, next_ext_word + tick ending handle it
                    if !self.prefetch_only {
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                    }
                    self.queue_internal_no_pc(20);
                }
                Some(Size::Word) => {
                    // ANDI #xx,SR - privileged, 20 cycles
                    if !self.regs.is_supervisor() {
                        self.exception(8); // Privilege violation
                        return;
                    }
                    let imm = self.next_ext_word();
                    self.regs.sr &= imm;
                    if !self.prefetch_only {
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                    }
                    self.queue_internal_no_pc(20);
                }
                _ => self.illegal_instruction(),
            }
            return;
        }

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
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.data = imm; // Immediate as source
                self.data2 = 2;  // 2 = AND
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
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
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.data = src; // Immediate as source
                self.data2 = 1;  // 1 = SUB
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
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
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.data = src; // Immediate as source
                self.data2 = 0;  // 0 = ADD
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
    }

    fn exec_eori(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // EORI #imm,<ea> - Exclusive OR Immediate
        // Special case: mode=7, reg=4 means CCR (byte) or SR (word)
        if mode == 7 && ea_reg == 4 {
            match size {
                Some(Size::Byte) => {
                    // EORI #xx,CCR - 20 cycles
                    // Get immediate from prefetch queue (low byte, masked to CCR bits)
                    let imm = (self.next_ext_word() & 0x1F) as u8;
                    let ccr = self.regs.ccr() & 0x1F;
                    self.regs.set_ccr(ccr ^ imm);
                    // In normal mode, advance PC for prefetch refill
                    if !self.prefetch_only {
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                    }
                    self.queue_internal_no_pc(20);
                }
                Some(Size::Word) => {
                    // EORI #xx,SR - privileged, 20 cycles
                    if !self.regs.is_supervisor() {
                        self.exception(8); // Privilege violation
                        return;
                    }
                    let imm = self.next_ext_word();
                    self.regs.sr ^= imm;
                    if !self.prefetch_only {
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                    }
                    self.queue_internal_no_pc(20);
                }
                _ => self.illegal_instruction(),
            }
            return;
        }

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
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.data = imm; // Immediate as source
                self.data2 = 4;  // 4 = EOR
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
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
                self.set_flags_cmp(src, dst, result, self.size);
                self.queue_internal(if self.size == Size::Long { 14 } else { 8 });
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read and compare
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.data = src;  // Immediate as source
                self.data2 = 14;  // 14 = CMPI
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemSrc);
            }
        }
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
                    // Memory destination - write SR word
                    self.size = Size::Word;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(self.regs.sr);
                    self.micro_ops.push(MicroOp::WriteWord);
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
                    let x = u32::from(self.regs.sr & X != 0);
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
                    // Memory operand - use AluMemRmw
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 7; // 7 = NEGX
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
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
                    // Memory source - read word and apply to CCR
                    self.size = Size::Word;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.micro_ops.push(MicroOp::ReadWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::DataReg(0)); // Marker for memory source
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_ccr_continuation(&mut self) {
        // Continuation for MOVE <ea>,CCR
        // For immediate: data is in ext_words[0]
        // For memory: data is in self.data (from ReadWord)
        let src = if self.dst_mode == Some(AddrMode::Immediate) {
            self.ext_words[0]
        } else {
            self.data as u16
        };
        let ccr = src & 0x1F;
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
                    // Memory operand - read-modify-write
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 5; // 5 = NEG
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
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
                    // Memory source - read word and apply to SR
                    self.size = Size::Word;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.micro_ops.push(MicroOp::ReadWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::DataReg(0)); // Marker for memory source
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_sr_continuation(&mut self) {
        // Continuation for MOVE <ea>,SR
        // For immediate: data is in ext_words[0]
        // For memory: data is in self.data (from ReadWord)
        let src = if self.dst_mode == Some(AddrMode::Immediate) {
            self.ext_words[0]
        } else {
            self.data as u16
        };
        self.regs.sr = src;
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
                    // Memory operand - read-modify-write
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 6; // 6 = NOT
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_nbcd(&mut self, mode: u8, ea_reg: u8) {
        // NBCD - Negate Decimal with Extend
        // Computes 0 - <ea> - X (BCD negation)
        // Format: 0100 1000 00 mode reg (byte only)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.regs.d[r as usize] as u8;
                    let x = u8::from(self.regs.sr & X != 0);

                    let (result, borrow) = self.bcd_sub(0, src, x);

                    // Write result to low byte
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(result);

                    // Set flags
                    let mut sr = self.regs.sr;
                    // Z: cleared if non-zero, unchanged otherwise
                    if result != 0 {
                        sr &= !Z;
                    }
                    // C and X: set if decimal borrow (result != 0 or X was set)
                    sr = Status::set_if(sr, C, borrow);
                    sr = Status::set_if(sr, X, borrow);
                    // N: undefined, but set based on MSB
                    sr = Status::set_if(sr, N, result & 0x80 != 0);
                    self.regs.sr = sr;

                    self.queue_internal(6);
                }
                _ => {
                    // Memory operand - read-modify-write with BCD negate
                    self.size = Size::Byte; // NBCD is always byte
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 8; // 8 = NBCD
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
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
        // Uses prefetched extension words. Computes EA and pushes it.
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // PEA (An) = 12 cycles: 4 (internal) + 8 (push)
                    self.data = self.regs.a(r as usize);
                    self.internal_cycles = 4;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    // Advance PC by 2 (single-word instruction, no extension)
                    self.regs.pc = self.regs.pc.wrapping_add(2);
                }
                AddrMode::AddrIndDisp(r) => {
                    // PEA d16(An) = 16 cycles: 8 (internal) + 8 (push)
                    let disp = self.next_ext_word() as i16 as i32;
                    let ea = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                    self.data = ea;
                    self.internal_cycles = 8;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    // Advance PC for prefetch refill
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                }
                AddrMode::AddrIndIndex(r) => {
                    // PEA d8(An,Xn) = 20 cycles: 12 (internal) + 8 (push)
                    let ea = self.calc_index_ea(self.regs.a(r as usize));
                    self.data = ea;
                    self.internal_cycles = 12;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    // Advance PC for prefetch refill
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                }
                AddrMode::AbsShort => {
                    // PEA addr.W = 16 cycles: 8 (internal) + 8 (push)
                    let ea = self.next_ext_word() as i16 as i32 as u32;
                    self.data = ea;
                    self.internal_cycles = 8;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    // Advance PC for prefetch refill
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                }
                AddrMode::AbsLong => {
                    // PEA addr.L = 20 cycles: 12 (internal) + 8 (push)
                    // Need both extension words
                    if self.ext_count >= 2 {
                        let hi = u32::from(self.ext_words[0]);
                        let lo = u32::from(self.ext_words[1]);
                        let ea = (hi << 16) | lo;
                        self.data = ea;
                        self.internal_cycles = 12;
                        self.internal_advances_pc = false;
                        self.micro_ops.push(MicroOp::Internal);
                        self.micro_ops.push(MicroOp::PushLongHi);
                        self.micro_ops.push(MicroOp::PushLongLo);
                        // Advance PC for prefetch refill
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                    } else {
                        // Need to fetch second word - set up continuation
                        self.micro_ops.push(MicroOp::FetchExtWord);
                        self.instr_phase = InstrPhase::SrcEACalc;
                        self.src_mode = Some(addr_mode);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
                AddrMode::PcDisp => {
                    // PEA d16(PC) = 16 cycles: 8 (internal) + 8 (push)
                    // PC for calculation is where the extension word was (PC - 2)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let disp = self.next_ext_word() as i16 as i32;
                    let ea = (pc_at_ext as i32).wrapping_add(disp) as u32;
                    self.data = ea;
                    self.internal_cycles = 8;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    // Advance PC for prefetch refill
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                }
                AddrMode::PcIndex => {
                    // PEA d8(PC,Xn) = 20 cycles: 12 (internal) + 8 (push)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let ea = self.calc_index_ea(pc_at_ext);
                    self.data = ea;
                    self.internal_cycles = 12;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    // Advance PC for prefetch refill
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_pea_continuation(&mut self) {
        // Called after fetching second extension word for PEA AbsLong
        // FetchExtWord already advanced PC by 2, so we only add 2 more
        let hi = u32::from(self.ext_words[0]);
        let lo = u32::from(self.ext_words[1]);
        let ea = (hi << 16) | lo;
        self.data = ea;
        self.internal_cycles = 4; // Remaining internal time
        self.internal_advances_pc = false;
        self.micro_ops.push(MicroOp::Internal);
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
        // Advance PC by 2 (FetchExtWord already advanced by 2)
        self.regs.pc = self.regs.pc.wrapping_add(2);
        self.instr_phase = InstrPhase::Complete;
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
        self.addr2 = u32::from(ea_reg); // Store EA register for address update
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.instr_phase = InstrPhase::SrcEACalc;
        self.src_mode = AddrMode::decode(mode, ea_reg);
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_movem_to_mem_continuation(&mut self) {
        let mask = self.ext_words[0];
        if mask == 0 {
            // No registers to transfer
            self.queue_internal(8);
            self.instr_phase = InstrPhase::Initial;
            return;
        }

        let Some(addr_mode) = self.src_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Determine addressing mode and starting address
        let is_predec = matches!(addr_mode, AddrMode::AddrIndPreDec(_));
        self.movem_predec = is_predec;
        self.movem_postinc = false;
        self.movem_long_phase = 0;

        let (start_addr, ea_reg) = match addr_mode {
            AddrMode::AddrIndPreDec(r) => {
                // For predecrement, we start at An and decrement before each write
                // The actual writes happen at decremented addresses
                let count = mask.count_ones();
                let dec_per_reg = if self.size == Size::Long { 4 } else { 2 };
                let start = self.regs.a(r as usize).wrapping_sub(count * dec_per_reg);
                (start, r)
            }
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp) as u32, r)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, 0)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                ((u32::from(hi) << 16) | u32::from(lo), 0)
            }
            _ => {
                self.queue_internal(8);
                self.instr_phase = InstrPhase::Initial;
                return;
            }
        };

        self.addr = start_addr;
        self.addr2 = u32::from(ea_reg);

        // Find first register to write
        // For predecrement mode, the mask is reversed: bit 0 = A7, bit 15 = D0
        // We iterate from highest bit down so D0 is written first (to lowest address)
        // For other modes: bit 0 = D0, bit 15 = A7, iterate up
        let first_bit = if is_predec {
            self.find_first_movem_bit_down(mask)
        } else {
            self.find_first_movem_bit_up(mask)
        };

        if let Some(bit) = first_bit {
            self.data2 = bit as u32;
            self.micro_ops.push(MicroOp::MovemWrite);
        }

        self.instr_phase = InstrPhase::Initial;
    }

    fn exec_tas(&mut self, mode: u8, ea_reg: u8) {
        // TAS <ea> - Test And Set
        // Tests the byte, sets N and Z, then sets bit 7 (atomically on real hardware)
        // Format: 0100 1010 11 mode reg
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // For data register, test and set bit 7 of low byte
                    let value = (self.regs.d[r as usize] & 0xFF) as u8;
                    self.set_flags_move(u32::from(value), Size::Byte);
                    let new_value = value | 0x80;
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(new_value);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory operand - calculate EA then do read-modify-write
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.movem_long_phase = 0;
                    // Single micro-op handles both read and write phases
                    self.micro_ops.push(MicroOp::TasExecute);
                }
            }
        } else {
            self.illegal_instruction();
        }
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
                    // Memory operand - read and set flags
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = 0; // Not used for TST
                    self.data2 = 8; // 8 = TST
                    self.movem_long_phase = 0;
                    // Add internal cycles for EA calculation based on addressing mode
                    let ea_cycles = self.ea_calc_cycles(addr_mode);
                    if ea_cycles > 0 {
                        self.internal_cycles = ea_cycles;
                        self.micro_ops.push(MicroOp::Internal);
                    }
                    self.micro_ops.push(MicroOp::AluMemSrc);
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
        self.addr2 = u32::from(ea_reg); // Store EA register for address update
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.instr_phase = InstrPhase::DstEACalc;
        self.dst_mode = AddrMode::decode(mode, ea_reg);
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_movem_from_mem_continuation(&mut self) {
        let mask = self.ext_words[0];
        if mask == 0 {
            // No registers to transfer
            self.queue_internal(12);
            self.instr_phase = InstrPhase::Initial;
            return;
        }

        let Some(addr_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // For memory to registers, order is always D0-D7, A0-A7 (bits 0-15)
        let is_postinc = matches!(addr_mode, AddrMode::AddrIndPostInc(_));
        self.movem_predec = false;
        self.movem_postinc = is_postinc;
        self.movem_long_phase = 0;

        let (start_addr, ea_reg) = match addr_mode {
            AddrMode::AddrIndPostInc(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp) as u32, r)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, 0)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                ((u32::from(hi) << 16) | u32::from(lo), 0)
            }
            _ => {
                self.queue_internal(12);
                self.instr_phase = InstrPhase::Initial;
                return;
            }
        };

        self.addr = start_addr;
        self.addr2 = u32::from(ea_reg);

        // Find first register to read (always ascending for memory-to-registers)
        let first_bit = self.find_first_movem_bit_up(mask);

        if let Some(bit) = first_bit {
            self.data2 = bit as u32;
            self.micro_ops.push(MicroOp::MovemRead);
        }

        self.instr_phase = InstrPhase::Initial;
    }

    fn exec_trap(&mut self, op: u16) {
        let vector = 32 + (op & 0xF) as u8;
        // TRAP is a 1-word instruction.
        // After FetchOpcode: PC = opcode_addr + 2 (pointing to next instruction)
        // This is the correct return address to push.
        self.exception(vector);
    }

    fn exec_link(&mut self, reg: u8) {
        // LINK An,#displacement (16 cycles)
        // 1. Push An onto stack
        // 2. Copy SP to An (An becomes frame pointer)
        // 3. Add signed displacement to SP (allocate stack space)

        // Get displacement from prefetched extension word
        let disp = self.next_ext_word() as i16 as i32;

        // Push An
        let an_value = self.regs.a(reg as usize);
        self.data = an_value;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // Store reg and displacement for continuation
        self.addr2 = u32::from(reg);
        self.data2 = disp as u32;
        self.instr_phase = InstrPhase::SrcRead;
        self.micro_ops.push(MicroOp::Execute);

        // In normal mode, advance PC for prefetch refill (4 bytes: 2 for ext word + 2 for prefetch)
        // In prefetch_only mode, next_ext_word() already advanced PC by 2 for the ext word
        if !self.prefetch_only {
            self.regs.pc = self.regs.pc.wrapping_add(4);
        }
    }

    fn exec_link_continuation(&mut self) {
        // After push completes:
        // 1. Copy SP to An (frame pointer)
        // 2. Add displacement to SP

        let reg = self.addr2 as usize;
        let disp = self.data2 as i32;

        // SP now points to the pushed value
        let sp = self.regs.active_sp();
        self.regs.set_a(reg, sp);

        // Add displacement to SP
        let new_sp = (sp as i32).wrapping_add(disp) as u32;
        self.regs.set_active_sp(new_sp);

        self.instr_phase = InstrPhase::Complete;
        // 8 internal cycles for the remaining operations
        self.queue_internal_no_pc(8);
    }

    fn exec_unlk(&mut self, reg: u8) {
        // UNLK An (12 cycles)
        // 1. Copy An to SP (restore stack to frame pointer)
        // 2. Pop An from stack (restore old frame pointer)

        // Copy An to SP
        let an_value = self.regs.a(reg as usize);
        self.regs.set_active_sp(an_value);

        // Pop An from stack
        self.micro_ops.push(MicroOp::PopLongHi);
        self.micro_ops.push(MicroOp::PopLongLo);

        // Store which register to restore, set up continuation
        self.addr2 = u32::from(reg);
        self.instr_phase = InstrPhase::SrcRead;
        self.micro_ops.push(MicroOp::Execute);

        // Advance PC by 2 (single-word instruction)
        self.regs.pc = self.regs.pc.wrapping_add(2);
    }

    fn exec_unlk_continuation(&mut self) {
        // After pop completes: An = popped value (in self.data)
        let reg = self.addr2 as usize;
        self.regs.set_a(reg, self.data);

        self.instr_phase = InstrPhase::Complete;
        // 4 internal cycles for remaining operations
        self.queue_internal_no_pc(4);
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
            // TRAPV is a 1-word instruction.
            // PC already points past the instruction (correct return address).
            self.exception(7); // TRAPV exception
        } else {
            self.queue_internal(4);
        }
    }

    fn exec_rtr(&mut self) {
        // RTR = 20 cycles: 4 (pop CCR) + 8 (pop PC) + 8 (prefetch)
        // Pop CCR first, then save it before popping PC
        self.micro_ops.push(MicroOp::PopWord);
        self.instr_phase = InstrPhase::SrcRead; // Signal continuation
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_rtr_continuation(&mut self) {
        match self.instr_phase {
            InstrPhase::SrcRead => {
                // After PopWord: data contains CCR word
                // Save CCR to data2 before it's overwritten by PC pop
                self.data2 = self.data;
                // Now pop PC
                self.micro_ops.push(MicroOp::PopLongHi);
                self.micro_ops.push(MicroOp::PopLongLo);
                self.instr_phase = InstrPhase::DstWrite;
                self.micro_ops.push(MicroOp::Execute);
            }
            InstrPhase::DstWrite => {
                // After PopLong: data contains return PC, data2 contains CCR
                // Restore CCR (only low 5 bits - XNZVC, keeping upper SR bits)
                let ccr = (self.data2 & 0x1F) as u8;
                self.regs.set_ccr(ccr);
                // Set PC = return address + 4 (for prefetch)
                self.regs.pc = self.data.wrapping_add(4);
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal_no_pc(8); // Prefetch cycles
            }
            _ => {
                self.instr_phase = InstrPhase::Initial;
            }
        }
    }

    fn exec_jsr(&mut self, mode: u8, ea_reg: u8) {
        // JSR <ea> - Jump to Subroutine (push return address, then jump)
        // With prefetch model: PC already points past the instruction,
        // and ext_words[0] may contain the extension word from prefetch.
        //
        // After JSR completes, the 68000 fills its 2-word prefetch queue at the
        // new PC, which advances PC by 4. We simulate this by adding 4 to target.
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // JSR (An) = 16 cycles: 8 (push) + 8 (prefetch)
                    let target = self.regs.a(r as usize);
                    self.data = self.regs.pc; // Return address
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
                    self.queue_internal_no_pc(8); // Prefetch time
                }
                AddrMode::AddrIndDisp(r) => {
                    // JSR d16(An) = 18 cycles: 2 (EA calc) + 8 (push) + 8 (prefetch)
                    let return_addr = self.regs.pc; // Capture before consuming ext word
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                    self.data = return_addr;
                    self.internal_cycles = 2;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
                    self.queue_internal_no_pc(8);
                }
                AddrMode::AddrIndIndex(r) => {
                    // JSR d8(An,Xn) = 22 cycles: 6 (EA calc) + 8 (push) + 8 (prefetch)
                    let return_addr = self.regs.pc; // Capture before consuming ext word
                    let target = self.calc_index_ea(self.regs.a(r as usize));
                    self.data = return_addr;
                    self.internal_cycles = 6;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
                    self.queue_internal_no_pc(8);
                }
                AddrMode::AbsShort => {
                    // JSR addr.W = 18 cycles: 2 (EA) + 8 (push) + 8 (prefetch)
                    let return_addr = self.regs.pc; // Capture before consuming ext word
                    let target = self.next_ext_word() as i16 as i32 as u32;
                    self.data = return_addr;
                    self.internal_cycles = 2;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                AddrMode::AbsLong => {
                    // JSR addr.L = 20 cycles: 4 (fetch 2nd word) + 8 (push) + 8 (prefetch)
                    // Note: First word from prefetch, second word needs fetch
                    if self.ext_count >= 2 {
                        // Both words available (rare case)
                        let hi = u32::from(self.ext_words[0]);
                        let lo = u32::from(self.ext_words[1]);
                        let target = (hi << 16) | lo;
                        self.data = self.regs.pc;
                        self.micro_ops.push(MicroOp::PushLongHi);
                        self.micro_ops.push(MicroOp::PushLongLo);
                        self.regs.pc = target.wrapping_add(4); // Include prefetch
                        self.queue_internal_no_pc(8);
                    } else {
                        // Need to fetch second word - set up continuation
                        self.micro_ops.push(MicroOp::FetchExtWord);
                        self.instr_phase = InstrPhase::SrcEACalc;
                        self.src_mode = Some(addr_mode);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
                AddrMode::PcDisp => {
                    // JSR d16(PC) = 18 cycles: 2 (EA calc) + 8 (push) + 8 (prefetch)
                    // The "PC" for calculation is where the extension word was (PC - 2)
                    // Capture return address before consuming ext word
                    let return_addr = self.regs.pc;
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (pc_at_ext as i32).wrapping_add(disp) as u32;
                    self.data = return_addr;
                    self.internal_cycles = 2;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                AddrMode::PcIndex => {
                    // JSR d8(PC,Xn) = 22 cycles: 6 (EA calc) + 8 (push) + 8 (prefetch)
                    // Capture return address before consuming ext word
                    let return_addr = self.regs.pc;
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let target = self.calc_index_ea(pc_at_ext);
                    self.data = return_addr;
                    self.internal_cycles = 6;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_jsr_continuation(&mut self) {
        // Called after fetching second extension word for JSR AbsLong
        // At this point: ext_words[0] has high word (from prefetch),
        // ext_words[1] has low word (just fetched), PC is past both words
        let hi = u32::from(self.ext_words[0]);
        let lo = u32::from(self.ext_words[1]);
        let target = (hi << 16) | lo;

        // Return address is current PC (past both extension words)
        self.data = self.regs.pc;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
        self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal_no_pc(8); // Prefetch time
    }

    fn exec_jmp(&mut self, mode: u8, ea_reg: u8) {
        // JMP <ea> - Jump to address
        // With prefetch model: PC already points past the instruction,
        // and ext_words[0] may contain the extension word from prefetch.
        // After JMP, the 68000 fills its prefetch queue at the new PC,
        // advancing PC by 4.
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // JMP (An) = 8 cycles (prefetch only)
                    let target = self.regs.a(r as usize);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
                    self.queue_internal_no_pc(8);
                }
                AddrMode::AddrIndDisp(r) => {
                    // JMP d16(An) = 10 cycles: 2 (EA) + 8 (prefetch)
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                    self.internal_cycles = 2;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                AddrMode::AddrIndIndex(r) => {
                    // JMP d8(An,Xn) = 14 cycles: 6 (EA) + 8 (prefetch)
                    let target = self.calc_index_ea(self.regs.a(r as usize));
                    self.internal_cycles = 6;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                AddrMode::AbsShort => {
                    // JMP addr.W = 10 cycles: 2 (EA) + 8 (prefetch)
                    let target = self.next_ext_word() as i16 as i32 as u32;
                    self.internal_cycles = 2;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                AddrMode::AbsLong => {
                    // JMP addr.L = 12 cycles: 4 (fetch 2nd word) + 8 (prefetch)
                    if self.ext_count >= 2 {
                        // Both words available (rare case)
                        let hi = u32::from(self.ext_words[0]);
                        let lo = u32::from(self.ext_words[1]);
                        let target = (hi << 16) | lo;
                        self.regs.pc = target.wrapping_add(4); // Include prefetch
                        self.queue_internal_no_pc(8);
                    } else {
                        // Need to fetch second word - set up continuation
                        self.micro_ops.push(MicroOp::FetchExtWord);
                        self.instr_phase = InstrPhase::SrcEACalc;
                        self.src_mode = Some(addr_mode);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
                AddrMode::PcDisp => {
                    // JMP d16(PC) = 10 cycles: 2 (EA) + 8 (prefetch)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (pc_at_ext as i32).wrapping_add(disp) as u32;
                    self.internal_cycles = 2;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                AddrMode::PcIndex => {
                    // JMP d8(PC,Xn) = 14 cycles: 6 (EA) + 8 (prefetch)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let target = self.calc_index_ea(pc_at_ext);
                    self.internal_cycles = 6;
                    self.internal_advances_pc = false;
                    self.micro_ops.push(MicroOp::Internal);
                    self.regs.pc = target.wrapping_add(4); // Include prefetch
                    self.queue_internal_no_pc(8);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_jmp_continuation(&mut self) {
        // Called after fetching second extension word for JMP AbsLong
        // JMP addr.L = 12 cycles total: 4 (fetch 2nd word) + 8 (prefetch)
        // At this point we've already fetched (4 cycles), just need prefetch (8 cycles)
        let hi = u32::from(self.ext_words[0]);
        let lo = u32::from(self.ext_words[1]);
        let target = (hi << 16) | lo;

        self.regs.pc = target.wrapping_add(4); // Include prefetch in PC
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal_no_pc(8); // Prefetch time
    }

    fn exec_chk(&mut self, op: u16) {
        // CHK <ea>,Dn - Check register against bounds
        // If Dn < 0 or Dn > <ea>, trigger CHK exception
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Get the data register value (word operation)
                    let dn = self.regs.d[reg as usize] as i16;
                    let upper_bound = self.regs.d[r as usize] as i16;

                    // Check bounds
                    if dn < 0 {
                        // N flag set for negative, clear N/Z/V/C then set N
                        // X is not affected by CHK
                        self.regs.sr &= !(N | Z | V | C);
                        self.regs.sr |= N;
                        self.exception(6); // CHK exception
                    } else if dn > upper_bound {
                        // N flag clear for upper bound violation, clear N/Z/V/C
                        // X is not affected by CHK
                        self.regs.sr &= !(N | Z | V | C);
                        self.exception(6); // CHK exception
                    } else {
                        // Value is within bounds, no exception
                        self.queue_internal(10);
                    }
                }
                AddrMode::Immediate => {
                    // Need to fetch immediate - simplified for now
                    self.queue_internal(10);
                }
                _ => {
                    // Memory source - use AluMemSrc with CHK operation
                    self.size = Size::Word; // CHK always operates on word
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg); // Data register to check
                    self.data2 = 9; // 9 = CHK
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
        } else {
            self.illegal_instruction();
        }
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
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = imm; // Immediate value as source
                    self.data2 = 0; // 0 = ADD
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
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
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = imm; // Immediate value as source
                    self.data2 = 1; // 1 = SUB
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_scc(&mut self, condition: u8, mode: u8, ea_reg: u8) {
        // Scc <ea> - Set byte to $FF if condition true, $00 if false
        let value: u8 = if Status::condition(self.regs.sr, condition) {
            0xFF
        } else {
            0x00
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Set low byte of register
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(value);
                    // 4 cycles if false, 6 if true
                    self.queue_internal(if value == 0xFF { 6 } else { 4 });
                }
                _ => {
                    // Memory destination - write byte
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(value);
                    self.micro_ops.push(MicroOp::WriteByte);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_dbcc(&mut self, condition: u8, reg: u8) {
        // DBcc Dn, label - Decrement and branch
        // If condition is true, no branch (fall through past displacement word)
        // If condition is false, decrement Dn.W and branch if Dn != -1
        // Note: DBcc ALWAYS has word displacement following the opcode

        if self.prefetch_only {
            // In prefetch_only mode, displacement is already preloaded
            let disp = self.next_ext_word() as i16 as i32;
            self.exec_dbcc_with_disp(condition, reg, disp);
        } else {
            // Store condition and register for continuation
            // data2: bits 0-3 = reg, bits 4-7 = condition
            self.data2 = u32::from(reg) | (u32::from(condition) << 4);
            self.instr_phase = InstrPhase::SrcRead;
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Execute DBcc with known displacement.
    fn exec_dbcc_with_disp(&mut self, condition: u8, reg: u8, disp: i32) {
        use crate::flags::Status;

        if Status::condition(self.regs.sr, condition) {
            // Condition true - no branch, fall through past displacement
            // PC needs to advance +2 for prefetch of next instruction
            self.queue_internal(12); // Condition true, no loop (internal advances PC)
        } else {
            // Condition false - check if we would branch
            let val = (self.regs.d[reg as usize] & 0xFFFF) as i16;
            let new_val = val.wrapping_sub(1);

            if new_val == -1 {
                // Counter exhausted - no branch, fall through
                self.regs.d[reg as usize] =
                    (self.regs.d[reg as usize] & 0xFFFF_0000) | (new_val as u16 as u32);
                // PC needs to advance +2 for prefetch of next instruction
                self.queue_internal(14); // Loop terminated (internal advances PC)
            } else {
                // Counter not exhausted - branch
                // Displacement is relative to PC after opcode (before displacement word)
                // But next_ext_word already advanced PC past the displacement, so:
                // target = (PC - 2) + disp, where PC is now past displacement
                // Actually: target = (opcode_addr + 2) + disp
                // PC is now at opcode_addr + 4 (after consuming displacement via next_ext_word)
                let target = ((self.regs.pc as i32) - 2 + disp) as u32;

                // Check for odd branch target
                if target & 1 != 0 {
                    self.address_error(target, true, true);
                    return;
                }

                // Decrement and branch
                self.regs.d[reg as usize] =
                    (self.regs.d[reg as usize] & 0xFFFF_0000) | (new_val as u16 as u32);
                // After branch, PC = target + 4 to account for prefetch refill
                self.regs.pc = target.wrapping_add(4);
                self.queue_internal_no_pc(10); // Loop continues
            }
        }
    }

    /// DBcc continuation after fetching displacement word.
    fn dbcc_continuation(&mut self) {
        use crate::cpu::InstrPhase;

        if self.instr_phase != InstrPhase::SrcRead {
            self.instr_phase = InstrPhase::Complete;
            return;
        }

        let reg = (self.data2 & 0xF) as usize;
        let condition = ((self.data2 >> 4) & 0xF) as u8;
        let disp = self.ext_words[0] as i16 as i32;

        self.instr_phase = InstrPhase::Complete;

        if Status::condition(self.regs.sr, condition) {
            // Condition true - no branch, PC is already past the displacement word
            self.queue_internal_no_pc(12); // Condition true, no loop
        } else {
            // Condition false - check if we would branch
            let val = (self.regs.d[reg] & 0xFFFF) as i16;
            let new_val = val.wrapping_sub(1);

            if new_val == -1 {
                // Counter will be exhausted - no branch, fall through
                self.regs.d[reg] =
                    (self.regs.d[reg] & 0xFFFF_0000) | (new_val as u16 as u32);
                self.queue_internal_no_pc(14); // Loop terminated
            } else {
                // Counter not exhausted - would branch
                // Calculate branch target: displacement is relative to PC after opcode
                // PC is now at opcode+4 (past displacement), so target = PC - 2 + disp
                let target = ((self.regs.pc as i32) - 2 + disp) as u32;

                // Check for odd branch target - this causes address error
                if target & 1 != 0 {
                    self.address_error(target, true, true); // read, instruction fetch
                    return;
                }

                // Target is valid - now we can decrement and branch
                self.regs.d[reg] =
                    (self.regs.d[reg] & 0xFFFF_0000) | (new_val as u16 as u32);
                self.regs.pc = target.wrapping_add(4); // Include prefetch
                self.queue_internal_no_pc(10); // Loop continues
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
                    // Memory source - use AluMemSrc
                    self.size = Size::Word; // DIVU reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 12; // 12 = DIVU
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
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
                    // Memory source - use AluMemSrc
                    self.size = Size::Word; // DIVS reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 13; // 13 = DIVS
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_sbcd(&mut self, op: u16) {
        // SBCD - Subtract Decimal with Extend (packed BCD subtraction)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        // Format: 1000 Ry 10000 R Rx (R=0 register, R=1 memory)
        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0;

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = Size::Byte;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 1; // 1 = SBCD
            self.movem_long_phase = 0;
            self.extend_predec_done = false;
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dy - Dx - X -> Dy
            let src = self.regs.d[rx] as u8;
            let dst = self.regs.d[ry] as u8;
            let x = u8::from(self.regs.sr & X != 0);

            let (result, borrow) = self.bcd_sub(dst, src, x);

            // Write result to low byte of Dy
            self.regs.d[ry] = (self.regs.d[ry] & 0xFFFF_FF00) | u32::from(result);

            // Set flags
            let mut sr = self.regs.sr;
            // Z: cleared if non-zero, unchanged otherwise
            if result != 0 {
                sr &= !Z;
            }
            // C and X: set if decimal borrow
            sr = Status::set_if(sr, C, borrow);
            sr = Status::set_if(sr, X, borrow);
            // N: undefined, but set based on MSB for consistency
            sr = Status::set_if(sr, N, result & 0x80 != 0);
            // V: undefined
            self.regs.sr = sr;

            self.queue_internal(6);
        }
    }

    /// Perform packed BCD subtraction: dst - src - extend.
    /// Returns (result, borrow).
    pub(crate) fn bcd_sub(&self, dst: u8, src: u8, extend: u8) -> (u8, bool) {
        // Subtract low nibbles
        let low_dst = i16::from(dst & 0x0F);
        let low_src = i16::from(src & 0x0F) + i16::from(extend);
        let mut low = low_dst - low_src;

        // Track borrow from low nibble
        let low_borrow = low < 0;
        if low_borrow {
            low += 10; // BCD correction
        }

        // Subtract high nibbles
        let high_dst = i16::from(dst >> 4);
        let high_src = i16::from(src >> 4) + i16::from(low_borrow);
        let mut high = high_dst - high_src;

        // Track borrow from high nibble
        let borrow = high < 0;
        if borrow {
            high += 10; // BCD correction
        }

        let result = ((high as u8 & 0x0F) << 4) | (low as u8 & 0x0F);
        (result, borrow)
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
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst | src;
                        self.write_data_reg(r, result, size);
                        self.set_flags_move(result, size); // OR sets N,Z, clears V,C
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 3; // 3 = OR
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
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
                        // Memory source
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 3; // 3 = OR
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
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
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(r, result, size);
                        self.set_flags_sub(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 1; // 1 = SUB
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
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
                    AddrMode::AddrReg(r) => {
                        // SUB.W/L An,Dn
                        let src = self.regs.a(r as usize);
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
                        // Memory source
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 1; // 1 = SUB
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
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
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 6; // 6 = SUBA
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_subx(&mut self, op: u16) {
        // SUBX - Subtract with extend (for multi-precision arithmetic)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0; // Register/Memory flag

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = size;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 3; // 3 = SUBX
            self.movem_long_phase = 0;
            self.extend_predec_done = false;
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dx,Dy
            let src = self.read_data_reg(rx as u8, size);
            let dst = self.read_data_reg(ry as u8, size);
            let x = u32::from(self.regs.sr & X != 0);

            let result = dst.wrapping_sub(src).wrapping_sub(x);
            self.write_data_reg(ry as u8, result, size);

            // SUBX flags (like SUB but Z is only cleared, never set)
            let (src_masked, dst_masked, result_masked, msb) = match size {
                Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80u32),
                Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
                Size::Long => (src, dst, result, 0x8000_0000),
            };

            let mut sr = self.regs.sr;
            sr = Status::set_if(sr, N, result_masked & msb != 0);
            // Z: cleared if non-zero, unchanged if zero
            if result_masked != 0 {
                sr &= !Z;
            }
            // V: overflow
            let overflow = ((dst_masked ^ src_masked) & (dst_masked ^ result_masked) & msb) != 0;
            sr = Status::set_if(sr, V, overflow);
            // C: borrow
            let carry = src_masked.wrapping_add(x) > dst_masked
                || (src_masked == dst_masked && x != 0);
            sr = Status::set_if(sr, C, carry);
            sr = Status::set_if(sr, X, carry);

            self.regs.sr = sr;
            self.queue_internal(if size == Size::Long { 8 } else { 4 });
        }
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
                    // CMP #imm,Dn is encoded as CMPI (different opcode)
                    self.illegal_instruction();
                }
                _ => {
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 4; // 4 = CMP
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
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
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 7; // 7 = CMPA
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_cmpm(&mut self, op: u16) {
        // CMPM (Ay)+,(Ax)+ - Compare memory with postincrement
        // Format: 1011 Ax 1 ss 001 Ay
        let ay = (op & 7) as usize;
        let ax = ((op >> 9) & 7) as usize;
        let size = Size::from_bits(((op >> 6) & 3) as u8);

        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        // Set up state for tick_cmpm_execute
        self.size = size;
        self.addr = self.regs.a(ay);   // Source address
        self.addr2 = self.regs.a(ax);  // Destination address
        self.data = ay as u32;         // Source register number
        self.data2 = ax as u32;        // Destination register number
        self.movem_long_phase = 0;     // Start at phase 0

        // Queue the CMPM micro-op (two phases: read src, read dst + compare)
        self.micro_ops.push(MicroOp::CmpmExecute);
        self.micro_ops.push(MicroOp::CmpmExecute);
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
                    // Memory destination: read-modify-write
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = self.read_data_reg(reg, size);
                    self.data2 = 4; // 4 = EOR
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
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

                    // Timing: 38 + 2*number of 1-bits in source operand
                    let ones = (src as u16).count_ones() as u8;
                    self.queue_internal(38 + 2 * ones);
                }
                _ => {
                    // Memory source - use AluMemSrc
                    self.size = Size::Word; // MULU reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 10; // 10 = MULU
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
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

                    // Timing: 38 + 2*number of 1-bits in <ea> XOR'd with sign bit
                    // For negative source, effectively count bit transitions
                    let src16 = self.regs.d[r as usize] as u16;
                    let pattern = src16 ^ (src16 << 1); // Count bit transitions
                    let ones = pattern.count_ones() as u8;
                    self.queue_internal(38 + 2 * ones);
                }
                _ => {
                    // Memory source - use AluMemSrc
                    self.size = Size::Word; // MULS reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 11; // 11 = MULS
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_abcd(&mut self, op: u16) {
        // ABCD - Add Decimal with Extend (packed BCD addition)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        // Format: 1100 Ry 10000 R Rx (R=0 register, R=1 memory)
        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0;

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = Size::Byte;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 0; // 0 = ABCD
            self.movem_long_phase = 0;
            self.extend_predec_done = false;
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dx,Dy
            let src = self.regs.d[rx] as u8;
            let dst = self.regs.d[ry] as u8;
            let x = u8::from(self.regs.sr & X != 0);

            let (result, carry) = self.bcd_add(src, dst, x);

            // Write result to low byte of Dy
            self.regs.d[ry] = (self.regs.d[ry] & 0xFFFF_FF00) | u32::from(result);

            // Set flags
            let mut sr = self.regs.sr;
            // Z: cleared if non-zero, unchanged otherwise
            if result != 0 {
                sr &= !Z;
            }
            // C and X: set if decimal carry
            sr = Status::set_if(sr, C, carry);
            sr = Status::set_if(sr, X, carry);
            // N: undefined, but set based on MSB for consistency
            sr = Status::set_if(sr, N, result & 0x80 != 0);
            // V: undefined
            self.regs.sr = sr;

            self.queue_internal(6);
        }
    }

    /// Perform packed BCD addition: src + dst + extend.
    /// Returns (result, carry).
    pub(crate) fn bcd_add(&self, src: u8, dst: u8, extend: u8) -> (u8, bool) {
        // Add low nibbles
        let mut low = (dst & 0x0F) + (src & 0x0F) + extend;
        let mut carry = false;

        // BCD correction for low nibble
        if low > 9 {
            low += 6;
        }

        // Add high nibbles plus carry from low
        let mut high = (dst >> 4) + (src >> 4) + u8::from(low > 0x0F);

        // BCD correction for high nibble
        if high > 9 {
            high += 6;
            carry = true;
        }

        let result = ((high & 0x0F) << 4) | (low & 0x0F);
        (result, carry)
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
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst & src;
                        self.write_data_reg(r, result, size);
                        self.set_flags_move(result, size); // AND sets N,Z, clears V,C
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 2; // 2 = AND
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
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
                        // Memory source
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 2; // 2 = AND
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
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
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(r, result, size);
                        self.set_flags_add(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 0; // 0 = ADD
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
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
                    AddrMode::AddrReg(r) => {
                        // ADD.W/L An,Dn
                        let src = self.regs.a(r as usize);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_add(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::Immediate => {
                        // ADD #imm,Dn is encoded as ADDI (different opcode)
                        self.illegal_instruction();
                    }
                    _ => {
                        // Memory source: read from memory, add to register
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 0; // 0 = ADD
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
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
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 5; // 5 = ADDA
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_addx(&mut self, op: u16) {
        // ADDX - Add with extend (for multi-precision arithmetic)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0; // Register/Memory flag

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = size;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 2; // 2 = ADDX
            self.movem_long_phase = 0;
            // Mark that registers haven't been pre-decremented yet
            self.extend_predec_done = false;
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dx,Dy
            let src = self.read_data_reg(rx as u8, size);
            let dst = self.read_data_reg(ry as u8, size);
            let x = u32::from(self.regs.sr & X != 0);

            let result = dst.wrapping_add(src).wrapping_add(x);
            self.write_data_reg(ry as u8, result, size);

            // ADDX flags (like ADD but Z is only cleared, never set)
            let (src_masked, dst_masked, result_masked, msb) = match size {
                Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80u32),
                Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
                Size::Long => (src, dst, result, 0x8000_0000),
            };

            let mut sr = self.regs.sr;
            sr = Status::set_if(sr, N, result_masked & msb != 0);
            // Z: cleared if non-zero, unchanged if zero
            if result_masked != 0 {
                sr &= !Z;
            }
            // V: overflow (same signs in, different sign out)
            let overflow = (!(src_masked ^ dst_masked) & (src_masked ^ result_masked) & msb) != 0;
            sr = Status::set_if(sr, V, overflow);
            // C: carry out
            let carry = match size {
                Size::Byte => {
                    (u16::from(src as u8) + u16::from(dst as u8) + u16::from(x as u8)) > 0xFF
                }
                Size::Word => {
                    (u32::from(src as u16) + u32::from(dst as u16) + x) > 0xFFFF
                }
                Size::Long => src.checked_add(dst).and_then(|v| v.checked_add(x)).is_none(),
            };
            sr = Status::set_if(sr, C, carry);
            sr = Status::set_if(sr, X, carry);

            self.regs.sr = sr;
            self.queue_internal(if size == Size::Long { 8 } else { 4 });
        }
    }

    fn exec_shift_mem(&mut self, kind: u8, direction: bool, mode: u8, ea_reg: u8) {
        // Memory shift/rotate - always word size, always shift by 1
        // Format: 1110 kind dr 11 mode reg
        // kind: 00=AS, 01=LS, 10=ROX, 11=RO
        // dr: 0=right, 1=left
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(_) | AddrMode::AddrReg(_) => {
                    // Memory only - registers use exec_shift_reg
                    self.illegal_instruction();
                }
                _ => {
                    // Memory operand - calculate EA then do read-modify-write
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(kind);
                    self.data2 = u32::from(direction);
                    self.movem_long_phase = 0;
                    // Single micro-op handles both read and write phases
                    self.micro_ops.push(MicroOp::ShiftMemExecute);
                }
            }
        } else {
            self.illegal_instruction();
        }
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
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        let c = if count == bits {
                            value & 1 != 0
                        } else {
                            false
                        };
                        (0, c)
                    } else {
                        let shifted = (value << count) & mask;
                        let c = (value >> (bits - count)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // ASR - Arithmetic shift right (sign extends)
            (0, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let sign_bit = value & msb_bit != 0;
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    // For large counts, result is all sign bits
                    if count >= bits {
                        let result = if sign_bit { mask } else { 0 };
                        (result, sign_bit)
                    } else {
                        let mut result = value;
                        for _ in 0..count {
                            result = (result >> 1) | if sign_bit { msb_bit } else { 0 };
                        }
                        let c = (value >> (count - 1)) & 1 != 0;
                        (result & mask, c)
                    }
                }
            }
            // LSL - Logical shift left
            (1, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        // All bits shifted out, carry is last bit shifted out
                        let c = if count == bits {
                            value & 1 != 0
                        } else {
                            false
                        };
                        (0, c)
                    } else {
                        let shifted = (value << count) & mask;
                        let c = (value >> (bits - count)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // LSR - Logical shift right
            (1, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        // All bits shifted out
                        let c = if count == bits {
                            (value >> (bits - 1)) & 1 != 0
                        } else {
                            false
                        };
                        (0, c)
                    } else {
                        let shifted = (value >> count) & mask;
                        let c = (value >> (count - 1)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // ROXL - Rotate through X left
            // X flag is included as bit in the rotation chain
            (2, true) => {
                if count == 0 {
                    // Count 0: result unchanged, C = X
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    // Rotation is through (bits + 1) positions (including X)
                    let total_bits = bits + 1;
                    let count = count % total_bits;
                    if count == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        // Build extended value: X in position 'bits', data in lower bits
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        // Rotate left through the (bits+1) wide value
                        let rotated = ((extended << count) | (extended >> (total_bits - count)))
                            & ((1u64 << total_bits) - 1);
                        // Extract result and new X
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
            }
            // ROXR - Rotate through X right
            // X flag is included as bit in the rotation chain
            (2, false) => {
                if count == 0 {
                    // Count 0: result unchanged, C = X
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    // Rotation is through (bits + 1) positions (including X)
                    let total_bits = bits + 1;
                    let count = count % total_bits;
                    if count == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        // Build extended value: X in position 'bits', data in lower bits
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        // Rotate right through the (bits+1) wide value
                        let rotated = ((extended >> count) | (extended << (total_bits - count)))
                            & ((1u64 << total_bits) - 1);
                        // Extract result and new X
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
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
                    let eff_count = count % bits;
                    if eff_count == 0 {
                        // Full rotation, value unchanged, carry is LSB
                        (value, value & 1 != 0)
                    } else {
                        let rotated = ((value << eff_count) | (value >> (bits - eff_count))) & mask;
                        let c = rotated & 1 != 0;
                        (rotated, c)
                    }
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
                    let eff_count = count % bits;
                    if eff_count == 0 {
                        // Full rotation, value unchanged, carry is MSB
                        (value, value & msb_bit != 0)
                    } else {
                        let rotated = ((value >> eff_count) | (value << (bits - eff_count))) & mask;
                        let c = (value >> (eff_count - 1)) & 1 != 0;
                        (rotated, c)
                    }
                }
            }
            _ => (value, false),
        };

        self.write_data_reg(reg, result, size);

        // Set flags
        // N and Z based on result
        self.set_flags_move(result, size);

        // C flag is last bit shifted out (or cleared if count=0 for non-ROX)
        // X flag is set same as C for shifts and ROXL/ROXR, unchanged for ROL/ROR
        if count > 0 {
            self.regs.sr = Status::set_if(self.regs.sr, C, carry);
            // X is set for shifts (kind 0,1) and ROXL/ROXR (kind 2), not ROL/ROR (kind 3)
            if kind < 3 {
                self.regs.sr = Status::set_if(self.regs.sr, X, carry);
            }
        } else {
            // Count=0: For ROXL/ROXR, C = X; for others, C is cleared
            if kind == 2 {
                let x = self.regs.sr & X != 0;
                self.regs.sr = Status::set_if(self.regs.sr, C, x);
            } else {
                self.regs.sr &= !C;
            }
        }

        // V is cleared except for ASL where it can be set
        // Simplified: always clear V for now
        self.regs.sr &= !crate::flags::V;

        // Timing: 6 + 2*count for byte/word, 8 + 2*count for long
        let base_cycles = if size == Size::Long { 8 } else { 6 };
        self.queue_internal(base_cycles + 2 * count as u8);
    }
}
