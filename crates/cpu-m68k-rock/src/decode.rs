//! Instruction decoding and follow-up state machine for the 68000.
//!
//! The 68000 executes instructions in multiple phases tracked by follow-up
//! tags. The first call to `decode_and_execute` decodes the opcode in IR
//! and sets up the initial follow-up tag. Subsequent calls route through
//! `continue_instruction` which advances through the tag state machine:
//!
//! ```text
//! FETCH_SRC_EA -> FETCH_SRC_DATA -> FETCH_DST_EA -> FETCH_DST_DATA
//!     -> EXECUTE -> WRITEBACK
//! ```
//!
//! Not every instruction uses all phases. MOVEQ has no follow-ups at all.
//! LEA skips data fetch. CMP skips writeback. The tag values let each
//! instruction define its own path through the pipeline.

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::bus::M68kBus;
use crate::cpu::{
    AluOp, Cpu68000, State,
    TAG_AE_FETCH_VECTOR, TAG_AE_FINISH, TAG_AE_PUSH_FAULT, TAG_AE_PUSH_INFO,
    TAG_AE_PUSH_IR, TAG_AE_PUSH_SR,
    TAG_BCC_EXECUTE, TAG_BSR_EXECUTE, TAG_DATA_DST_LONG,
    TAG_DATA_SRC_LONG, TAG_DBCC_EXECUTE, TAG_EA_DST_DISP, TAG_EA_DST_LONG,
    TAG_EA_DST_PCDISP, TAG_EA_SRC_DISP, TAG_EA_SRC_LONG, TAG_EA_SRC_PCDISP,
    TAG_EXC_FETCH_VECTOR, TAG_EXC_FINISH, TAG_EXC_STACK_PC_HI,
    TAG_EXC_STACK_PC_LO, TAG_EXC_STACK_SR, TAG_EXECUTE, TAG_FETCH_DST_DATA,
    TAG_FETCH_DST_EA, TAG_FETCH_SRC_DATA, TAG_FETCH_SRC_EA, TAG_JSR_EXECUTE,
    TAG_JSR_JUMP, TAG_RTS_PC_HI, TAG_RTS_PC_LO, TAG_WRITEBACK,
};
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Decode the opcode in IR and begin execution.
    ///
    /// If `in_followup` is set, routes to `continue_instruction` instead.
    /// Otherwise, decodes the opcode and sets up the initial follow-up tag
    /// and micro-op sequence.
    pub fn decode_and_execute<B: M68kBus>(&mut self, bus: &mut B) {
        if self.in_followup {
            self.continue_instruction(bus);
            return;
        }

        let opcode = self.ir;

        // --- MOVE.b/w/l (0x1xxx, 0x2xxx, 0x3xxx) ---
        // Top 2 bits = 00, next 2 bits encode size (non-zero)
        if (opcode & 0xC000) == 0 && (opcode & 0x3000) != 0 {
            let size = match (opcode >> 12) & 3 {
                1 => Size::Byte,
                2 => Size::Long,
                3 => Size::Word,
                _ => unreachable!(),
            };
            let src_mode_bits = ((opcode >> 3) & 7) as u8;
            let src_reg = (opcode & 7) as u8;
            let dst_reg = ((opcode >> 9) & 7) as u8;
            let dst_mode_bits = ((opcode >> 6) & 7) as u8;

            self.size = size;
            self.src_mode = AddrMode::decode(src_mode_bits, src_reg);
            self.dst_mode = AddrMode::decode(dst_mode_bits, dst_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- CMPM (1011 Rx 1 ss 001 Ry) — must check before general EOR ---
        if (opcode & 0xF000) == 0xB000 {
            let opmode = (opcode >> 6) & 7;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;

            // CMPM: opmodes 4-6 with EA mode 001 (postincrement)
            if opmode >= 4 && opmode <= 6 && ea_mode_bits == 1 {
                let rx = ((opcode >> 9) & 7) as u8;
                let ry = (opcode & 7) as u8;
                let size = match opmode {
                    4 => Size::Byte,
                    5 => Size::Word,
                    6 => Size::Long,
                    _ => unreachable!(),
                };

                self.alu_op = AluOp::Cmp;
                self.size = size;
                self.src_mode = Some(AddrMode::AddrIndPostInc(ry));
                self.dst_mode = Some(AddrMode::AddrIndPostInc(rx));
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_SRC_EA;
                self.continue_instruction(bus);
                return;
            }
        }

        // --- ADD/SUB/CMP/EOR/AND/OR (general register-EA form) ---
        // 0xBxxx: CMP (opmodes 0-2), EOR (opmodes 4-6), CMPA (3/7).
        if matches!(opcode & 0xF000, 0xD000 | 0x9000 | 0xB000 | 0xC000 | 0x8000) {
            let reg = ((opcode >> 9) & 7) as u8;
            let opmode = (opcode >> 6) & 7;

            let op = match opcode & 0xF000 {
                0xD000 => AluOp::Add,
                0x9000 => AluOp::Sub,
                0xB000 => if opmode >= 4 && opmode <= 6 { AluOp::Eor } else { AluOp::Cmp },
                0xC000 => AluOp::And,
                0x8000 => AluOp::Or,
                _ => unreachable!(),
            };

            // Handle all opmodes: 0-2 = EA→Dn, 4-6 = Dn→EA, 3/7 = ADDA/SUBA/CMPA
            if (opmode != 3 && opmode != 7) || matches!(opcode & 0xF000, 0xD000 | 0x9000 | 0xB000) {
                let size = match opmode {
                    0 | 4 => Size::Byte,
                    1 | 5 | 3 => Size::Word,
                    2 | 6 | 7 => Size::Long,
                    _ => unreachable!(),
                };
                let to_reg = opmode <= 2;
                let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                let ea_reg = (opcode & 7) as u8;
                let ea_mode = AddrMode::decode(ea_mode_bits, ea_reg).unwrap();

                // ADDA/SUBA/CMPA: source is EA, destination is An
                let is_addr = matches!(opmode, 3 | 7)
                    && matches!(opcode & 0xF000, 0xD000 | 0x9000 | 0xB000);

                self.alu_op = op;
                self.size = size;
                self.src_mode = if to_reg || is_addr {
                    Some(ea_mode)
                } else {
                    Some(AddrMode::DataReg(reg))
                };
                self.dst_mode = if to_reg {
                    Some(AddrMode::DataReg(reg))
                } else if is_addr {
                    Some(AddrMode::AddrReg(reg))
                } else {
                    Some(ea_mode)
                };
                self.in_followup = true;
                self.followup_tag = TAG_FETCH_SRC_EA;
                self.continue_instruction(bus);
                return;
            }
        }

        // --- ADDQ/SUBQ (0x5xxx with size != 3) ---
        // Also handles DBcc (size == 3, EA mode == 001) and Scc (size == 3, other EA)
        if (opcode & 0xF000) == 0x5000 {
            let quick_val = {
                let d = (opcode >> 9) & 7;
                if d == 0 { 8 } else { d }
            } as u32;
            let is_sub = (opcode & 0x0100) != 0;
            let opmode = (opcode >> 6) & 3;

            if opmode == 3 {
                // DBcc or Scc
                if (opcode & 0x38) == 0x08 {
                    // DBcc: decrement Dn and branch if not -1.
                    // Save displacement from IRC now, before FetchIRC overwrites it.
                    self.ea_reg = (opcode & 7) as u8;
                    self.src_val = u32::from(self.irc);
                    self.in_followup = true;
                    self.followup_tag = TAG_DBCC_EXECUTE;
                    self.micro_ops.push(MicroOp::FetchIRC);
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                } else {
                    // Scc: set byte to 0xFF or 0x00 based on condition
                    let ea_mode_bits = ((opcode >> 3) & 7) as u8;
                    let ea_reg = (opcode & 7) as u8;
                    self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
                    self.size = Size::Byte;
                    self.in_followup = true;
                    self.followup_tag = TAG_FETCH_DST_EA;
                    self.continue_instruction(bus);
                    return;
                }
            }

            let size = match opmode {
                0 => Size::Byte,
                1 => Size::Word,
                2 => Size::Long,
                _ => unreachable!(),
            };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.alu_op = if is_sub { AluOp::Sub } else { AluOp::Add };
            self.size = size;
            self.src_mode = Some(AddrMode::Immediate);
            self.src_val = quick_val;
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_DST_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- ALU immediate (ORI, ANDI, SUBI, ADDI, EORI, CMPI) ---
        if matches!(
            opcode & 0xFF00,
            0x0000 | 0x0200 | 0x0400 | 0x0600 | 0x0A00 | 0x0C00
        ) {
            let op = match (opcode >> 9) & 7 {
                0 => AluOp::Or,
                1 => AluOp::And,
                2 => AluOp::Sub,
                3 => AluOp::Add,
                5 => AluOp::Eor,
                6 => AluOp::Cmp,
                _ => {
                    // Bit operations or other 0x0xxx — not handled here
                    self.halt();
                    return;
                }
            };
            let size = match (opcode >> 6) & 3 {
                0 => Size::Byte,
                1 => Size::Word,
                2 => Size::Long,
                _ => unreachable!(),
            };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.alu_op = op;
            self.size = size;
            self.src_mode = Some(AddrMode::Immediate);
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_DATA;
            self.continue_instruction(bus);
            return;
        }

        // --- MOVEQ (0x7xxx with bit 8 = 0) ---
        if (opcode & 0xF100) == 0x7000 {
            let reg = ((opcode >> 9) & 7) as usize;
            let val = (opcode & 0xFF) as i8 as i32 as u32;
            self.regs.d[reg] = val;
            self.set_flags_move(val, Size::Long);
            return;
        }

        // --- BRA/Bcc/BSR (0x6xxx) ---
        if (opcode & 0xF000) == 0x6000 {
            let cond = ((opcode >> 8) & 0x0F) as u8;
            let disp8 = (opcode & 0xFF) as i8;
            self.in_followup = true;

            if cond == 1 {
                // BSR: push return PC, then branch
                self.followup_tag = TAG_BSR_EXECUTE;
                let return_pc = if disp8 == 0 {
                    self.instr_start_pc.wrapping_add(4)
                } else {
                    self.instr_start_pc.wrapping_add(2)
                };
                self.data = return_pc;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
            } else {
                self.followup_tag = TAG_BCC_EXECUTE;
            }

            if disp8 == 0 {
                // 16-bit displacement: read from IRC now, before FetchIRC
                // overwrites it. Queue FetchIRC to advance past the word.
                self.src_val = u32::from(self.irc);
                self.micro_ops.push(MicroOp::FetchIRC);
                // followup_tag stays as TAG_BCC_EXECUTE / TAG_BSR_EXECUTE
                // (no separate FETCH_DISP phase needed)
            } else {
                self.src_val = disp8 as i32 as u32;
            }

            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- JSR / JMP (0x4E80 / 0x4EC0) ---
        if (opcode & 0xFFC0) == 0x4E80 || (opcode & 0xFFC0) == 0x4EC0 {
            let is_jsr = (opcode & 0x40) == 0;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;

            if is_jsr {
                // Compute return PC and save for push AFTER EA resolution.
                // The real 68000 computes EA first, then pushes, then jumps.
                let ext = self.src_mode.unwrap().ext_word_count();
                let return_pc = self.instr_start_pc
                    .wrapping_add(2)
                    .wrapping_add(u32::from(ext) * 2);
                self.dst_val = return_pc;
            }

            self.continue_instruction(bus);
            return;
        }

        // --- LEA (0x41C0) ---
        if (opcode & 0xF1C0) == 0x41C0 {
            let reg = ((opcode >> 9) & 7) as u8;
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            // Store destination A-register in dst_mode (not ea_reg, which
            // gets overwritten by calc_ea_start for displacement modes).
            self.dst_mode = Some(AddrMode::AddrReg(reg));
            self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- PEA (0x4840) ---
        if (opcode & 0xFFC0) == 0x4840 {
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.src_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            self.followup_tag = TAG_FETCH_SRC_EA;
            if self.calc_ea_start(self.src_mode.unwrap(), true) {
                self.followup_tag = TAG_FETCH_DST_EA;
            }
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- CLR / TST (0x4200 / 0x4A00) ---
        if (opcode & 0xFF00) == 0x4200 || (opcode & 0xFF00) == 0x4A00 {
            let is_tst = (opcode & 0xFF00) == 0x4A00;
            let size = match (opcode >> 6) & 3 {
                0 => Size::Byte,
                1 => Size::Word,
                2 => Size::Long,
                _ => {
                    self.halt();
                    return;
                }
            };
            let ea_mode_bits = ((opcode >> 3) & 7) as u8;
            let ea_reg = (opcode & 7) as u8;

            self.size = size;
            self.dst_mode = AddrMode::decode(ea_mode_bits, ea_reg);
            self.in_followup = true;
            // Both CLR and TST need EA calculation first.
            // CLR: 68000 quirk reads then writes 0.
            // TST: reads the operand then sets flags (no writeback).
            self.followup_tag = TAG_FETCH_DST_EA;
            self.continue_instruction(bus);
            return;
        }

        // --- RTS (0x4E75) ---
        if opcode == 0x4E75 {
            self.in_followup = true;
            self.followup_tag = TAG_RTS_PC_HI;
            self.micro_ops.push(MicroOp::PopLongHi);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // --- Simple one-word instructions ---
        match opcode {
            0x4E71 => { /* NOP */ }
            0x4E70 => {
                // RESET (supervisor only)
                if self.regs.is_supervisor() {
                    self.micro_ops.push(MicroOp::AssertReset);
                    self.micro_ops.push(MicroOp::Internal(124));
                } else {
                    self.halt();
                }
            }
            _ => {
                self.halt();
            }
        }
    }

    /// Continue a multi-phase instruction based on the current follow-up tag.
    ///
    /// Each tag represents a point in the instruction's execution pipeline.
    /// After completing its work, each handler sets the next tag and queues
    /// micro-ops needed to reach it.
    pub fn continue_instruction<B: M68kBus>(&mut self, _bus: &mut B) {
        match self.followup_tag {
            // --- Operand fetch pipeline ---

            TAG_FETCH_SRC_EA => {
                if self.calc_ea_start(self.src_mode.unwrap(), true) {
                    self.followup_tag = TAG_FETCH_SRC_DATA;
                }
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_FETCH_SRC_DATA => {
                let opcode = self.ir;

                // LEA/PEA: source "data" is the computed address
                if (opcode & 0xF1C0) == 0x41C0 || (opcode & 0xFFC0) == 0x4840 {
                    self.src_val = self.addr;
                    self.followup_tag = TAG_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }

                // JMP/JSR: source address is the jump target
                if (opcode & 0xFFC0) == 0x4EC0 || (opcode & 0xFFC0) == 0x4E80 {
                    self.followup_tag = TAG_JSR_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }

                match self.src_mode.unwrap() {
                    AddrMode::DataReg(r) => {
                        self.src_val = self.regs.d[r as usize];
                        self.followup_tag = TAG_FETCH_DST_EA;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(r) => {
                        self.src_val = self.regs.a(r as usize);
                        self.followup_tag = TAG_FETCH_DST_EA;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::Immediate => {
                        let v = self.consume_irc();
                        if self.size == Size::Long {
                            self.src_val = u32::from(v) << 16;
                            self.followup_tag = TAG_DATA_SRC_LONG;
                        } else {
                            self.src_val = u32::from(v);
                            self.followup_tag = TAG_FETCH_DST_EA;
                        }
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        // Memory source: queue read ops, data arrives via finish_bus_cycle
                        self.followup_tag = TAG_FETCH_DST_EA;
                        self.queue_read_ops(self.size);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }

            TAG_FETCH_DST_EA => {
                // If source was a memory read, the result is in self.data.
                // Copy it to src_val now, before the dst read can overwrite data.
                if let Some(mode) = self.src_mode {
                    if !matches!(
                        mode,
                        AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
                    ) {
                        self.src_val = self.data;
                    }
                }

                // Source phase is complete — any source EA side effects (postinc,
                // predec) are committed. Clear ae_undo_reg so only destination
                // EA side effects can be rolled back on address errors.
                // Also clear program_space_access since the source read is done;
                // destination writes always use data space.
                self.ae_undo_reg = None;
                self.program_space_access = false;

                if let Some(m) = self.dst_mode {
                    if self.calc_ea_start(m, false) {
                        self.followup_tag = TAG_FETCH_DST_DATA;
                    }
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    self.followup_tag = TAG_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }

            TAG_FETCH_DST_DATA => {
                // MOVE and Scc are write-only: skip the destination read.
                // Read-modify-write instructions (ALU ops, CLR) need the read.
                let is_move = (self.ir & 0xC000) == 0 && (self.ir & 0x3000) != 0;
                let is_scc = (self.ir & 0xF0C0) == 0x50C0
                    && (self.ir & 0x0038) != 0x0008;
                if is_move || is_scc {
                    self.followup_tag = TAG_EXECUTE;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }

                match self.dst_mode.unwrap() {
                    AddrMode::DataReg(r) => {
                        self.dst_val = self.regs.d[r as usize];
                        self.followup_tag = TAG_EXECUTE;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(r) => {
                        self.dst_val = self.regs.a(r as usize);
                        self.followup_tag = TAG_EXECUTE;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        self.followup_tag = TAG_EXECUTE;
                        self.queue_read_ops(self.size);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }

            // --- Execute and writeback ---

            TAG_EXECUTE => {
                // If destination was a memory read, the result is in self.data.
                // Copy to dst_val before execute uses it.
                if let Some(mode) = self.dst_mode {
                    if !matches!(mode, AddrMode::DataReg(_) | AddrMode::AddrReg(_)) {
                        self.dst_val = self.data;
                    }
                }

                self.perform_execute();
                self.followup_tag = TAG_WRITEBACK;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_WRITEBACK => {
                self.perform_writeback();
                self.in_followup = false;
            }

            // --- EA extension word handlers ---

            TAG_EA_SRC_LONG => {
                self.addr |= u32::from(self.consume_irc());
                self.followup_tag = TAG_FETCH_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_SRC_DISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.regs.a(self.ea_reg as usize).wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_SRC_PCDISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.ea_pc.wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_DST_LONG => {
                self.addr |= u32::from(self.consume_irc());
                self.followup_tag = TAG_FETCH_DST_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_DST_DISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.regs.a(self.ea_reg as usize).wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_DST_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EA_DST_PCDISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.ea_pc.wrapping_add(disp as u32);
                self.followup_tag = TAG_FETCH_DST_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- Immediate long lo-word handlers ---

            TAG_DATA_SRC_LONG => {
                self.src_val |= u32::from(self.consume_irc());
                self.followup_tag = TAG_FETCH_DST_EA;
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_DATA_DST_LONG => {
                self.dst_val |= u32::from(self.consume_irc());
                self.followup_tag = TAG_EXECUTE;
                self.micro_ops.push(MicroOp::Execute);
            }

            // --- Branch handlers ---

            TAG_BCC_EXECUTE => {
                let cond = ((self.ir >> 8) & 0x0F) as u8;
                if self.check_condition(cond) {
                    let disp = self.src_val as i16 as i32;
                    self.regs.pc = self.instr_start_pc.wrapping_add(2).wrapping_add(disp as u32);
                    self.next_fetch_addr = self.regs.pc;
                    self.micro_ops.clear();
                    self.micro_ops.push(MicroOp::FetchIRC);
                    self.micro_ops.push(MicroOp::PromoteIRC);
                }
                self.in_followup = false;
            }

            TAG_DBCC_EXECUTE => {
                let cond = ((self.ir >> 8) & 0x0F) as u8;
                let reg = self.ea_reg as usize;
                let disp = self.src_val as i16 as i32;

                if !self.check_condition(cond) {
                    let counter = (self.regs.d[reg] & 0xFFFF) as u16;
                    // Save original for undo on branch AE (odd target).
                    self.dbcc_dn_undo = Some((reg as u8, counter));
                    let decremented = counter.wrapping_sub(1);
                    self.regs.d[reg] =
                        (self.regs.d[reg] & 0xFFFF_0000) | u32::from(decremented);

                    if decremented != 0xFFFF {
                        // Branch taken: reload prefetch from branch target
                        self.regs.pc = self
                            .instr_start_pc
                            .wrapping_add(2)
                            .wrapping_add(disp as u32);
                        self.next_fetch_addr = self.regs.pc;
                        self.micro_ops.clear();
                        self.micro_ops.push(MicroOp::FetchIRC);
                        self.micro_ops.push(MicroOp::PromoteIRC);
                    }
                }
                self.in_followup = false;
            }

            // --- Subroutine handlers ---

            TAG_JSR_EXECUTE => {
                let is_jsr = (self.ir & 0x40) == 0;
                // Set PC to target and start the pipeline refill.
                self.regs.pc = self.addr;
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                // FetchIRC at target first — triggers AE if odd address.
                // For JSR, the push happens AFTER FetchIRC succeeds.
                // This matches the real 68000 where the stack is not
                // modified if the jump target is misaligned.
                self.micro_ops.push(MicroOp::FetchIRC);
                if is_jsr {
                    self.data = self.dst_val; // return PC saved at decode
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            TAG_BSR_EXECUTE => {
                let disp = self.src_val as i16 as i32;
                self.regs.pc = self.instr_start_pc.wrapping_add(2).wrapping_add(disp as u32);
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            TAG_RTS_PC_HI => {
                // PopLongHi stored hi word in self.data (already shifted << 16).
                // PopLongLo will combine the lo word into self.data.
                self.followup_tag = TAG_RTS_PC_LO;
                self.micro_ops.push(MicroOp::PopLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_RTS_PC_LO => {
                // self.data now has the complete 32-bit return address
                self.regs.pc = self.data;
                self.next_fetch_addr = self.regs.pc;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            // --- Exception handlers ---

            TAG_EXC_STACK_PC_HI => {
                self.followup_tag = TAG_EXC_STACK_PC_LO;
                self.micro_ops.push(MicroOp::PushLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EXC_STACK_PC_LO => {
                // PushWord writes from self.data
                self.data = u32::from(self.regs.sr);
                self.followup_tag = TAG_EXC_STACK_SR;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EXC_STACK_SR => {
                self.followup_tag = TAG_EXC_FETCH_VECTOR;
                self.micro_ops.push(MicroOp::InterruptAck);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EXC_FETCH_VECTOR => {
                let vector = self.data as u8;
                self.addr = u32::from(vector) * 4;
                self.size = Size::Long;
                self.followup_tag = TAG_EXC_FINISH;
                self.queue_read_ops(Size::Long);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_EXC_FINISH => {
                self.regs.pc = self.data;
                self.next_fetch_addr = self.regs.pc;
                self.regs.set_supervisor(true);
                self.regs.sr &= !0x8000; // Clear trace
                self.regs.sr =
                    (self.regs.sr & !0x0700) | (u16::from(self.target_ipl) << 8);
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            // --- Address error exception frame ---
            // Frame pushed in order: PC, SR, IR, fault addr, access info
            // Then read vector 3 (0x0C) and jump.

            TAG_AE_PUSH_SR => {
                // PC already pushed by begin_address_error.
                // Now push the saved SR.
                self.data = u32::from(self.ae_saved_sr);
                self.followup_tag = TAG_AE_PUSH_IR;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_PUSH_IR => {
                self.data = u32::from(self.ae_frame_ir);
                self.followup_tag = TAG_AE_PUSH_FAULT;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_PUSH_FAULT => {
                self.data = self.ae_fault_addr;
                self.followup_tag = TAG_AE_PUSH_INFO;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_PUSH_INFO => {
                self.data = u32::from(self.ae_access_info);
                self.followup_tag = TAG_AE_FETCH_VECTOR;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_FETCH_VECTOR => {
                // Vector 3 = address error, at memory address 0x0C
                self.addr = 3 * 4;
                self.followup_tag = TAG_AE_FINISH;
                self.queue_read_ops(Size::Long);
                self.micro_ops.push(MicroOp::Execute);
            }

            TAG_AE_FINISH => {
                self.regs.pc = self.data;
                self.next_fetch_addr = self.regs.pc;
                self.ae_in_progress = false;
                self.micro_ops.clear();
                self.micro_ops.push(MicroOp::FetchIRC);
                self.micro_ops.push(MicroOp::PromoteIRC);
                self.in_followup = false;
            }

            _ => {
                self.in_followup = false;
            }
        }
    }
}
