//! ALU execution and result writeback for the 68000.
//!
//! After operands are fetched, the execute stage runs the ALU operation
//! and the writeback stage stores the result. These are separate follow-up
//! tags because some instructions (like CMP) execute but don't write back.

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::cpu::{AluOp, Cpu68000};
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Execute the ALU operation for the current instruction.
    ///
    /// Called at TAG_EXECUTE after both source and destination operands
    /// are available in `src_val` and `dst_val`.
    ///
    /// Sets `self.data` to the result for writeback. Updates flags in SR.
    pub fn perform_execute(&mut self) {
        let opcode = self.ir;

        // MOVE: copy source to data, set flags (unless MOVEA)
        if (opcode & 0xC000) == 0 && (opcode & 0x3000) != 0 {
            self.data = self.src_val;
            let dst = self.dst_mode.unwrap();
            if !matches!(dst, AddrMode::AddrReg(_)) {
                let src_is_reg = matches!(
                    self.src_mode,
                    Some(AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate)
                );

                // The real 68000's 16-bit ALU evaluates MOVE flags in stages
                // during the write bus cycle. On write AE, the frame SR reflects
                // how far evaluation progressed before the fault.
                if self.size == Size::Long {
                    if src_is_reg {
                        // Register/immediate source: flags not yet evaluated
                        // when write starts for simple destinations.
                        if matches!(dst, AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_)) {
                            self.pre_move_sr = Some(self.regs.sr);
                        } else if matches!(dst, AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_)) {
                            self.pre_move_vc = Some(self.regs.sr);
                        }
                        // -(An), abs.w, abs.l: flags fully committed, no save
                    } else {
                        // Memory source: the 68000's 16-bit ALU evaluates flags
                        // from the last word read (lo word). The write starts
                        // immediately for simple destinations, so the AE frame
                        // reflects lo-word-based flags.
                        if matches!(
                            dst,
                            AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_) | AddrMode::AbsLong
                        ) {
                            let pre_sr = self.regs.sr;
                            self.set_flags_move(self.data, self.size);
                            // Build synthetic SR with lo-word N,Z and cleared V,C
                            let lo = self.data as u16;
                            let mut lo_sr = pre_sr & !0x000F;
                            if lo == 0 { lo_sr |= 0x0004; } // Z from lo word
                            if lo & 0x8000 != 0 { lo_sr |= 0x0008; } // N from lo word
                            self.pre_move_sr = Some(lo_sr);
                            return;
                        }
                    }
                }

                self.set_flags_move(self.data, self.size);
            }
            return;
        }

        // LEA: result is the computed address
        if (opcode & 0xF1C0) == 0x41C0 {
            self.src_val = self.addr;
            return;
        }

        // PEA: result is the computed address (for push)
        if (opcode & 0xFFC0) == 0x4840 {
            self.src_val = self.addr;
            return;
        }

        // CLR: result is zero, set flags
        if (opcode & 0xFF00) == 0x4200 {
            self.data = 0;
            self.set_flags_move(0, self.size);
            return;
        }

        // TST: already read into dst_val, just set flags
        if (opcode & 0xFF00) == 0x4A00 {
            self.set_flags_move(self.dst_val, self.size);
            return;
        }

        // Scc: set byte to 0xFF if condition true, 0x00 if false
        if (opcode & 0xF0C0) == 0x50C0 && (opcode & 0x0038) != 0x0008 {
            let cond = ((opcode >> 8) & 0x0F) as u8;
            self.data = if self.check_condition(cond) { 0xFF } else { 0x00 };
            return;
        }

        // ADDA/SUBA/CMPA: address register operations don't set flags
        // (except CMPA which sets flags on the subtraction).
        // The source is sign-extended from word to long before the operation,
        // and the full 32-bit result is written to the address register.
        if matches!(self.dst_mode, Some(AddrMode::AddrReg(_))) {
            let src = if self.size == Size::Word {
                self.src_val as i16 as i32 as u32
            } else {
                self.src_val
            };
            match self.alu_op {
                AluOp::Add => self.data = self.dst_val.wrapping_add(src),
                AluOp::Sub => self.data = self.dst_val.wrapping_sub(src),
                AluOp::Cmp => {
                    // CMPA sets flags but doesn't write back
                    self.data = self.exec_alu(AluOp::Cmp, src, self.dst_val, Size::Long);
                }
                _ => {
                    self.data = self.exec_alu(self.alu_op, self.src_val, self.dst_val, self.size);
                }
            }
            // Force Long size so writeback stores the full 32-bit result
            self.size = Size::Long;
            return;
        }

        // ALU operations: ADD, SUB, CMP, AND, OR, EOR, ADDI, SUBI, etc.
        // ADDQ/SUBQ also land here (src_val already set to quick value)
        self.data = self.exec_alu(self.alu_op, self.src_val, self.dst_val, self.size);
    }

    /// Write the execution result back to the destination.
    ///
    /// Called at TAG_WRITEBACK after `perform_execute`. Routes the result
    /// to a register or queues memory write bus cycles.
    pub fn perform_writeback(&mut self) {
        let opcode = self.ir;

        // LEA: store address in destination A-register
        if (opcode & 0xF1C0) == 0x41C0 {
            if let Some(AddrMode::AddrReg(r)) = self.dst_mode {
                self.regs.set_a(r as usize, self.src_val);
            }
            return;
        }

        // PEA: push the computed address onto the stack
        if (opcode & 0xFFC0) == 0x4840 {
            self.data = self.src_val;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            return;
        }

        // CMP/CMPI/CMPA: no writeback (flags only)
        if matches!(self.alu_op, AluOp::Cmp) {
            let is_cmp = (opcode & 0xF000) == 0xB000
                || (opcode & 0xFF00) == 0x0C00;
            if is_cmp {
                return;
            }
        }

        // TST: no writeback (flags only)
        if (opcode & 0xFF00) == 0x4A00 {
            return;
        }

        // Everything else: write data to destination
        // Covers MOVE, ALU general (ADD/SUB/AND/OR/EOR), ALU immediate
        // (ORI/ANDI/SUBI/ADDI/EORI), ADDQ/SUBQ, CLR, Scc
        if let Some(dst) = self.dst_mode {
            match dst {
                AddrMode::DataReg(r) => {
                    let reg = &mut self.regs.d[r as usize];
                    *reg = match self.size {
                        Size::Byte => (*reg & 0xFFFF_FF00) | (self.data & 0xFF),
                        Size::Word => (*reg & 0xFFFF_0000) | (self.data & 0xFFFF),
                        Size::Long => self.data,
                    };
                }
                AddrMode::AddrReg(r) => {
                    // Address register destinations are always sign-extended
                    let val = if self.size == Size::Word {
                        (self.data as i16 as i32) as u32
                    } else {
                        self.data
                    };
                    self.regs.set_a(r as usize, val);
                }
                _ => {
                    // Memory destination: queue write bus cycles
                    self.queue_write_ops(self.size);
                }
            }
        }
    }

    /// Dispatch an ALU operation and update flags.
    pub(crate) fn exec_alu(&mut self, op: AluOp, src: u32, dst: u32, size: Size) -> u32 {
        let mask = size.mask();
        let s = src & mask;
        let d = dst & mask;
        match op {
            AluOp::Add => {
                let r = s.wrapping_add(d) & mask;
                self.set_flags_add(s, d, r, size);
                r
            }
            AluOp::Sub => {
                let r = d.wrapping_sub(s) & mask;
                self.set_flags_sub(s, d, r, size);
                r
            }
            AluOp::Cmp => {
                // CMP sets N, Z, V, C but does NOT affect X
                let saved_x = self.regs.sr & crate::flags::X;
                let r = d.wrapping_sub(s) & mask;
                self.set_flags_sub(s, d, r, size);
                self.regs.sr = (self.regs.sr & !crate::flags::X) | saved_x;
                r
            }
            AluOp::And => {
                let r = s & d;
                self.set_flags_logic(r, size);
                r
            }
            AluOp::Or => {
                let r = s | d;
                self.set_flags_logic(r, size);
                r
            }
            AluOp::Eor => {
                let r = s ^ d;
                self.set_flags_logic(r, size);
                r
            }
        }
    }

    // --- Flag computation ---

    pub(crate) fn set_flags_add(&mut self, s: u32, d: u32, r: u32, size: Size) {
        use crate::flags::{C, N, V, X, Z};
        let msb = size.msb_mask();
        self.regs.sr &= !(N | Z | V | C | X);
        if r == 0 {
            self.regs.sr |= Z;
        }
        if r & msb != 0 {
            self.regs.sr |= N;
        }
        let sm = s & msb != 0;
        let dm = d & msb != 0;
        let rm = r & msb != 0;
        if (sm && dm) || (!rm && (sm || dm)) {
            self.regs.sr |= C | X;
        }
        if (sm && dm && !rm) || (!sm && !dm && rm) {
            self.regs.sr |= V;
        }
    }

    pub(crate) fn set_flags_sub(&mut self, s: u32, d: u32, r: u32, size: Size) {
        use crate::flags::{C, N, V, X, Z};
        let msb = size.msb_mask();
        self.regs.sr &= !(N | Z | V | C | X);
        if r == 0 {
            self.regs.sr |= Z;
        }
        if r & msb != 0 {
            self.regs.sr |= N;
        }
        let sm = s & msb != 0;
        let dm = d & msb != 0;
        let rm = r & msb != 0;
        if (sm && !dm) || (rm && (sm || !dm)) {
            self.regs.sr |= C | X;
        }
        if (!sm && dm && !rm) || (sm && !dm && rm) {
            self.regs.sr |= V;
        }
    }

    pub(crate) fn set_flags_logic(&mut self, r: u32, size: Size) {
        use crate::flags::{C, N, V, Z};
        let msb = size.msb_mask();
        self.regs.sr &= !(N | Z | V | C);
        if r == 0 {
            self.regs.sr |= Z;
        }
        if r & msb != 0 {
            self.regs.sr |= N;
        }
    }

    pub(crate) fn set_flags_move(&mut self, val: u32, size: Size) {
        use crate::flags::{C, N, V, Z};
        let mask = size.mask();
        let msb = size.msb_mask();
        let v = val & mask;
        self.regs.sr &= !(N | Z | V | C);
        if v == 0 {
            self.regs.sr |= Z;
        }
        if v & msb != 0 {
            self.regs.sr |= N;
        }
    }

    /// Evaluate a condition code (0-15) against the current SR flags.
    pub(crate) fn check_condition(&self, cond: u8) -> bool {
        use crate::flags::{C, N, V, Z};
        let sr = self.regs.sr;
        let n = sr & N != 0;
        let z = sr & Z != 0;
        let v = sr & V != 0;
        let c = sr & C != 0;
        match cond {
            0 => true,                                          // T
            1 => false,                                         // F
            2 => !c && !z,                                      // HI
            3 => c || z,                                        // LS
            4 => !c,                                            // CC
            5 => c,                                             // CS
            6 => !z,                                            // NE
            7 => z,                                             // EQ
            8 => !v,                                            // VC
            9 => v,                                             // VS
            10 => !n,                                           // PL
            11 => n,                                            // MI
            12 => n == v,                                       // GE
            13 => n != v,                                       // LT
            14 => !z && (n == v),                               // GT
            15 => z || (n != v),                                // LE
            _ => unreachable!(),
        }
    }
}
