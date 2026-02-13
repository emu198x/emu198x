//! Arithmetic instruction execution: ADD, SUB, CMP, ADDA, SUBA, CMPA.
//!
//! These instructions share a common EA resolution pattern:
//! - Opmode determines direction (EA→Dn, Dn→EA, EA→An)
//! - Source operand is resolved from EA or register
//! - ALU operation produces result and flags
//!
//! Followup tags:
//!   40 = AbsLong/Imm.Long second extension word
//!   41 = ALU operation after memory read (EA→Dn / EA→An direction)
//!   42 = Read-modify-write: read complete, ALU + write (Dn→EA direction)

use crate::addressing::AddrMode;
use crate::alu::{self, Size};
use crate::cpu::Cpu68000;
use crate::microcode::MicroOp;

/// Which ALU operation to perform.
#[derive(Debug, Clone, Copy)]
enum AluOp {
    Add,
    Sub,
    Cmp,
}

impl Cpu68000 {
    // ================================================================
    // ADD / SUB  (0xD / 0x9)
    // ================================================================
    //
    // Encoding: TTTT RRR OOO MMMRRR
    //   TTTT = 1101 (ADD) or 1001 (SUB)
    //   RRR = data/address register
    //   OOO = opmode:
    //     000 = .b EA+Dn→Dn   001 = .w EA+Dn→Dn   010 = .l EA+Dn→Dn
    //     011 = ADDA/SUBA.w    100 = .b Dn+EA→EA    101 = .w Dn+EA→EA
    //     110 = .l Dn+EA→EA    111 = ADDA/SUBA.l

    pub(crate) fn exec_add_sub(&mut self, is_add: bool) {
        let op = self.ir;
        let reg = ((op >> 9) & 7) as u8;
        let opmode = ((op >> 6) & 7) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        // Handle followups
        if self.in_followup {
            match self.followup_tag {
                40 => { self.arith_ext2(is_add); return; }
                41 => { self.arith_alu(is_add); return; }
                42 => { self.arith_rmw_alu(is_add); return; }
                60 | 61 => { self.exec_addx_subx(is_add); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        self.program_space_access = false;

        match opmode {
            // EA → Dn (byte/word/long)
            0 | 1 | 2 => {
                let size = Size::from_bits(opmode).unwrap();
                self.size = size;
                self.addr2 = u32::from(reg); // stash Dn index
                self.resolve_ea_read(&ea, size, AluOp::if_add(is_add), 41);
            }
            // ADDA/SUBA.w: EA → An (word, sign-extend to long)
            3 => {
                self.size = Size::Word;
                self.addr2 = u32::from(reg) | 0x100; // flag: address register
                self.resolve_ea_read(&ea, Size::Word, AluOp::if_add(is_add), 41);
            }
            // Dn → EA (byte/word/long, memory-only read-modify-write)
            // BUT: EA mode 0 = ADDX/SUBX Dy,Dx; EA mode 1 = ADDX/SUBX -(Ay),-(Ax)
            4 | 5 | 6 => {
                if ea_mode == 0 || ea_mode == 1 {
                    self.exec_addx_subx(is_add);
                    return;
                }
                let size = Size::from_bits(opmode & 3).unwrap();
                self.size = size;
                self.data2 = self.read_data_reg(reg, size); // stash Dn value as source
                self.resolve_ea_rmw(&ea, size, 42);
            }
            // ADDA/SUBA.l: EA → An (long)
            7 => {
                self.size = Size::Long;
                self.addr2 = u32::from(reg) | 0x100; // flag: address register
                self.resolve_ea_read(&ea, Size::Long, AluOp::if_add(is_add), 41);
            }
            _ => self.illegal_instruction(),
        }
    }

    // ================================================================
    // CMP / CMPA / EOR  (0xB)
    // ================================================================
    //
    // Encoding: 1011 RRR OOO MMMRRR
    //   OOO:
    //     000 = CMP.b EA,Dn   001 = CMP.w EA,Dn   010 = CMP.l EA,Dn
    //     011 = CMPA.w EA,An
    //     100 = EOR.b Dn→EA   101 = EOR.w Dn→EA   110 = EOR.l Dn→EA
    //     111 = CMPA.l EA,An
    //   Special: CMPM if opmode=1xx and EA mode=001 (postinc)

    pub(crate) fn exec_cmp_eor(&mut self) {
        let op = self.ir;
        let reg = ((op >> 9) & 7) as u8;
        let opmode = ((op >> 6) & 7) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        // Handle followups
        if self.in_followup {
            match self.followup_tag {
                40 => { self.cmp_eor_ext2(); return; }
                41 => { self.cmp_eor_alu(); return; }
                42 => { self.eor_rmw_alu(); return; }
                62 | 63 => { self.exec_cmpm(); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        self.program_space_access = false;

        match opmode {
            // CMP.b/w/l EA,Dn
            0 | 1 | 2 => {
                let size = Size::from_bits(opmode).unwrap();
                self.size = size;
                self.addr2 = u32::from(reg); // stash Dn index
                self.resolve_ea_read(&ea, size, AluOp::Cmp, 41);
            }
            // CMPA.w EA,An
            3 => {
                self.size = Size::Word;
                self.addr2 = u32::from(reg) | 0x100; // address register
                self.resolve_ea_read(&ea, Size::Word, AluOp::Cmp, 41);
            }
            // EOR.b/w/l Dn→EA
            // BUT: EA mode 1 = CMPM (Ay)+,(Ax)+
            4 | 5 | 6 => {
                if ea_mode == 1 {
                    self.exec_cmpm();
                    return;
                }
                let size = Size::from_bits(opmode & 3).unwrap();
                self.size = size;
                self.data2 = self.read_data_reg(reg, size); // stash Dn value
                // EOR to register?
                if matches!(ea, AddrMode::DataReg(_)) {
                    let dst_r = ea_reg;
                    let dst_val = self.read_data_reg(dst_r, size);
                    let result = dst_val ^ self.data2;
                    self.write_data_reg(dst_r, result, size);
                    self.set_flags_logic(result, size);
                    if size == Size::Long {
                        self.micro_ops.push(MicroOp::Internal(4));
                    }
                } else {
                    self.resolve_ea_rmw(&ea, size, 42);
                }
            }
            // CMPA.l EA,An
            7 => {
                self.size = Size::Long;
                self.addr2 = u32::from(reg) | 0x100;
                self.resolve_ea_read(&ea, Size::Long, AluOp::Cmp, 41);
            }
            _ => self.illegal_instruction(),
        }
    }

    // ================================================================
    // Shared EA resolution
    // ================================================================

    /// Resolve EA for a read-only source operand.
    ///
    /// If data is available immediately (register/immediate), performs the ALU
    /// operation and returns. Otherwise, queues bus ops and sets up a followup.
    fn resolve_ea_read(&mut self, ea: &AddrMode, size: Size, alu_op: AluOp, done_tag: u8) {
        match ea {
            AddrMode::DataReg(r) => {
                self.data = self.read_data_reg(*r, size);
                self.perform_alu_ea_to_reg(alu_op);
            }
            AddrMode::AddrReg(r) => {
                self.data = self.regs.a(*r as usize);
                self.perform_alu_ea_to_reg(alu_op);
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
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
                self.followup_tag = done_tag;
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
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                // Two ext words: staged approach
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 40; // Second ext word stage
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
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
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::Immediate => {
                match size {
                    Size::Byte => {
                        self.data = u32::from(self.consume_irc()) & 0xFF;
                        self.perform_alu_ea_to_reg(alu_op);
                    }
                    Size::Word => {
                        self.data = u32::from(self.consume_irc());
                        self.perform_alu_ea_to_reg(alu_op);
                    }
                    Size::Long => {
                        // Two ext words: staged approach
                        self.data2 = u32::from(self.consume_irc()) << 16;
                        self.in_followup = true;
                        self.followup_tag = 40;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }
        }
    }

    /// Resolve EA for read-modify-write (Dn→EA direction).
    /// self.data2 must already contain the source Dn value.
    fn resolve_ea_rmw(&mut self, ea: &AddrMode, size: Size, done_tag: u8) {
        match ea {
            // RMW to data register (no bus access)
            AddrMode::DataReg(r) => {
                self.data = self.read_data_reg(*r, size);
                self.addr2 = u32::from(*r); // stash dest reg
                // Inline ALU for register destination
                self.in_followup = true;
                self.followup_tag = done_tag;
                // For register-to-register, the Execute runs immediately (same cycle)
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
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
                self.followup_tag = done_tag;
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
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = done_tag;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data = self.data2; // preserve source in data
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 40;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    // ================================================================
    // Followup handlers
    // ================================================================

    /// Tag 40: AbsLong/Imm.Long second extension word.
    fn arith_ext2(&mut self, is_add: bool) {
        let lo = self.consume_irc();
        let opmode = ((self.ir >> 6) & 7) as u8;

        // Determine if this was AbsLong address or Immediate data
        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;
        let ea = AddrMode::decode(ea_mode, ea_reg).unwrap_or(AddrMode::DataReg(0));

        match ea {
            AddrMode::AbsLong => {
                self.addr = self.data2 | u32::from(lo);
                if opmode >= 4 && opmode <= 6 {
                    // RMW direction: read from addr, then ALU + write
                    // Restore source from data (saved in resolve_ea_rmw)
                    self.data2 = self.data;
                    self.queue_read_ops(self.size);
                    self.followup_tag = 42;
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    // EA→Reg direction: read from addr
                    self.queue_read_ops(self.size);
                    self.followup_tag = 41;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            AddrMode::Immediate => {
                self.data = self.data2 | u32::from(lo);
                let alu_op = if is_add { AluOp::Add } else { AluOp::Sub };
                self.perform_alu_ea_to_reg(alu_op);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 40 for CMP/EOR.
    fn cmp_eor_ext2(&mut self) {
        let lo = self.consume_irc();
        let opmode = ((self.ir >> 6) & 7) as u8;
        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;
        let ea = AddrMode::decode(ea_mode, ea_reg).unwrap_or(AddrMode::DataReg(0));

        match ea {
            AddrMode::AbsLong => {
                self.addr = self.data2 | u32::from(lo);
                if opmode >= 4 && opmode <= 6 {
                    // EOR RMW
                    self.data2 = self.data;
                    self.queue_read_ops(self.size);
                    self.followup_tag = 42;
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    self.queue_read_ops(self.size);
                    self.followup_tag = 41;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            AddrMode::Immediate => {
                self.data = self.data2 | u32::from(lo);
                self.perform_alu_ea_to_reg(AluOp::Cmp);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 41: ALU after memory read (EA→Dn/An direction) for ADD/SUB.
    fn arith_alu(&mut self, is_add: bool) {
        let alu_op = if is_add { AluOp::Add } else { AluOp::Sub };
        self.perform_alu_ea_to_reg(alu_op);
    }

    /// Tag 41 for CMP/EOR.
    fn cmp_eor_alu(&mut self) {
        let opmode = ((self.ir >> 6) & 7) as u8;
        if opmode >= 4 && opmode <= 6 {
            // This shouldn't happen for CMP direction
            self.illegal_instruction();
        } else {
            self.perform_alu_ea_to_reg(AluOp::Cmp);
        }
    }

    /// Tag 42: Read-modify-write ALU (Dn→EA direction) for ADD/SUB.
    fn arith_rmw_alu(&mut self, is_add: bool) {
        let size = self.size;
        let src = self.data2; // Dn value (stashed earlier)
        let dst = self.data;  // Value read from memory

        let (result, new_sr) = if is_add {
            alu::add(src, dst, size, self.regs.sr)
        } else {
            alu::sub(src, dst, size, self.regs.sr)
        };

        self.regs.sr = new_sr;
        self.data = result;

        // Determine if destination is register or memory
        let ea_mode = ((self.ir >> 3) & 7) as u8;
        if ea_mode == 0 {
            // DataReg destination
            let ea_reg = (self.ir & 7) as u8;
            self.write_data_reg(ea_reg, result, size);
            self.in_followup = false;
            self.followup_tag = 0;
            if size == Size::Long {
                self.micro_ops.push(MicroOp::Internal(4));
            }
        } else {
            // Memory destination: write back
            self.in_followup = false;
            self.followup_tag = 0;
            self.queue_write_ops(size);
        }
    }

    /// Tag 42 for EOR RMW.
    fn eor_rmw_alu(&mut self) {
        let size = self.size;
        let src = self.data2; // Dn value
        let dst = self.data;  // Memory value

        let result = src ^ dst;
        self.set_flags_logic(result, size);
        self.data = result;

        let ea_mode = ((self.ir >> 3) & 7) as u8;
        if ea_mode == 0 {
            let ea_reg = (self.ir & 7) as u8;
            self.write_data_reg(ea_reg, result, size);
            self.in_followup = false;
            self.followup_tag = 0;
            if size == Size::Long {
                self.micro_ops.push(MicroOp::Internal(4));
            }
        } else {
            self.in_followup = false;
            self.followup_tag = 0;
            self.queue_write_ops(size);
        }
    }

    // ================================================================
    // ALU helpers
    // ================================================================

    /// Perform ALU operation for EA→Dn/An direction.
    /// self.data = source value from EA, self.addr2 = dest register.
    fn perform_alu_ea_to_reg(&mut self, alu_op: AluOp) {
        let size = self.size;
        let reg_idx = (self.addr2 & 0xFF) as usize;
        let is_addr_reg = self.addr2 & 0x100 != 0;

        self.in_followup = false;
        self.followup_tag = 0;

        if is_addr_reg {
            // ADDA/SUBA/CMPA: destination is address register
            let src = if size == Size::Word {
                // Word-size: sign-extend source to 32 bits
                self.data as u16 as i16 as i32 as u32
            } else {
                self.data
            };
            let dst = self.regs.a(reg_idx);

            match alu_op {
                AluOp::Add => {
                    // ADDA: full 32-bit add, no flags affected
                    self.regs.set_a(reg_idx, dst.wrapping_add(src));
                }
                AluOp::Sub => {
                    // SUBA: full 32-bit sub, no flags affected
                    self.regs.set_a(reg_idx, dst.wrapping_sub(src));
                }
                AluOp::Cmp => {
                    // CMPA: full 32-bit compare, set NZVC, preserve X
                    let (_, new_sr) = alu::sub(src, dst, Size::Long, self.regs.sr);
                    let x_preserved = self.regs.sr & 0x0010;
                    self.regs.sr = (new_sr & !0x0010) | x_preserved;
                }
            }

            // Timing depends on operation and source type:
            // CMPA: always Internal(2) regardless of source
            // ADDA/SUBA register/immediate: Internal(4)
            // ADDA/SUBA long memory: Internal(2) (overlaps with bus)
            // ADDA/SUBA word memory: Internal(4)
            match alu_op {
                AluOp::Cmp => {
                    self.micro_ops.push(MicroOp::Internal(2));
                }
                AluOp::Add | AluOp::Sub => {
                    let ea_mode = ((self.ir >> 3) & 7) as u8;
                    let ea_reg = (self.ir & 7) as u8;
                    let ea = AddrMode::decode(ea_mode, ea_reg)
                        .unwrap_or(AddrMode::DataReg(0));
                    let from_memory = !matches!(
                        ea,
                        AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
                    );
                    if from_memory && size == Size::Long {
                        self.micro_ops.push(MicroOp::Internal(2));
                    } else {
                        self.micro_ops.push(MicroOp::Internal(4));
                    }
                }
            }
        } else {
            // ADD/SUB/CMP: destination is data register
            let dst = self.read_data_reg(reg_idx as u8, size);
            let src = self.data;

            match alu_op {
                AluOp::Add => {
                    let (result, new_sr) = alu::add(src, dst, size, self.regs.sr);
                    self.regs.sr = new_sr;
                    self.write_data_reg(reg_idx as u8, result, size);
                }
                AluOp::Sub => {
                    let (result, new_sr) = alu::sub(src, dst, size, self.regs.sr);
                    self.regs.sr = new_sr;
                    self.write_data_reg(reg_idx as u8, result, size);
                }
                AluOp::Cmp => {
                    // CMP: compare only, don't write result, don't touch X flag
                    let (_, new_sr) = alu::sub(src, dst, size, self.regs.sr);
                    let x_preserved = self.regs.sr & 0x0010;
                    self.regs.sr = (new_sr & !0x0010) | x_preserved;
                }
            }

            // Long-size operations take extra internal cycles.
            // Timing differs between ADD/SUB and CMP:
            //   ADD/SUB.l reg/imm: Internal(4) [8 total]
            //   ADD/SUB.l memory:  Internal(2)
            //   CMP.l reg/imm:     Internal(2) [6 total]
            //   CMP.l memory:      no extra    [overlaps with bus]
            if size == Size::Long {
                let ea_mode = ((self.ir >> 3) & 7) as u8;
                let ea_reg = (self.ir & 7) as u8;
                let ea = AddrMode::decode(ea_mode, ea_reg)
                    .unwrap_or(AddrMode::DataReg(0));
                let is_reg_or_imm = matches!(
                    ea,
                    AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
                );
                match alu_op {
                    AluOp::Add | AluOp::Sub => {
                        if is_reg_or_imm {
                            self.micro_ops.push(MicroOp::Internal(4));
                        } else {
                            self.micro_ops.push(MicroOp::Internal(2));
                        }
                    }
                    AluOp::Cmp => {
                        // CMP.l always needs Internal(2), whether source is
                        // register/immediate or memory
                        self.micro_ops.push(MicroOp::Internal(2));
                    }
                }
            }
        }
    }

    /// Set flags for logic operations (AND, OR, EOR): N,Z from result, clear V,C.
    pub(crate) fn set_flags_logic(&mut self, result: u32, size: Size) {
        self.set_flags_move(result, size);
    }

    // ================================================================
    // ADDQ / SUBQ  (group 0x5)
    // ================================================================
    //
    // Encoding: 0101 DDD O SS MMMRRR
    //   DDD = data (1-7, 0 encodes 8)
    //   O = 0 for ADDQ, 1 for SUBQ
    //   SS = size (00=byte, 01=word, 10=long)
    //   MMMRRR = EA (all alterable modes)
    //
    // When destination is An: always operates on full 32 bits, no flags.
    // When SS=11: this is Scc/DBcc, not ADDQ/SUBQ.
    //
    // Followup tags:
    //   50 = AbsLong second extension word (for memory destination)
    //   51 = RMW writeback after read

    pub(crate) fn exec_addq_subq(&mut self) {
        let op = self.ir;
        let imm_raw = ((op >> 9) & 7) as u32;
        let imm = if imm_raw == 0 { 8 } else { imm_raw };
        let is_sub = op & 0x0100 != 0;
        let size_bits = ((op >> 6) & 3) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        // SS=11 is Scc/DBcc
        if size_bits == 3 {
            self.exec_scc_dbcc();
            return;
        }

        let size = match Size::from_bits(size_bits) {
            Some(s) => s,
            None => { self.illegal_instruction(); return; }
        };

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        // Handle followups
        if self.in_followup {
            match self.followup_tag {
                50 => { self.addq_subq_ext2(is_sub, imm); return; }
                51 => { self.addq_subq_rmw(is_sub, imm); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        self.program_space_access = false;
        self.size = size;

        match ea {
            AddrMode::DataReg(r) => {
                // ADDQ/SUBQ Dn: add/sub with flags
                let dst = self.read_data_reg(r, size);
                let (result, new_sr) = if is_sub {
                    alu::sub(imm, dst, size, self.regs.sr)
                } else {
                    alu::add(imm, dst, size, self.regs.sr)
                };
                self.regs.sr = new_sr;
                self.write_data_reg(r, result, size);
                // Long-size Dn: Internal(4) [8 total]
                if size == Size::Long {
                    self.micro_ops.push(MicroOp::Internal(4));
                }
            }
            AddrMode::AddrReg(r) => {
                // ADDQ/SUBQ An: full 32-bit, no flags, always Long-sized internally
                let a = self.regs.a(r as usize);
                if is_sub {
                    self.regs.set_a(r as usize, a.wrapping_sub(imm));
                } else {
                    self.regs.set_a(r as usize, a.wrapping_add(imm));
                }
                // An destination: always Internal(4) [8 total]
                self.micro_ops.push(MicroOp::Internal(4));
            }
            // Memory destinations: read-modify-write
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 51;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                let inc = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                self.regs.set_a(r as usize, a.wrapping_add(inc));
                if size == Size::Long {
                    self.src_postinc_undo = Some((r, inc));
                }
                self.addr = a;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 51;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                let a = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 51;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 51;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 51;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 51;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 50;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 50: AbsLong second extension word for ADDQ/SUBQ.
    fn addq_subq_ext2(&mut self, is_sub: bool, imm: u32) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(self.size);
        self.followup_tag = 51;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 51: RMW writeback for ADDQ/SUBQ memory.
    fn addq_subq_rmw(&mut self, is_sub: bool, imm: u32) {
        let size = self.size;
        let dst = self.data;
        let (result, new_sr) = if is_sub {
            alu::sub(imm, dst, size, self.regs.sr)
        } else {
            alu::add(imm, dst, size, self.regs.sr)
        };
        self.regs.sr = new_sr;
        self.data = result;
        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(size);
    }

    // ================================================================
    // ADDX / SUBX  (opmodes 4-6 of ADD/SUB group, EA mode 0 or 1)
    // ================================================================
    //
    // Encoding: TTTT RRR 1SS 00M YYY
    //   TTTT = 1101 (ADDX) or 1001 (SUBX)
    //   RRR = Rx (destination), YYY = Ry (source)
    //   SS = size (00=byte, 01=word, 10=long)
    //   M = 0: Dy,Dx  |  M = 1: -(Ay),-(Ax)
    //
    // Flags: X,N,V,C set normally. Z only cleared, never set.
    //
    // Timing:
    //   Dy,Dx:           byte/word 4, long 8
    //   -(Ay),-(Ax):     byte/word 18, long 30
    //
    // Followup tags:
    //   60 = src read complete for -(Ay),-(Ax), now read dst
    //   61 = dst read complete for -(Ay),-(Ax), ALU + write

    fn exec_addx_subx(&mut self, is_add: bool) {
        let op = self.ir;
        let rx = ((op >> 9) & 7) as u8; // destination
        let ry = (op & 7) as u8;         // source
        let opmode = ((op >> 6) & 7) as u8;
        let rm = (op >> 3) & 1;          // 0 = Dy,Dx; 1 = -(Ay),-(Ax)
        let size = Size::from_bits(opmode & 3).unwrap();

        // Handle followups for memory mode
        if self.in_followup {
            match self.followup_tag {
                60 => { self.addx_subx_read_dst(is_add); return; }
                61 => { self.addx_subx_write(is_add); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        self.size = size;

        if rm == 0 {
            // Register mode: Dy,Dx
            let src = self.read_data_reg(ry, size);
            let dst = self.read_data_reg(rx, size);
            let (result, new_sr) = if is_add {
                alu::addx(src, dst, size, self.regs.sr)
            } else {
                alu::subx(src, dst, size, self.regs.sr)
            };
            self.regs.sr = new_sr;
            self.write_data_reg(rx, result, size);
            if size == Size::Long {
                self.micro_ops.push(MicroOp::Internal(4));
            }
        } else {
            // Memory mode: -(Ay),-(Ax)
            // Phase 1: predec Ay, Internal(2), read source
            let original_a = self.regs.a(ry as usize);
            let dec = if size == Size::Byte && ry == 7 { 2 } else { size.bytes() };
            let a = original_a.wrapping_sub(dec);
            self.regs.set_a(ry as usize, a);
            self.addr = a; // source address
            if size == Size::Long {
                // For long: real 68000 decrements by 2 first, tries at An-2.
                // On AE, it undoes the predecrement entirely.
                self.predec_long_read = true;
                self.src_predec_undo = Some((ry, original_a));
            }

            // Stash Rx index and Ax address for later
            self.addr2 = u32::from(rx);

            self.micro_ops.push(MicroOp::Internal(2));
            self.queue_read_ops(size);
            self.in_followup = true;
            self.followup_tag = 60;
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Tag 60: Source read complete for ADDX/SUBX -(Ay),-(Ax).
    /// Now predec Ax and read destination.
    fn addx_subx_read_dst(&mut self, _is_add: bool) {
        let rx = (self.addr2 & 0xFF) as u8;
        let size = self.size;

        // Clear source predec undo (source read succeeded)
        self.src_predec_undo = None;

        // Save source value
        self.data2 = self.data;

        // Predec Ax
        let original_a = self.regs.a(rx as usize);
        let dec = if size == Size::Byte && rx == 7 { 2 } else { size.bytes() };
        let a = original_a.wrapping_sub(dec);
        self.regs.set_a(rx as usize, a);
        self.addr = a; // destination address (also write address)
        // For long size, set predec flag so AE reports fault at An-2
        self.predec_long_read = size == Size::Long;
        // Only undo destination predecrement for Long size.
        // The 68000 does long predecrement in two -2 steps; if the first
        // word read faults, it restores the partial -2. For word/byte the
        // full decrement is committed before the bus read.
        if size == Size::Long {
            self.src_predec_undo = Some((rx, original_a));
        }

        self.queue_read_ops(size);
        self.followup_tag = 61;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 61: Destination read complete for ADDX/SUBX -(Ay),-(Ax).
    /// Perform ALU and write back.
    fn addx_subx_write(&mut self, is_add: bool) {
        let size = self.size;
        let src = self.data2;
        let dst = self.data;

        let (result, new_sr) = if is_add {
            alu::addx(src, dst, size, self.regs.sr)
        } else {
            alu::subx(src, dst, size, self.regs.sr)
        };
        self.regs.sr = new_sr;
        self.data = result;

        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(size);
    }

    // ================================================================
    // CMPM  (Ay)+,(Ax)+
    // ================================================================
    //
    // Encoding: 1011 RRR 1SS 001 YYY
    //   RRR = Ax (destination for compare), YYY = Ay (source)
    //   SS = size (00=byte, 01=word, 10=long)
    //
    // Compare (Ay)+ with (Ax)+: (Ax)+ - (Ay)+, set flags, no writeback.
    // Both registers post-incremented.
    //
    // Timing: byte/word 12, long 20
    //
    // Followup tags:
    //   62 = src read complete, now read dst
    //   63 = dst read complete, perform compare

    fn exec_cmpm(&mut self) {
        let op = self.ir;
        let ax = ((op >> 9) & 7) as u8; // destination
        let ay = (op & 7) as u8;         // source
        let opmode = ((op >> 6) & 7) as u8;
        let size = Size::from_bits(opmode & 3).unwrap();

        // Handle followups
        if self.in_followup {
            match self.followup_tag {
                62 => { self.cmpm_read_dst(); return; }
                63 => { self.cmpm_compare(); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        self.size = size;
        self.addr2 = u32::from(ax); // stash Ax index

        // Phase 1: Read from (Ay)+
        let a = self.regs.a(ay as usize);
        let inc = if size == Size::Byte && ay == 7 { 2 } else { size.bytes() };
        self.regs.set_a(ay as usize, a.wrapping_add(inc));
        self.addr = a;

        // For Long reads, the 68000 does word-by-word postincrement. If AE fires
        // on ReadLongHi, An = original + 2 (partial). Undo only 2 of the 4.
        if size == Size::Long {
            self.src_postinc_undo = Some((ay, 2));
        }

        self.queue_read_ops(size);
        self.in_followup = true;
        self.followup_tag = 62;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 62: Source read complete for CMPM. Now read (Ax)+.
    fn cmpm_read_dst(&mut self) {
        let ax = (self.addr2 & 0xFF) as u8;
        let size = self.size;

        // Source (Ay) read succeeded — its postinc is committed.
        self.src_postinc_undo = None;

        // Save source value
        self.data2 = self.data;

        // Read from (Ax)+
        let a = self.regs.a(ax as usize);
        let inc = if size == Size::Byte && ax == 7 { 2 } else { size.bytes() };
        self.regs.set_a(ax as usize, a.wrapping_add(inc));
        self.addr = a;

        // CMPM: the 68000 undoes Ax postincrement on AE (all sizes).
        self.src_postinc_undo = Some((ax, inc as u32));

        self.queue_read_ops(size);
        self.followup_tag = 63;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 63: Destination read complete for CMPM. Perform compare.
    fn cmpm_compare(&mut self) {
        let size = self.size;
        let src = self.data2; // (Ay)+ value
        let dst = self.data;  // (Ax)+ value

        // CMP: dst - src, set NZVC, preserve X
        let (_, new_sr) = alu::sub(src, dst, size, self.regs.sr);
        let x_preserved = self.regs.sr & 0x0010;
        self.regs.sr = (new_sr & !0x0010) | x_preserved;

        self.in_followup = false;
        self.followup_tag = 0;
        // No writeback, no extra internal cycles
    }
}

impl AluOp {
    fn if_add(is_add: bool) -> Self {
        if is_add { AluOp::Add } else { AluOp::Sub }
    }
}
