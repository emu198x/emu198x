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
use crate::flags::Status;
use crate::microcode::MicroOp;

impl M68000 {
    /// Decode and execute the current instruction.
    pub(super) fn decode_and_execute(&mut self) {
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
    // These are stubs that will be filled in with proper implementations

    fn exec_move(&mut self, size: Size, src_mode: u8, src_reg: u8, dst_mode: u8, dst_reg: u8) {
        let src = AddrMode::decode(src_mode, src_reg);
        let dst = AddrMode::decode(dst_mode, dst_reg);

        if let (Some(src_mode), Some(dst_mode)) = (src, dst) {
            self.src_mode = Some(src_mode);
            self.dst_mode = Some(dst_mode);
            self.size = size;

            // Simple case: register to register
            match (src_mode, dst_mode) {
                (AddrMode::DataReg(s), AddrMode::DataReg(d)) => {
                    let value = match size {
                        Size::Byte => self.regs.d[s as usize] & 0xFF,
                        Size::Word => self.regs.d[s as usize] & 0xFFFF,
                        Size::Long => self.regs.d[s as usize],
                    };
                    self.regs.d[d as usize] = match size {
                        Size::Byte => {
                            (self.regs.d[d as usize] & 0xFFFF_FF00) | value
                        }
                        Size::Word => {
                            (self.regs.d[d as usize] & 0xFFFF_0000) | value
                        }
                        Size::Long => value,
                    };
                    self.set_flags_move(value, size);
                    self.queue_internal(4); // Basic timing
                }
                (AddrMode::AddrReg(s), AddrMode::AddrReg(d)) => {
                    let value = self.regs.a(s as usize);
                    self.regs.set_a(d as usize, value);
                    // MOVEA doesn't affect flags
                    self.queue_internal(4);
                }
                (AddrMode::Immediate, AddrMode::DataReg(d)) => {
                    // Queue fetching the immediate value
                    match size {
                        Size::Byte | Size::Word => {
                            self.micro_ops.push(MicroOp::FetchExtWord);
                        }
                        Size::Long => {
                            self.micro_ops.push(MicroOp::FetchExtWord);
                            self.micro_ops.push(MicroOp::FetchExtWord);
                        }
                    }
                    // Will need follow-up execution
                    self.dst_mode = Some(AddrMode::DataReg(d));
                }
                _ => {
                    // For other addressing modes, queue the EA calculations
                    self.queue_ea_read(src_mode, size);
                }
            }
        } else {
            self.illegal_instruction();
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

    // Stub implementations for remaining instructions
    fn exec_btst_reg(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_bchg_reg(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_bclr_reg(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_bset_reg(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
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

    fn exec_neg(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_move_to_sr(&mut self, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_not(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
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

    fn exec_tst(&mut self, _size: Option<Size>, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
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

    fn exec_addq(&mut self, _size: Option<Size>, _data: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_subq(&mut self, _size: Option<Size>, _data: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_scc(&mut self, _condition: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_dbcc(&mut self, _condition: u8, _reg: u8) {
        self.queue_internal(4); // TODO: implement
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

    fn exec_or(&mut self, _size: Option<Size>, _reg: u8, _mode: u8, _ea_reg: u8, _to_ea: bool) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_sub(
        &mut self,
        _size: Option<Size>,
        _reg: u8,
        _mode: u8,
        _ea_reg: u8,
        _to_ea: bool,
    ) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_suba(&mut self, _size: Size, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_subx(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_cmp(&mut self, _size: Option<Size>, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_cmpa(&mut self, _size: Size, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_cmpm(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_eor(&mut self, _size: Option<Size>, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_mulu(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_muls(&mut self, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
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
        _size: Option<Size>,
        _reg: u8,
        _mode: u8,
        _ea_reg: u8,
        _to_ea: bool,
    ) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_add(
        &mut self,
        _size: Option<Size>,
        _reg: u8,
        _mode: u8,
        _ea_reg: u8,
        _to_ea: bool,
    ) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_adda(&mut self, _size: Size, _reg: u8, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_addx(&mut self, _op: u16) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_shift_mem(&mut self, _kind: u8, _direction: bool, _mode: u8, _ea_reg: u8) {
        self.queue_internal(4); // TODO: implement
    }

    fn exec_shift_reg(
        &mut self,
        _kind: u8,
        _direction: bool,
        _count_or_reg: u8,
        _reg: u8,
        _size: Option<Size>,
        _immediate: bool,
    ) {
        self.queue_internal(4); // TODO: implement
    }
}
