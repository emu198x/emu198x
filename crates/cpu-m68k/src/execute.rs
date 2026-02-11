//! Instruction execution for the 68000.
//!
//! Contains the actual instruction implementations that are called from
//! decode.rs. Each instruction handler:
//! 1. Reads operands (from registers, from IRC via consume_irc(), etc.)
//! 2. Performs the operation
//! 3. Writes results
//! 4. Queues micro-ops for any remaining bus activity

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::cpu::Cpu68000;
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// MOVEQ: sign-extend 8-bit immediate to 32 bits, write to Dn, set flags.
    ///
    /// Encoding: 0111 RRR 0 DDDDDDDD
    /// Timing: 4 cycles (just the FetchIRC from start_next_instruction)
    pub(crate) fn exec_moveq(&mut self) {
        let reg = ((self.ir >> 9) & 7) as usize;
        let data = (self.ir & 0xFF) as i8 as i32 as u32;
        self.regs.d[reg] = data;
        self.set_flags_move(data, Size::Long);
    }

    // ================================================================
    // MOVE / MOVEA
    // ================================================================
    //
    // Encoding: 00SS DDD MMM sss SSS
    //   SS = size (01=byte, 11=word, 10=long)
    //   DDD = dst register, MMM = dst mode (reversed from standard)
    //   sss = src mode, SSS = src register
    //
    // Followup tags:
    //   1  = source AbsLong/Imm.Long second word
    //  10  = destination first ext word
    //  11  = destination AbsLong second word
    //  20  = writeback after source memory read

    /// Decode MOVE opcode fields from IR.
    fn move_decode(&self) -> (Size, AddrMode, AddrMode) {
        let op = self.ir;
        let size = Size::from_move_bits(((op >> 12) & 3) as u8).unwrap_or(Size::Word);
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let dst_reg = ((op >> 9) & 7) as u8;
        let src = AddrMode::decode(src_mode, src_reg).unwrap_or(AddrMode::DataReg(0));
        let dst = AddrMode::decode(dst_mode, dst_reg).unwrap_or(AddrMode::DataReg(0));
        (size, src, dst)
    }

    /// Returns true if the source mode means data is already in self.data (no bus read needed).
    fn src_has_data(src: &AddrMode) -> bool {
        matches!(src, AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate)
    }

    /// MOVE / MOVEA entry point.
    pub(crate) fn exec_move(&mut self) {
        let op = self.ir;
        let size = match Size::from_move_bits(((op >> 12) & 3) as u8) {
            Some(s) => s,
            None => { self.illegal_instruction(); return; }
        };
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;

        // MOVE.b to An is illegal
        if size == Size::Byte && dst_mode == 1 {
            self.illegal_instruction();
            return;
        }

        let src = match AddrMode::decode(src_mode, src_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        if self.in_followup {
            match self.followup_tag {
                1 => self.move_src_ext2(),
                10 => self.move_dst_ext1(),
                11 => self.move_dst_ext2(),
                20 => self.move_writeback(),
                _ => self.illegal_instruction(),
            }
            return;
        }

        self.size = size;
        self.program_space_access = false;

        // Process source EA — sets self.data (register/imm) or self.addr (memory)
        let src_consumed_irc = match src {
            AddrMode::DataReg(r) => {
                self.data = self.read_data_reg(r, size);
                false
            }
            AddrMode::AddrReg(r) => {
                self.data = self.regs.a(r as usize) & size.mask();
                false
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                false
            }
            AddrMode::AddrIndPostInc(r) => {
                let a = self.regs.a(r as usize);
                let inc = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                self.regs.set_a(r as usize, a.wrapping_add(inc));
                self.addr = a;
                false
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                let a = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, a);
                self.addr = a;
                self.micro_ops.push(MicroOp::Internal(2)); // PreDec overhead
                false
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr = (self.regs.a(r as usize) as i32).wrapping_add(i32::from(disp)) as u32;
                true
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2)); // Index overhead
                true
            }
            AddrMode::AbsShort => {
                self.addr = self.consume_irc() as i16 as i32 as u32;
                true
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 1;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::PcDisp => {
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                self.addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.program_space_access = true;
                true
            }
            AddrMode::PcIndex => {
                let base = self.irc_addr;
                let ext = self.consume_irc();
                self.addr = self.calc_index_ea(base, ext);
                self.program_space_access = true;
                self.micro_ops.push(MicroOp::Internal(2)); // Index overhead
                true
            }
            AddrMode::Immediate => {
                match size {
                    Size::Byte => {
                        self.data = u32::from(self.consume_irc()) & 0xFF;
                        true
                    }
                    Size::Word => {
                        self.data = u32::from(self.consume_irc());
                        true
                    }
                    Size::Long => {
                        self.data2 = u32::from(self.consume_irc()) << 16;
                        self.in_followup = true;
                        self.followup_tag = 1;
                        self.micro_ops.push(MicroOp::Execute);
                        return;
                    }
                }
            }
        };

        self.move_after_source(src_consumed_irc);
    }

    /// Tag 1: consume second ext word for AbsLong or Immediate.Long source.
    fn move_src_ext2(&mut self) {
        let (_, src, _) = self.move_decode();
        let lo = self.consume_irc();
        match src {
            AddrMode::AbsLong => {
                self.addr = self.data2 | u32::from(lo);
            }
            AddrMode::Immediate => {
                self.data = self.data2 | u32::from(lo);
            }
            _ => { self.illegal_instruction(); return; }
        }
        self.move_after_source(true);
    }

    /// Common path after source EA is fully resolved.
    /// Determines whether destination needs ext words and either finalizes
    /// or defers to tag 10/11.
    fn move_after_source(&mut self, src_consumed_irc: bool) {
        let (_, _, dst) = self.move_decode();

        match dst {
            // Destinations needing 0 ext words
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::AddrInd(_)
            | AddrMode::AddrIndPostInc(_) | AddrMode::AddrIndPreDec(_) => {
                self.move_finalize();
            }
            // Destinations needing 1 ext word
            AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_) | AddrMode::AbsShort => {
                if !src_consumed_irc {
                    self.move_calc_dst_ext(dst);
                    self.move_finalize();
                } else {
                    self.in_followup = true;
                    self.followup_tag = 10;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            // Destinations needing 2 ext words
            AddrMode::AbsLong => {
                if !src_consumed_irc {
                    self.data2 = u32::from(self.consume_irc()) << 16;
                    self.in_followup = true;
                    self.followup_tag = 11;
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    self.in_followup = true;
                    self.followup_tag = 10;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            // Can't write to these
            _ => self.illegal_instruction(),
        }
    }

    /// Calculate destination EA from a single ext word (consumed from IRC).
    fn move_calc_dst_ext(&mut self, dst: AddrMode) {
        match dst {
            AddrMode::AddrIndDisp(r) => {
                let disp = self.consume_irc() as i16;
                self.addr2 = (self.regs.a(r as usize) as i32).wrapping_add(i32::from(disp)) as u32;
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.consume_irc();
                self.addr2 = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.micro_ops.push(MicroOp::Internal(2)); // Index overhead
            }
            AddrMode::AbsShort => {
                self.addr2 = self.consume_irc() as i16 as i32 as u32;
            }
            _ => {}
        }
    }

    /// Tag 10: consume first destination ext word.
    fn move_dst_ext1(&mut self) {
        let (_, _, dst) = self.move_decode();
        match dst {
            AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_) | AddrMode::AbsShort => {
                self.move_calc_dst_ext(dst);
                self.move_finalize();
            }
            AddrMode::AbsLong => {
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.followup_tag = 11;
                self.micro_ops.push(MicroOp::Execute);
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 11: consume second destination ext word (AbsLong).
    fn move_dst_ext2(&mut self) {
        let lo = self.consume_irc();
        self.addr2 = self.data2 | u32::from(lo);
        self.move_finalize();
    }

    /// All ext words consumed — queue bus ops.
    fn move_finalize(&mut self) {
        let (size, src, dst) = self.move_decode();
        let is_movea = matches!(dst, AddrMode::AddrReg(_));
        let src_has_data = Self::src_has_data(&src);

        // Calculate destination address for 0-ext-word memory modes
        match dst {
            AddrMode::AddrInd(r) => self.addr2 = self.regs.a(r as usize),
            AddrMode::AddrIndPostInc(r) => {
                self.addr2 = self.regs.a(r as usize);
                let inc = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                self.regs.set_a(r as usize, self.addr2.wrapping_add(inc));
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                self.addr2 = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, self.addr2);
            }
            _ => {} // Register destinations or addr2 already set
        }

        self.in_followup = false;
        self.followup_tag = 0;

        if src_has_data {
            // Source data already in self.data — write destination directly
            match dst {
                AddrMode::DataReg(r) => {
                    self.write_data_reg(r, self.data, size);
                    if !is_movea { self.set_flags_move(self.data, size); }
                }
                AddrMode::AddrReg(r) => {
                    let val = if size == Size::Word {
                        self.data as u16 as i16 as i32 as u32
                    } else {
                        self.data
                    };
                    self.regs.set_a(r as usize, val);
                }
                _ => {
                    // Register/immediate → memory
                    if !is_movea { self.set_flags_move(self.data, size); }
                    self.addr = self.addr2;
                    self.program_space_access = false;
                    self.queue_write_ops(size);
                }
            }
        } else {
            // Source is memory — queue read, then writeback
            self.queue_read_ops(size);
            self.in_followup = true;
            self.followup_tag = 20;
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Tag 20: writeback after source memory read.
    fn move_writeback(&mut self) {
        let (size, _, dst) = self.move_decode();
        let is_movea = matches!(dst, AddrMode::AddrReg(_));

        self.in_followup = false;
        self.followup_tag = 0;
        self.program_space_access = false;

        match dst {
            AddrMode::DataReg(r) => {
                self.write_data_reg(r, self.data, size);
                if !is_movea { self.set_flags_move(self.data, size); }
            }
            AddrMode::AddrReg(r) => {
                let val = if size == Size::Word {
                    self.data as u16 as i16 as i32 as u32
                } else {
                    self.data
                };
                self.regs.set_a(r as usize, val);
            }
            _ => {
                // Memory → memory: set flags, queue write
                if !is_movea { self.set_flags_move(self.data, size); }
                self.addr = self.addr2;
                self.queue_write_ops(size);
            }
        }
    }
}
