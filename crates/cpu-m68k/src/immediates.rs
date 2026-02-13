//! Immediate ALU instructions: ADDI, SUBI, CMPI, ANDI, ORI, EORI.
//!
//! All in opcode group 0x0: 0000 OOO 0SS MMMRRR
//!   OOO = operation (000=ORI, 001=ANDI, 010=SUBI, 011=ADDI, 101=EORI, 110=CMPI)
//!   SS  = size (00=byte, 01=word, 10=long)
//!   MMMRRR = destination EA
//!
//! Pattern: read immediate from IRC, resolve EA, perform ALU, write back.
//! CMPI is compare-only (no writeback). ANDI/ORI/EORI to CCR/SR are special.
//!
//! Followup tags:
//!   70 = immediate long: second word → defers to 77
//!   71 = memory read complete: perform ALU + writeback
//!   72 = AbsLong destination: second address word
//!   73 = bit memory read complete
//!   74 = bit AbsLong ext2
//!   75 = bit imm: deferred EA resolution
//!   77 = immediate ALU: deferred EA resolution

use crate::addressing::AddrMode;
use crate::alu::{self, Size};
use crate::cpu::Cpu68000;
use crate::microcode::MicroOp;

/// Which immediate operation.
#[derive(Debug, Clone, Copy)]
enum ImmOp {
    Ori,
    Andi,
    Subi,
    Addi,
    Eori,
    Cmpi,
}

impl Cpu68000 {
    /// Decode and execute group 0x0 instructions.
    ///
    /// This includes: ORI, ANDI, SUBI, ADDI, EORI, CMPI,
    /// BTST/BCHG/BCLR/BSET (immediate and register bit number), and MOVEP.
    pub(crate) fn exec_group0(&mut self) {
        let op = self.ir;

        // Handle followups first
        if self.in_followup {
            match self.followup_tag {
                70 | 71 | 72 | 77 => { self.imm_followup(); return; }
                73 | 74 | 75 => { self.bit_followup(); return; }
                76 => { self.movep_transfer(); return; }
                _ => { self.illegal_instruction(); return; }
            }
        }

        // Bit 8 distinguishes register bit ops / MOVEP from immediate ops
        if op & 0x0100 != 0 {
            // Bit 8 set: register bit ops or MOVEP
            let ea_mode = ((op >> 3) & 7) as u8;
            if ea_mode == 1 {
                // MOVEP: 0000 DDD 1 OO 001 AAA
                self.exec_movep();
            } else {
                // Register bit op: 0000 RRR 1 TT MMMRRR
                self.exec_bit_reg();
            }
            return;
        }

        // Bit 8 clear: immediate operations
        let sub_op = ((op >> 9) & 7) as u8;

        match sub_op {
            0b000 => self.exec_imm_alu(ImmOp::Ori),
            0b001 => self.exec_imm_alu(ImmOp::Andi),
            0b010 => self.exec_imm_alu(ImmOp::Subi),
            0b011 => self.exec_imm_alu(ImmOp::Addi),
            0b100 => self.exec_bit_imm(),
            0b101 => self.exec_imm_alu(ImmOp::Eori),
            0b110 => self.exec_imm_alu(ImmOp::Cmpi),
            // 0b111 = MOVES (68010+) — illegal on 68000
            _ => self.illegal_instruction(),
        }
    }

    /// Execute an immediate ALU instruction.
    fn exec_imm_alu(&mut self, imm_op: ImmOp) {
        let op = self.ir;
        let size_bits = ((op >> 6) & 3) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        // Size 0b11 with certain EA modes = special: to CCR or to SR
        if size_bits == 3 {
            self.illegal_instruction();
            return;
        }

        let size = match Size::from_bits(size_bits) {
            Some(s) => s,
            None => { self.illegal_instruction(); return; }
        };

        // Check for xxI to CCR (byte, EA=imm i.e. mode=7,reg=4... no)
        // Actually: ORI to CCR = 0x003C, ANDI to CCR = 0x023C, EORI to CCR = 0x0A3C
        // These are: ea_mode=7, ea_reg=4 with size=byte
        // ORI to SR = 0x007C, ANDI to SR = 0x027C, EORI to SR = 0x0A7C
        // These are: ea_mode=7, ea_reg=4 with size=word
        if ea_mode == 7 && ea_reg == 4 {
            match imm_op {
                ImmOp::Ori | ImmOp::Andi | ImmOp::Eori => {
                    self.exec_imm_to_sr(imm_op, size);
                    return;
                }
                _ => {
                    self.illegal_instruction();
                    return;
                }
            }
        }

        // EA validation is deferred to tag 77 / imm_resolve_ea_deferred.
        self.size = size;
        self.program_space_access = false;

        // Stash the operation type in addr2 high bits
        self.addr2 = imm_op_code(imm_op) as u32;

        // Read immediate value from IRC.
        // consume_irc queues a FetchIRC but it won't run until the current
        // Execute completes. Defer EA resolution to a followup so IRC is
        // refilled before any EA extension word is consumed.
        match size {
            Size::Byte => {
                self.data2 = u32::from(self.consume_irc()) & 0xFF;
                self.in_followup = true;
                self.followup_tag = 77;
                self.micro_ops.push(MicroOp::Execute);
            }
            Size::Word => {
                self.data2 = u32::from(self.consume_irc());
                self.in_followup = true;
                self.followup_tag = 77;
                self.micro_ops.push(MicroOp::Execute);
            }
            Size::Long => {
                // Two words: high from IRC now, low from IRC after FetchIRC
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 70;
                self.micro_ops.push(MicroOp::Execute);
            }
        }
    }

    /// Tag 70: Long immediate second word. Consume low word, then defer
    /// EA resolution to tag 77 so FetchIRC refills IRC before any EA
    /// extension word is consumed.
    fn imm_long_ext2(&mut self) {
        let lo = self.consume_irc();
        self.data2 |= u32::from(lo);

        // Defer EA resolution — FetchIRC must refill IRC first
        self.followup_tag = 77;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 77: Deferred EA resolution for immediate ALU instructions.
    /// IRC has been refilled by the preceding FetchIRC, so consume_irc
    /// for EA extension words will return the correct value.
    fn imm_resolve_ea_deferred(&mut self) {
        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;
        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };
        self.imm_resolve_ea(&ea, self.size);
    }

    /// Resolve EA destination for an immediate instruction.
    /// self.data2 = immediate value.
    fn imm_resolve_ea(&mut self, ea: &AddrMode, size: Size) {
        match ea {
            AddrMode::DataReg(r) => {
                // Register destination: perform ALU immediately
                let dst = self.read_data_reg(*r, size);
                let imm_op = imm_op_from_code(self.addr2 as u8);
                self.imm_perform_alu(imm_op, self.data2, dst, *r, size);
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 71;
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
                self.followup_tag = 71;
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
                self.followup_tag = 71;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 71;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 71;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(size);
                self.in_followup = true;
                self.followup_tag = 71;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                // Save immediate in data field temporarily
                let saved_imm = self.data2;
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.data = saved_imm; // stash imm in data
                self.in_followup = true;
                self.followup_tag = 72;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 72: AbsLong second address word for immediate instructions.
    fn imm_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.data2 = self.data; // restore immediate from data

        self.queue_read_ops(self.size);
        self.followup_tag = 71;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 71: Memory read complete, perform ALU and write back.
    fn imm_mem_alu(&mut self) {
        let size = self.size;
        let imm = self.data2;
        let dst = self.data;
        let imm_op = imm_op_from_code(self.addr2 as u8);

        let (result, new_sr) = match imm_op {
            ImmOp::Addi => alu::add(imm, dst, size, self.regs.sr),
            ImmOp::Subi | ImmOp::Cmpi => alu::sub(imm, dst, size, self.regs.sr),
            ImmOp::Andi => {
                let r = imm & dst;
                let sr = logic_flags(r, size, self.regs.sr);
                (r, sr)
            }
            ImmOp::Ori => {
                let r = imm | dst;
                let sr = logic_flags(r, size, self.regs.sr);
                (r, sr)
            }
            ImmOp::Eori => {
                let r = imm ^ dst;
                let sr = logic_flags(r, size, self.regs.sr);
                (r, sr)
            }
        };

        // Save original X before overwriting SR — CMPI preserves X.
        let x_preserved = self.regs.sr & 0x0010;
        self.regs.sr = new_sr;
        self.in_followup = false;
        self.followup_tag = 0;

        if matches!(imm_op, ImmOp::Cmpi) {
            // CMPI: compare only, no writeback. Restore original X flag.
            self.regs.sr = (new_sr & !0x0010) | x_preserved;
            // CMPI memory: no extra internal cycles
        } else {
            self.data = result;
            self.queue_write_ops(size);
        }
    }

    /// Followup dispatcher for immediate instructions.
    fn imm_followup(&mut self) {
        match self.followup_tag {
            70 => self.imm_long_ext2(),
            71 => self.imm_mem_alu(),
            72 => self.imm_abslong_ext2(),
            77 => self.imm_resolve_ea_deferred(),
            _ => self.illegal_instruction(),
        }
    }

    /// Perform ALU for register destination.
    fn imm_perform_alu(&mut self, imm_op: ImmOp, imm: u32, dst: u32, reg: u8, size: Size) {
        self.in_followup = false;
        self.followup_tag = 0;

        let (result, new_sr) = match imm_op {
            ImmOp::Addi => alu::add(imm, dst, size, self.regs.sr),
            ImmOp::Subi | ImmOp::Cmpi => alu::sub(imm, dst, size, self.regs.sr),
            ImmOp::Andi => {
                let r = imm & dst;
                let sr = logic_flags(r, size, self.regs.sr);
                (r, sr)
            }
            ImmOp::Ori => {
                let r = imm | dst;
                let sr = logic_flags(r, size, self.regs.sr);
                (r, sr)
            }
            ImmOp::Eori => {
                let r = imm ^ dst;
                let sr = logic_flags(r, size, self.regs.sr);
                (r, sr)
            }
        };

        match imm_op {
            ImmOp::Cmpi => {
                // CMPI: compare only, preserve X flag
                let x_preserved = self.regs.sr & 0x0010;
                self.regs.sr = (new_sr & !0x0010) | x_preserved;
                // CMPI.l Dn: Internal(2), byte/word: no extra
                if size == Size::Long {
                    self.micro_ops.push(MicroOp::Internal(2));
                }
            }
            _ => {
                self.regs.sr = new_sr;
                self.write_data_reg(reg, result, size);
                // ADDI/SUBI.l Dn: Internal(4), byte/word: no extra
                // ANDI/ORI/EORI.l Dn: Internal(4), byte/word: no extra
                if size == Size::Long {
                    self.micro_ops.push(MicroOp::Internal(4));
                }
            }
        }
    }

    // ================================================================
    // Bit operations: BTST / BCHG / BCLR / BSET
    // ================================================================
    //
    // Register: 0000 RRR 1TT MMMRRR — bit# in Dn
    //   TT: 00=BTST, 01=BCHG, 10=BCLR, 11=BSET
    //   Dn dest: long (bit# mod 32). Memory dest: byte (bit# mod 8).
    //
    // Immediate: 0000 100 0TT MMMRRR — bit# in extension word
    //   TT same encoding.
    //
    // Timing (register bit#):
    //   BTST Dn,Dn: 6    BTST Dn,<mem>: 4+EA
    //   BCHG Dn,Dn: 8 (bit<16) or 10 (bit>=16)   BCHG Dn,<mem>: 8+EA
    //   BCLR Dn,Dn: 10 (bit<16) or 12 (bit>=16)  BCLR Dn,<mem>: 8+EA
    //   BSET Dn,Dn: 8 (bit<16) or 10 (bit>=16)   BSET Dn,<mem>: 8+EA
    //
    // Timing (immediate bit#):
    //   BTST #,Dn: 10     BTST #,<mem>: 8+EA
    //   BCHG #,Dn: 12     BCHG #,<mem>: 12+EA
    //   BCLR #,Dn: 14     BCLR #,<mem>: 12+EA
    //   BSET #,Dn: 12     BSET #,<mem>: 12+EA
    //
    // Followup tags:
    //   73 = bit memory read complete (BTST: just test, others: modify + write)
    //   74 = bit AbsLong ext2
    //   75 = bit imm bit# consumed, now resolve EA

    /// Register bit operation: 0000 RRR 1TT MMMRRR
    fn exec_bit_reg(&mut self) {
        let op = self.ir;
        let dn = ((op >> 9) & 7) as u8;
        let bit_type = ((op >> 6) & 3) as u8; // 00=BTST 01=BCHG 10=BCLR 11=BSET
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let bit_num = self.regs.d[dn as usize];

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;

        match ea {
            AddrMode::DataReg(r) => {
                // Long-word operation: bit# mod 32
                let bit = bit_num % 32;
                let val = self.regs.d[r as usize];
                let z = val & (1 << bit) == 0;
                self.regs.sr = if z {
                    self.regs.sr | 0x0004
                } else {
                    self.regs.sr & !0x0004
                };

                match bit_type {
                    0 => {} // BTST: test only
                    1 => self.regs.d[r as usize] = val ^ (1 << bit),      // BCHG
                    2 => self.regs.d[r as usize] = val & !(1 << bit),     // BCLR
                    3 => self.regs.d[r as usize] = val | (1 << bit),      // BSET
                    _ => unreachable!(),
                }

                // Timing: DL-verified. Internal(N) + trailing FetchIRC(4) = total.
                let internal = match bit_type {
                    0 => 2u8,                                  // BTST: 6 total
                    2 => if bit >= 16 { 6 } else { 4 },       // BCLR: 8/10 total
                    _ => if bit >= 16 { 4 } else { 2 },       // BCHG/BSET: 6/8 total
                };
                self.micro_ops.push(MicroOp::Internal(internal));
            }
            _ => {
                // Memory: byte operation, bit# mod 8
                self.size = Size::Byte;
                // Stash bit_type and bit_num
                self.addr2 = u32::from(bit_type) | ((bit_num % 8) << 8);
                self.resolve_bit_ea_read(&ea);
            }
        }
    }

    /// Immediate bit operation: 0000 100 0TT MMMRRR
    fn exec_bit_imm(&mut self) {
        let op = self.ir;
        let bit_type = ((op >> 6) & 3) as u8;
        let ea_mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        // Bit number in extension word (IRC)
        let bit_num = u32::from(self.consume_irc());

        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        self.program_space_access = false;

        match ea {
            AddrMode::DataReg(r) => {
                // Long-word operation: bit# mod 32
                let bit = bit_num % 32;
                let val = self.regs.d[r as usize];
                let z = val & (1 << bit) == 0;
                self.regs.sr = if z {
                    self.regs.sr | 0x0004
                } else {
                    self.regs.sr & !0x0004
                };

                match bit_type {
                    0 => {} // BTST
                    1 => self.regs.d[r as usize] = val ^ (1 << bit),
                    2 => self.regs.d[r as usize] = val & !(1 << bit),
                    3 => self.regs.d[r as usize] = val | (1 << bit),
                    _ => unreachable!(),
                }

                // Timing: DL-verified. consume_irc FetchIRC(4) + Internal(N) +
                // trailing FetchIRC(4) = total. Same Internal values as register form.
                let internal = match bit_type {
                    0 => 2u8,                                  // BTST #,Dn: 10 total
                    2 => if bit >= 16 { 6 } else { 4 },       // BCLR #,Dn: 12/14 total
                    _ => if bit >= 16 { 4 } else { 2 },       // BCHG/BSET #,Dn: 10/12 total
                };
                self.micro_ops.push(MicroOp::Internal(internal));
            }
            _ => {
                // Memory: byte operation, bit# mod 8.
                // Defer EA resolution to tag 75 so FetchIRC (from consume_irc
                // above) refills IRC before any EA extension word is consumed.
                self.size = Size::Byte;
                self.addr2 = u32::from(bit_type) | ((bit_num % 8) << 8);
                self.in_followup = true;
                self.followup_tag = 75;
                self.micro_ops.push(MicroOp::Execute);
            }
        }
    }

    /// Resolve EA for bit operation (memory, byte).
    fn resolve_bit_ea_read(&mut self, ea: &AddrMode) {
        match ea {
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(*r as usize);
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(*r as usize);
                let inc = if *r == 7 { 2 } else { 1 };
                self.regs.set_a(*r as usize, a.wrapping_add(inc));
                self.addr = a;
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if *r == 7 { 2 } else { 1 };
                let a = self.regs.a(*r as usize).wrapping_sub(dec);
                self.regs.set_a(*r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(*r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(*r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 74;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::PcIndex => {
                let base = self.irc_addr;
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(base, ext);
                self.program_space_access = true;
                self.micro_ops.push(MicroOp::Internal(2));
                self.queue_read_ops(Size::Byte);
                self.in_followup = true;
                self.followup_tag = 73;
                self.micro_ops.push(MicroOp::Execute);
            }
            AddrMode::Immediate => {
                // BTST/BCHG/BCLR/BSET Dn,#imm — reads the immediate byte.
                // DL-verified: 10 total = FetchIRC(4) + Internal(2) + trailing(4).
                let val = u32::from(self.consume_irc()) & 0xFF;
                self.data = val;
                self.micro_ops.push(MicroOp::Internal(2));
                self.bit_mem_complete();
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Followup dispatcher for bit operations.
    fn bit_followup(&mut self) {
        match self.followup_tag {
            73 => self.bit_mem_complete(),
            74 => self.bit_abslong_ext2(),
            75 => self.bit_imm_resolve_ea(),
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 75: Deferred EA resolution for immediate bit ops.
    /// IRC has been refilled, so consume_irc for EA extension words is safe.
    fn bit_imm_resolve_ea(&mut self) {
        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;
        let ea = match AddrMode::decode(ea_mode, ea_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };
        self.resolve_bit_ea_read(&ea);
    }

    /// Tag 74: Bit op AbsLong second address word.
    fn bit_abslong_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr = self.data2 | u32::from(lo);
        self.queue_read_ops(Size::Byte);
        self.followup_tag = 73;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 73: Bit memory read complete — test + optional modify + optional write.
    fn bit_mem_complete(&mut self) {
        let bit_type = (self.addr2 & 0xFF) as u8;
        let bit = ((self.addr2 >> 8) & 0xFF) as u32;
        let val = self.data & 0xFF;

        let z = val & (1 << bit) == 0;
        self.regs.sr = if z {
            self.regs.sr | 0x0004
        } else {
            self.regs.sr & !0x0004
        };

        self.in_followup = false;
        self.followup_tag = 0;

        match bit_type {
            0 => {} // BTST: read-only, no writeback
            1 => { // BCHG
                self.data = (val ^ (1 << bit)) & 0xFF;
                self.queue_write_ops(Size::Byte);
            }
            2 => { // BCLR
                self.data = (val & !(1 << bit)) & 0xFF;
                self.queue_write_ops(Size::Byte);
            }
            3 => { // BSET
                self.data = (val | (1 << bit)) & 0xFF;
                self.queue_write_ops(Size::Byte);
            }
            _ => {}
        }
    }

    /// Execute ORI/ANDI/EORI to CCR or SR.
    fn exec_imm_to_sr(&mut self, imm_op: ImmOp, size: Size) {
        let imm = u32::from(self.consume_irc());

        self.in_followup = false;
        self.followup_tag = 0;

        match size {
            Size::Byte => {
                // to CCR: only affects low byte of SR
                let ccr = (self.regs.sr & 0xFF) as u32;
                let result = match imm_op {
                    ImmOp::Ori => ccr | (imm & 0x1F),
                    ImmOp::Andi => ccr & imm,
                    ImmOp::Eori => ccr ^ (imm & 0x1F),
                    _ => { self.illegal_instruction(); return; }
                };
                self.regs.sr = (self.regs.sr & 0xFF00) | (result as u16 & 0xFF);
                // 20 cycles total (3 reads, 0 writes):
                // consume_irc → FetchIRC(4) already queued
                // + dummy re-read of extension word: Internal(4)
                // + internal processing: Internal(8)
                // = 16 from Execute + start_next FetchIRC(4) = 20
                self.micro_ops.push(MicroOp::Internal(4));
                self.micro_ops.push(MicroOp::Internal(8));
            }
            Size::Word => {
                // to SR: affects full SR (privileged)
                if self.check_supervisor() { return; }
                let sr = u32::from(self.regs.sr);
                let result = match imm_op {
                    ImmOp::Ori => sr | imm,
                    ImmOp::Andi => sr & imm,
                    ImmOp::Eori => sr ^ imm,
                    _ => { self.illegal_instruction(); return; }
                };
                self.regs.sr = (result as u16) & crate::flags::SR_MASK;
                // 20 cycles total: same as CCR variant
                self.micro_ops.push(MicroOp::Internal(4));
                self.micro_ops.push(MicroOp::Internal(8));
            }
            _ => self.illegal_instruction(),
        }
    }
}

/// Encode ImmOp as a u8 for stashing in addr2.
fn imm_op_code(op: ImmOp) -> u8 {
    match op {
        ImmOp::Ori => 0,
        ImmOp::Andi => 1,
        ImmOp::Subi => 2,
        ImmOp::Addi => 3,
        ImmOp::Eori => 5,
        ImmOp::Cmpi => 6,
    }
}

/// Decode ImmOp from stashed u8.
fn imm_op_from_code(code: u8) -> ImmOp {
    match code {
        0 => ImmOp::Ori,
        1 => ImmOp::Andi,
        2 => ImmOp::Subi,
        3 => ImmOp::Addi,
        5 => ImmOp::Eori,
        6 => ImmOp::Cmpi,
        _ => ImmOp::Ori, // shouldn't happen
    }
}

/// Compute flags for logic operations: N,Z from result, clear V,C, preserve X.
fn logic_flags(result: u32, size: Size, sr: u16) -> u16 {
    use crate::flags::{C, N, V, Z};
    let mask = size.mask();
    let msb = size.msb_mask();
    let r = result & mask;

    let mut flags = sr & !(C | V | Z | N);
    // Preserve X (bit 4) — logic ops don't affect X
    if r == 0 {
        flags |= Z;
    }
    if r & msb != 0 {
        flags |= N;
    }
    flags
}

// ================================================================
// MOVEP — Move Peripheral Data
// ================================================================
// 0000 DDD 1 OO 001 AAA
// OO: 00=word read, 01=long read, 10=word write, 11=long write
// Transfers data between Dn and alternating bytes at d16(An),
// d16(An)+2, [d16(An)+4, d16(An)+6 for long].
//
// Read word:  Dn[15:8] <- (d16+An), Dn[7:0] <- (d16+An+2)
// Read long:  Dn[31:24] <- (d16+An), Dn[23:16] <- (d16+An+2),
//             Dn[15:8] <- (d16+An+4), Dn[7:0] <- (d16+An+6)
// Write word: (d16+An) <- Dn[15:8], (d16+An+2) <- Dn[7:0]
// Write long: (d16+An) <- Dn[31:24], (d16+An+2) <- Dn[23:16],
//             (d16+An+4) <- Dn[15:8], (d16+An+6) <- Dn[7:0]
//
// Timing: word=16, long=24
// Followup tag 76: multi-byte transfer loop

impl Cpu68000 {
    fn exec_movep(&mut self) {
        let op = self.ir;
        let dn = ((op >> 9) & 7) as usize;
        let opmode = ((op >> 6) & 3) as u8;
        let an = (op & 7) as usize;

        let disp = self.consume_irc() as i16;
        let base_addr = (self.regs.a(an) as i32)
            .wrapping_add(i32::from(disp)) as u32;

        self.program_space_access = false;
        self.addr = base_addr;

        // Pack into data2: opmode in bits 0-1, dn in bits 2-4, byte_index in bits 8-11
        self.data2 = u32::from(opmode)
            | ((dn as u32) << 2)
            | (0 << 8); // byte_index starts at 0

        let is_write = opmode & 2 != 0;
        let is_long = opmode & 1 != 0;
        let total_bytes = if is_long { 4u32 } else { 2 };

        if is_write {
            // Write first byte
            let shift = (total_bytes - 1) * 8;
            self.data = (self.regs.d[dn] >> shift) & 0xFF;
            self.micro_ops.push(MicroOp::WriteByte);
        } else {
            // Read first byte
            self.micro_ops.push(MicroOp::ReadByte);
        }

        // Store total byte count in bits 12-15
        self.data2 |= total_bytes << 12;

        self.in_followup = true;
        self.followup_tag = 76;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Tag 76: MOVEP multi-byte transfer loop.
    fn movep_transfer(&mut self) {
        let opmode = (self.data2 & 3) as u8;
        let dn = ((self.data2 >> 2) & 7) as usize;
        let byte_idx = ((self.data2 >> 8) & 0xF) as u32;
        let total = ((self.data2 >> 12) & 0xF) as u32;
        let is_write = opmode & 2 != 0;

        if !is_write {
            // Store the byte we just read into the correct position in Dn
            let shift = (total - 1 - byte_idx) * 8;
            let mask = 0xFFu32 << shift;
            self.regs.d[dn] = (self.regs.d[dn] & !mask)
                | ((self.data & 0xFF) << shift);
        }

        let next_idx = byte_idx + 1;
        if next_idx >= total {
            // All bytes transferred
            self.in_followup = false;
            self.followup_tag = 0;
            return;
        }

        // Advance to next byte (skip one byte = +2 addresses)
        self.addr = self.addr.wrapping_add(2);

        // Update byte_index in data2
        self.data2 = (self.data2 & !0x0F00) | (next_idx << 8);

        if is_write {
            let shift = (total - 1 - next_idx) * 8;
            self.data = (self.regs.d[dn] >> shift) & 0xFF;
            self.micro_ops.push(MicroOp::WriteByte);
        } else {
            self.micro_ops.push(MicroOp::ReadByte);
        }

        self.micro_ops.push(MicroOp::Execute);
    }
}
