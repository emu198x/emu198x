//! Logic instructions: OR (group 0x8) and AND (group 0xC).
//!
//! Both follow the same pattern as ADD/SUB:
//! - Opmodes 0-2: EA → Dn (byte/word/long)
//! - Opmodes 3: (reserved for DIVU/MULU on 68000)
//! - Opmodes 4-6: Dn → EA (read-modify-write, memory only)
//! - Opmodes 7: (reserved for DIVS/MULS on 68000)
//!
//! Special cases in group 0x8: SBCD (opmodes 4 EA mode 0/1), DIVU (opmode 3), DIVS (opmode 7)
//! Special cases in group 0xC: ABCD (opmodes 4 EA mode 0/1), MULU (opmode 3), MULS (opmode 7),
//!                              EXG (opmodes 5/6 specific patterns)
//!
//! Followup tags:
//!   100 = AbsLong/Imm.Long second extension word
//!   101 = ALU after memory read (EA→Dn direction)
//!   102 = RMW writeback after read (Dn→EA direction)

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::cpu::Cpu68000;
use crate::microcode::MicroOp;

/// Is this OR (true) or AND (false)?
#[derive(Debug, Clone, Copy)]
enum LogicKind {
    Or,
    And,
}

impl Cpu68000 {
    // ================================================================
    // OR (group 0x8)
    // ================================================================

    pub(crate) fn exec_or(&mut self) {
        let op = self.ir;
        let opmode = ((op >> 6) & 7) as u8;

        // Followups
        if self.in_followup {
            self.logic_followup(LogicKind::Or);
            return;
        }

        // Opmode 3: DIVU.w
        if opmode == 3 {
            self.exec_divu();
            return;
        }
        // Opmode 7: DIVS.w
        if opmode == 7 {
            self.exec_divs();
            return;
        }

        // Opmode 4, EA mode 0/1: SBCD
        if opmode == 4 {
            let ea_mode = ((op >> 3) & 7) as u8;
            if ea_mode == 0 || ea_mode == 1 {
                self.exec_bcd(false); // SBCD
                return;
            }
        }

        self.exec_logic_common(LogicKind::Or);
    }

    // ================================================================
    // AND (group 0xC)
    // ================================================================

    pub(crate) fn exec_and(&mut self) {
        let op = self.ir;
        let opmode = ((op >> 6) & 7) as u8;

        // Followups
        if self.in_followup {
            self.logic_followup(LogicKind::And);
            return;
        }

        // Opmode 3: MULU.w
        if opmode == 3 {
            self.exec_mulu();
            return;
        }
        // Opmode 7: MULS.w
        if opmode == 7 {
            self.exec_muls();
            return;
        }

        // Opmodes 4-6 with special EA patterns
        if opmode >= 4 && opmode <= 6 {
            let ea_mode = ((op >> 3) & 7) as u8;
            if ea_mode == 0 || ea_mode == 1 {
                if opmode == 4 {
                    // ABCD (opmode 4, EA mode 0/1)
                    self.exec_bcd(true); // ABCD
                    return;
                }
                // EXG (opmodes 5/6 with mode 0/1)
                if (opmode == 5 && ea_mode == 0) || (opmode == 6 && ea_mode == 1) ||
                   (opmode == 5 && ea_mode == 1) {
                    self.exec_exg();
                    return;
                }
            }
        }

        self.exec_logic_common(LogicKind::And);
    }

    // ================================================================
    // Common logic implementation
    // ================================================================

    fn exec_logic_common(&mut self, kind: LogicKind) {
        let op = self.ir;
        let reg = ((op >> 9) & 7) as u8;
        let opmode = ((op >> 6) & 7) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;

        // Stash kind and register
        let kind_code = match kind { LogicKind::Or => 0u32, LogicKind::And => 1 };
        self.addr2 = kind_code | (u32::from(reg) << 8);

        match opmode {
            // EA → Dn (byte/word/long)
            0 | 1 | 2 => {
                let size = Size::from_bits(opmode).unwrap();
                self.size = size;
                self.resolve_logic_ea_read(&ea, size);
            }
            // Dn → EA (byte/word/long, memory RMW)
            4 | 5 | 6 => {
                let size = Size::from_bits(opmode & 3).unwrap();
                self.size = size;
                self.data2 = self.read_data_reg(reg, size); // stash Dn value
                self.resolve_logic_ea_rmw(&ea, size);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Resolve EA for logic read (EA → Dn).
    fn resolve_logic_ea_read(&mut self, ea: &AddrMode, size: Size) {
        match ea {
            AddrMode::DataReg(r) => {
                self.data = self.read_data_reg(*r, size);
                self.logic_alu_ea_to_reg();
            }
            AddrMode::AddrReg(r) => {
                // Only AND/OR.w and .l can use An source
                self.data = self.regs.a(*r as usize);
                self.logic_alu_ea_to_reg();
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 101;
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
                self.followup_tag = 101;
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
                self.followup_tag = 101;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 101;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 101;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 101;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                let hi = u32::from(self.consume_irc()) << 16;
                self.data2 = hi | (self.addr2 << 16 >> 16); // pack kind+reg in data2 high
                // Actually, simpler: stash addr hi in data field
                self.data = hi;
                self.in_followup = true;
                self.followup_tag = 100;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 101;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcIndex => {
                let base = self.irc_addr;
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(base, ext);
                self.program_space_access = true;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 101;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::Immediate => {
                match size {
                    Size::Byte => {
                        self.data = u32::from(self.consume_irc()) & 0xFF;
                        self.logic_alu_ea_to_reg();
                    }
                    Size::Word => {
                        self.data = u32::from(self.consume_irc());
                        self.logic_alu_ea_to_reg();
                    }
                    Size::Long => {
                        let hi = u32::from(self.consume_irc()) << 16;
                        self.data = hi;
                        self.in_followup = true;
                        self.followup_tag = 100;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }
        }
    }

    /// Resolve EA for logic RMW (Dn → EA).
    fn resolve_logic_ea_rmw(&mut self, ea: &AddrMode, size: Size) {
        match ea {
            AddrMode::DataReg(_) => {
                // Can't happen for opmodes 4-6 (would be ABCD/EXG)
                self.illegal_instruction();
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 102;
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
                self.followup_tag = 102;
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
                self.followup_tag = 102;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 102;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 102;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 102;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data = self.data2; // preserve Dn value in data
                self.data2 = u32::from(self.consume_irc()) << 16; // addr hi in data2
                // Actually this conflicts with addr2 usage. Let me use a simpler approach.
                // Stash addr hi, let the ext2 handler sort it out.
                self.in_followup = true;
                self.followup_tag = 100;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Followup dispatcher for OR/AND and their special cases.
    fn logic_followup(&mut self, _kind: LogicKind) {
        match self.followup_tag {
            100 => self.logic_ext2(),
            101 => self.logic_alu_ea_to_reg(),
            102 => self.logic_rmw_alu(),
            103 => self.muldiv_ea_complete(),
            104 => self.muldiv_abslong_ext2(),
            105 => self.bcd_mem_src_read(),
            106 => self.bcd_mem_dst_read(),
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 100: AbsLong/Imm.Long second extension word.
    fn logic_ext2(&mut self) {
        let lo = self.consume_irc();
        let opmode = ((self.ir >> 6) & 7) as u8;
        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;
        let ea = AddrMode::decode(ea_mode, ea_reg).unwrap_or(AddrMode::DataReg(0));

        match ea {
            AddrMode::AbsLong => {
                if opmode >= 4 && opmode <= 6 {
                    // RMW direction: data has Dn value, data2 has addr hi
                    self.addr = self.data2 | u32::from(lo);
                    self.data2 = self.data; // restore Dn value
                    self.queue_read_ops(self.size);
                    self.followup_tag = 102;
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    // EA→Dn direction: data has addr hi
                    self.addr = self.data | u32::from(lo);
                    self.queue_read_ops(self.size);
                    self.followup_tag = 101;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            AddrMode::Immediate => {
                // data has hi word of immediate
                self.data = self.data | u32::from(lo);
                self.logic_alu_ea_to_reg();
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 101: Memory read complete, perform logic EA → Dn.
    fn logic_alu_ea_to_reg(&mut self) {
        let size = self.size;
        let kind_code = self.addr2 & 0xFF;
        let reg = ((self.addr2 >> 8) & 0xFF) as u8;
        let src = self.data;
        let dst = self.read_data_reg(reg, size);

        let result = if kind_code == 0 { src | dst } else { src & dst };
        self.set_flags_logic(result, size);
        self.write_data_reg(reg, result, size);

        self.in_followup = false;
        self.followup_tag = 0;

        // Long-size: Internal(4) for reg/imm, Internal(2) for memory
        if size == Size::Long {
            let ea_mode = ((self.ir >> 3) & 7) as u8;
            let ea_reg = (self.ir & 7) as u8;
            let ea = AddrMode::decode(ea_mode, ea_reg).unwrap_or(AddrMode::DataReg(0));
            let is_reg_or_imm = matches!(
                ea,
                AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
            );
            if is_reg_or_imm {
                self.micro_ops.push(MicroOp::Internal(4));
            } else {
                self.micro_ops.push(MicroOp::Internal(2));
            }
        }
    }

    /// Tag 102: Memory read complete, perform logic Dn → EA writeback.
    fn logic_rmw_alu(&mut self) {
        let size = self.size;
        let kind_code = self.addr2 & 0xFF;
        let src = self.data2; // Dn value
        let dst = self.data;  // Memory value

        let result = if kind_code == 0 { src | dst } else { src & dst };
        self.set_flags_logic(result, size);
        self.data = result;

        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(size);
    }

    // ================================================================
    // EXG (exchange registers)
    // ================================================================
    // Group 0xC, various opmodes with special EA patterns:
    //   0xC100 pattern: 1100 RRR 1 01000 RRR = EXG Dx,Dy (opmode 01000)
    //   0xC108 pattern: 1100 RRR 1 01001 RRR = EXG Ax,Ay (opmode 01001)
    //   0xC188 pattern: 1100 RRR 1 10001 RRR = EXG Dx,Ay (opmode 10001)
    // 6 cycles.

    fn exec_exg(&mut self) {
        let op = self.ir;
        let rx = ((op >> 9) & 7) as usize;
        let ry = (op & 7) as usize;
        let opmode = ((op >> 3) & 0x1F) as u8;

        match opmode {
            0b01000 => {
                // EXG Dx,Dy
                let tmp = self.regs.d[rx];
                self.regs.d[rx] = self.regs.d[ry];
                self.regs.d[ry] = tmp;
            }
            0b01001 => {
                // EXG Ax,Ay
                let tmp = self.regs.a(rx);
                self.regs.set_a(rx, self.regs.a(ry));
                self.regs.set_a(ry, tmp);
            }
            0b10001 => {
                // EXG Dx,Ay
                let tmp = self.regs.d[rx];
                self.regs.d[rx] = self.regs.a(ry);
                self.regs.set_a(ry, tmp);
            }
            _ => {
                self.illegal_instruction();
                return;
            }
        }

        self.micro_ops.push(MicroOp::Internal(2));
    }

    // ================================================================
    // MULU / MULS / DIVU / DIVS
    // ================================================================
    //
    // MULU <ea>.w, Dn — unsigned multiply: Dn.w × EA.w → Dn.l
    //   Timing: 38+2n where n = number of set bits in source (unsigned)
    // MULS <ea>.w, Dn — signed multiply: Dn.w × EA.w → Dn.l
    //   Timing: 38+2n where n = number of 01 or 10 bit-pairs in source
    // DIVU <ea>.w, Dn — unsigned divide: Dn.l ÷ EA.w → Dn (quotient.w:remainder.w)
    // DIVS <ea>.w, Dn — signed divide: Dn.l ÷ EA.w → Dn (quotient.w:remainder.w)
    //
    // Followup tags:
    //   103 = EA read complete, perform mul/div
    //   104 = AbsLong ext2 for mul/div

    /// Resolve EA word source for multiply/divide.
    fn resolve_muldiv_ea(&mut self) {
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
                self.muldiv_ea_complete();
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 103;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                self.regs.set_a(r as usize, a.wrapping_add(2));
                self.addr = a;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 103;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let original_a = self.regs.a(r as usize);
                let a = original_a.wrapping_sub(2);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 103;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 103;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 103;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 103;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 104;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                self.queue_read_ops(Size::Word);
                self.in_followup = true;
                self.followup_tag = 103;
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
                self.followup_tag = 103;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::Immediate => {
                self.data = u32::from(self.consume_irc());
                self.muldiv_ea_complete();
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 104: mul/div AbsLong second word.
    fn muldiv_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(Size::Word);
        self.followup_tag = 103;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 103: EA read complete, perform the multiply or divide.
    fn muldiv_ea_complete(&mut self) {
        let op = self.ir;
        let dn = ((op >> 9) & 7) as usize;
        let opmode = ((op >> 6) & 7) as u8;
        let group = op >> 12;
        let src_word = (self.data & 0xFFFF) as u16;

        self.in_followup = false;
        self.followup_tag = 0;

        match (group, opmode) {
            (0x8, 3) => {
                // DIVU
                if src_word == 0 {
                    // Division by zero — trap vector 5
                    self.exception_pc_override = Some(self.irc_addr);
                    self.exception(5, 0);
                    return;
                }
                let dividend = self.regs.d[dn];
                let total_cycles = Cpu68000::divu_cycles(dividend, src_word);
                // Perform division
                let quotient = dividend / u32::from(src_word);
                let remainder = dividend % u32::from(src_word);

                if quotient > 0xFFFF {
                    // Overflow: V=1, C=0, N=1 (empirical — real 68000 always sets N on overflow)
                    let mut sr = self.regs.sr & !0x000F;
                    sr |= 0x000A; // V + N
                    self.regs.sr = sr;
                } else {
                    self.regs.d[dn] = (remainder << 16) | (quotient & 0xFFFF);
                    // Set flags: N from quotient bit 15, Z from quotient, V=0, C=0
                    let mut sr = self.regs.sr & !0x000F; // clear N,Z,V,C
                    if quotient & 0x8000 != 0 { sr |= 0x0008; } // N
                    if quotient & 0xFFFF == 0 { sr |= 0x0004; } // Z
                    self.regs.sr = sr;
                }
                // Internal(total-4): start_next_instruction adds FetchIRC(4).
                let internal = total_cycles.saturating_sub(4);
                if internal > 0 {
                    self.micro_ops.push(MicroOp::Internal(internal));
                }
            }
            (0x8, 7) => {
                // DIVS
                if src_word == 0 {
                    // Division by zero — trap vector 5
                    self.exception_pc_override = Some(self.irc_addr);
                    self.exception(5, 0);
                    return;
                }
                let dividend = self.regs.d[dn] as i32;
                let divisor = src_word as i16;
                let total_cycles = Cpu68000::divs_cycles(dividend, divisor);

                let quotient = dividend / i32::from(divisor);
                let remainder = dividend % i32::from(divisor);

                if quotient > 32767 || quotient < -32768 {
                    // Overflow: V=1, C=0, N=1 (empirical — real 68000 always sets N on overflow)
                    let mut sr = self.regs.sr & !0x000F;
                    sr |= 0x000A; // V + N
                    self.regs.sr = sr;
                } else {
                    let q16 = quotient as u16;
                    let r16 = remainder as u16;
                    self.regs.d[dn] = (u32::from(r16) << 16) | u32::from(q16);
                    let mut sr = self.regs.sr & !0x000F;
                    if q16 & 0x8000 != 0 { sr |= 0x0008; }
                    if q16 == 0 { sr |= 0x0004; }
                    self.regs.sr = sr;
                }
                let internal = total_cycles.saturating_sub(4);
                if internal > 0 {
                    self.micro_ops.push(MicroOp::Internal(internal));
                }
            }
            (0xC, 3) => {
                // MULU: unsigned word multiply
                let dst = (self.regs.d[dn] & 0xFFFF) as u16;
                let result = u32::from(dst) * u32::from(src_word);
                self.regs.d[dn] = result;

                // Flags: N from bit 31, Z from result, V=0, C=0
                let mut sr = self.regs.sr & !0x000F;
                if result & 0x8000_0000 != 0 { sr |= 0x0008; }
                if result == 0 { sr |= 0x0004; }
                self.regs.sr = sr;

                // Timing: 38 + 2 * (set bits in source)
                let set_bits = src_word.count_ones();
                let total = 38 + 2 * set_bits;
                let internal = total.saturating_sub(4);
                self.micro_ops.push(MicroOp::Internal(internal as u8));
            }
            (0xC, 7) => {
                // MULS: signed word multiply
                let dst = self.regs.d[dn] as i16;
                let src = src_word as i16;
                let result = (i32::from(dst) * i32::from(src)) as u32;
                self.regs.d[dn] = result;

                let mut sr = self.regs.sr & !0x000F;
                if result & 0x8000_0000 != 0 { sr |= 0x0008; }
                if result == 0 { sr |= 0x0004; }
                self.regs.sr = sr;

                // Timing: count 01 and 10 transitions in Booth encoding of source.
                // Booth's algorithm appends 0 at the RIGHT (bit -1), not the left.
                // Scan pairs: (bit_-1, bit_0), (bit_0, bit_1), ..., (bit_14, bit_15).
                // XOR with left-shifted value detects transitions; mask to 16 bits
                // excludes the unwanted bit 15 → bit 16 (prepended 0) transition.
                let v = u32::from(src_word);
                let transitions = ((v ^ (v << 1)) & 0xFFFF).count_ones();
                let total = 38 + 2 * transitions;
                let internal = total.saturating_sub(4);
                self.micro_ops.push(MicroOp::Internal(internal as u8));
            }
            _ => self.illegal_instruction(),
        }
    }

    fn exec_mulu(&mut self) {
        // addr2 high byte: stash which mul/div op (reuse for dispatch)
        self.resolve_muldiv_ea();
    }

    fn exec_muls(&mut self) {
        self.resolve_muldiv_ea();
    }

    fn exec_divu(&mut self) {
        self.resolve_muldiv_ea();
    }

    fn exec_divs(&mut self) {
        self.resolve_muldiv_ea();
    }

    // ================================================================
    // ABCD / SBCD (BCD arithmetic)
    // ================================================================
    //
    // ABCD Dy,Dx: 1100 RRR 10000 0 RRR (group 0xC, opmode 4, EA mode 0)
    // ABCD -(Ay),-(Ax): 1100 RRR 10000 1 RRR (group 0xC, opmode 4, EA mode 1)
    // SBCD Dy,Dx: 1000 RRR 10000 0 RRR (group 0x8, opmode 4, EA mode 0)
    // SBCD -(Ay),-(Ax): 1000 RRR 10000 1 RRR (group 0x8, opmode 4, EA mode 1)
    //
    // Timing: Register=6, Memory=18
    //
    // Followup tags:
    //   105 = BCD memory: src read complete, predec dst, read dst
    //   106 = BCD memory: dst read complete, ALU + write

    fn exec_bcd(&mut self, is_add: bool) {
        let op = self.ir;
        let rx = ((op >> 9) & 7) as u8; // Dx/Ax
        let ry = (op & 7) as u8;        // Dy/Ay
        let ea_mode = ((op >> 3) & 7) as u8;

        if ea_mode == 0 {
            // Register mode: Dy,Dx
            let src = (self.regs.d[ry as usize] & 0xFF) as u8;
            let dst = (self.regs.d[rx as usize] & 0xFF) as u8;
            let x = self.x_flag();

            let (result, carry, overflow) = if is_add {
                self.bcd_add(src, dst, x)
            } else {
                self.bcd_sub(dst, src, x)
            };

            self.regs.d[rx as usize] =
                (self.regs.d[rx as usize] & 0xFFFFFF00) | u32::from(result);

            self.set_bcd_flags(result, carry, overflow);
            self.micro_ops.push(MicroOp::Internal(2));
        } else {
            // Memory mode: -(Ay),-(Ax)
            // Predec source, read source byte (A7 uses 2 for word alignment)
            let dec_src = if ry == 7 { 2 } else { 1 };
            let a_src = self.regs.a(ry as usize).wrapping_sub(dec_src);
            self.regs.set_a(ry as usize, a_src);
            self.addr = a_src;

            // Stash: is_add in bit 0, rx in bits 8-10, ry in bits 12-14
            self.addr2 = if is_add { 1 } else { 0 } | (u32::from(rx) << 8);

            self.micro_ops.push(MicroOp::Internal(2));
            self.queue_read_ops(Size::Byte);
            self.in_followup = true;
            self.followup_tag = 105;
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    fn set_bcd_flags(&mut self, result: u8, carry: bool, overflow: bool) {
        use crate::flags::{C, V, X, Z};
        // X and C: set to carry
        self.regs.sr = if carry {
            self.regs.sr | X | C
        } else {
            self.regs.sr & !(X | C)
        };
        // Z: only cleared, never set (preserves from previous)
        if result != 0 {
            self.regs.sr &= !Z;
        }
        // N: set from MSB of result
        self.regs.sr = if result & 0x80 != 0 {
            self.regs.sr | 0x0008
        } else {
            self.regs.sr & !0x0008
        };
        // V: "undefined" per spec but real hardware sets it from BCD correction overflow
        self.regs.sr = if overflow {
            self.regs.sr | V
        } else {
            self.regs.sr & !V
        };
    }

    /// Tag 105: BCD memory src read complete. Predec dst and read.
    fn bcd_mem_src_read(&mut self) {
        let src = self.data as u8;
        let rx = ((self.addr2 >> 8) & 7) as u8;

        // Predec destination (A7 uses 2 for word alignment)
        let dec_dst = if rx == 7 { 2u32 } else { 1 };
        let a_dst = self.regs.a(rx as usize).wrapping_sub(dec_dst);
        self.regs.set_a(rx as usize, a_dst);
        self.addr = a_dst;

        // Stash source in data2
        self.data2 = u32::from(src);

        self.queue_read_ops(Size::Byte);
        self.followup_tag = 106;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 106: BCD memory dst read complete. ALU + write.
    fn bcd_mem_dst_read(&mut self) {
        let is_add = self.addr2 & 1 != 0;
        let src = self.data2 as u8;
        let dst = self.data as u8;
        let x = self.x_flag();

        let (result, carry, overflow) = if is_add {
            self.bcd_add(src, dst, x)
        } else {
            self.bcd_sub(dst, src, x)
        };

        self.set_bcd_flags(result, carry, overflow);
        self.data = u32::from(result);

        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(Size::Byte);
    }
}
