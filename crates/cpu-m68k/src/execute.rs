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
                21 => self.move_writeback_abslong(),
                _ => self.illegal_instruction(),
            }
            return;
        }

        self.size = size;
        self.program_space_access = false;
        self.irc_consumed_count = 0;
        self.move_src_was_memory = !Self::src_has_data(&src);
        self.src_postinc_undo = None;
        self.src_predec_undo = None;
        self.dst_reg_undo = None;
        self.pre_move_sr = None;
        self.pre_move_vc = None;
        self.deferred_fetch_count = 0;
        self.deferred_index = false;
        self.abslong_pending = false;

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
                // Only undo (An)+ on read AE for Long size. The real 68000
                // keeps the word-size increment committed even on read AE.
                if size == Size::Long {
                    self.src_postinc_undo = Some((r, inc));
                }
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
                    if self.move_src_was_memory {
                        // Memory source: defer BOTH ext word FetchIRCs.
                        // Consume first word now (IRC still has it), skip tag=11.
                        // The second word will be consumed in writeback after
                        // a deferred FetchIRC refills IRC.
                        self.data2 = u32::from(self.consume_irc_deferred()) << 16;
                        self.abslong_pending = true;
                        self.move_finalize();
                    } else {
                        self.data2 = u32::from(self.consume_irc()) << 16;
                        self.in_followup = true;
                        self.followup_tag = 11;
                        self.micro_ops.push(MicroOp::Execute);
                    }
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
    ///
    /// For memory-source MOVE, uses deferred IRC consumption — the FetchIRC
    /// refill happens after the source read, not before. On the real 68000,
    /// destination ext word bus cycles don't occur before the source read.
    fn move_calc_dst_ext(&mut self, dst: AddrMode) {
        let deferred = self.move_src_was_memory;
        match dst {
            AddrMode::AddrIndDisp(r) => {
                let disp = if deferred {
                    self.consume_irc_deferred() as i16
                } else {
                    self.consume_irc() as i16
                };
                self.addr2 = (self.regs.a(r as usize) as i32).wrapping_add(i32::from(disp)) as u32;
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = if deferred {
                    self.consume_irc_deferred()
                } else {
                    self.consume_irc()
                };
                self.addr2 = self.calc_index_ea(self.regs.a(r as usize), ext);
                if deferred {
                    self.deferred_index = true;
                } else {
                    self.micro_ops.push(MicroOp::Internal(2)); // Index overhead
                }
            }
            AddrMode::AbsShort => {
                self.addr2 = if deferred {
                    self.consume_irc_deferred() as i16 as i32 as u32
                } else {
                    self.consume_irc() as i16 as i32 as u32
                };
            }
            _ => {}
        }
    }

    /// Tag 10: consume first destination ext word.
    fn move_dst_ext1(&mut self) {
        let (_, _, dst) = self.move_decode();
        match dst {
            AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_) | AddrMode::AbsShort => {
                // Single ext word destination — move_calc_dst_ext handles deferral
                self.move_calc_dst_ext(dst);
                self.move_finalize();
            }
            AddrMode::AbsLong => {
                if self.move_src_was_memory {
                    // Memory source: defer both AbsLong FetchIRCs.
                    // Consume first word now (IRC has it from src FetchIRC refill).
                    // Second word deferred to writeback.
                    self.data2 = u32::from(self.consume_irc_deferred()) << 16;
                    self.abslong_pending = true;
                    self.move_finalize();
                } else {
                    // Register source: immediate FetchIRC for second word.
                    self.data2 = u32::from(self.consume_irc()) << 16;
                    self.followup_tag = 11;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Tag 11: consume second destination ext word (AbsLong).
    fn move_dst_ext2(&mut self) {
        // This is the last destination ext word — defer FetchIRC for memory sources.
        let lo = if self.move_src_was_memory {
            self.consume_irc_deferred()
        } else {
            self.consume_irc()
        };
        self.addr2 = self.data2 | u32::from(lo);
        self.move_finalize();
    }

    /// All ext words consumed — queue bus ops.
    fn move_finalize(&mut self) {
        let (size, src, dst) = self.move_decode();
        let is_movea = matches!(dst, AddrMode::AddrReg(_));
        let src_has_data = Self::src_has_data(&src);

        // Calculate destination address for 0-ext-word memory modes.
        // For memory-source MOVE (!src_has_data), defer -(An)/(An)+ register
        // updates to move_writeback — the source read happens first on the
        // real 68000, and if it triggers AE the dest register must be unchanged.
        match dst {
            AddrMode::AddrInd(r) => self.addr2 = self.regs.a(r as usize),
            AddrMode::AddrIndPostInc(r) => {
                self.addr2 = self.regs.a(r as usize);
                if src_has_data {
                    let inc = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                    self.dst_reg_undo = Some((r, self.addr2));
                    self.regs.set_a(r as usize, self.addr2.wrapping_add(inc));
                }
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                let orig = self.regs.a(r as usize);
                self.addr2 = orig.wrapping_sub(dec);
                if src_has_data {
                    // -(An) predecrement is only undone on write AE for MOVE.l.
                    // MOVE.w commits the predecrement even on write AE.
                    if size == Size::Long {
                        self.dst_reg_undo = Some((r, orig));
                    }
                    self.regs.set_a(r as usize, self.addr2);
                }
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
                    if !is_movea {
                        if size == Size::Long {
                            // The real 68000 evaluates MOVE flags in stages.
                            // On write AE, the frame SR reflects how far the
                            // evaluation got before the fault:
                            //
                            // (An)/(An)+: write starts immediately — no flag
                            //   evaluation. Full pre_move_sr restore.
                            // -(An): Internal(2) gives time — all flags committed.
                            // d16(An)/d8(An,Xn): FetchIRC sets N,Z but V,C are
                            //   cleared during the write cycle (aborted). Partial
                            //   restore: keep N,Z but revert V,C.
                            // abs.w/abs.l: all flags fully committed during the
                            //   FetchIRC cycle(s). No restore needed.
                            if matches!(dst,
                                AddrMode::AddrInd(_)
                                | AddrMode::AddrIndPostInc(_))
                            {
                                self.pre_move_sr = Some(self.regs.sr);
                            } else if matches!(dst,
                                AddrMode::AddrIndDisp(_)
                                | AddrMode::AddrIndIndex(_))
                            {
                                self.pre_move_vc = Some(self.regs.sr);
                            }
                            // -(An), abs.w, abs.l: no save — flags fully committed
                        }
                        self.set_flags_move(self.data, size);
                    }
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
        // Source read completed successfully — no need to undo (An)+/-(An)
        self.src_postinc_undo = None;
        self.src_predec_undo = None;

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
                // Memory → memory: apply deferred dest register updates,
                // set flags, then queue write.
                match dst {
                    AddrMode::AddrIndPostInc(r) => {
                        let inc = if size == Size::Byte && r == 7 { 2 } else { size.bytes() };
                        self.dst_reg_undo = Some((r, self.regs.a(r as usize)));
                        self.regs.set_a(r as usize, self.addr2.wrapping_add(inc));
                    }
                    AddrMode::AddrIndPreDec(r) => {
                        // -(An) predecrement only undone on write AE for MOVE.l.
                        if size == Size::Long {
                            self.dst_reg_undo = Some((r, self.regs.a(r as usize)));
                        }
                        self.regs.set_a(r as usize, self.addr2);
                    }
                    _ => {}
                }
                // Memory sources: flags always committed for normal execution.
                // For MOVE.l write AE to (An)/(An)+: the 68000's 16-bit ALU
                // evaluates flags from the LAST WORD read (low word). The write
                // starts immediately, so the AE frame SR reflects lo-word flags.
                // Normal execution still uses full-long flags.
                if !is_movea {
                    if size == Size::Long && matches!(dst,
                        AddrMode::AddrInd(_)
                        | AddrMode::AddrIndPostInc(_)
                        | AddrMode::AbsLong)
                    {
                        // Compute lo-word-based flags for AE restoration
                        let pre_sr = self.regs.sr;
                        self.set_flags_move(self.data, size); // Full long (normal exec)
                        // Build SR with lo-word N,Z and cleared V,C
                        let lo = self.data as u16;
                        let mut lo_sr = pre_sr & !0x000F;
                        if lo == 0 { lo_sr |= 0x0004; } // Z from lo word
                        if lo & 0x8000 != 0 { lo_sr |= 0x0008; } // N from lo word
                        self.pre_move_sr = Some(lo_sr);
                    } else {
                        self.set_flags_move(self.data, size);
                    }
                }

                if self.abslong_pending {
                    // AbsLong destination: two-stage writeback.
                    // Push the deferred FetchIRC (refills IRC with second AbsLong word),
                    // then continue to tag 21 to consume the second word and write.
                    for _ in 0..self.deferred_fetch_count {
                        self.micro_ops.push(MicroOp::FetchIRC);
                    }
                    self.deferred_fetch_count = 0;
                    self.in_followup = true;
                    self.followup_tag = 21;
                    self.micro_ops.push(MicroOp::Execute);
                } else {
                    // Normal path: push deferred FetchIRCs and write.
                    for _ in 0..self.deferred_fetch_count {
                        self.micro_ops.push(MicroOp::FetchIRC);
                    }
                    self.deferred_fetch_count = 0;
                    if self.deferred_index {
                        self.micro_ops.push(MicroOp::Internal(2));
                        self.deferred_index = false;
                    }
                    self.addr = self.addr2;
                    self.queue_write_ops(size);
                }
            }
        }
    }

    /// Tag 21: AbsLong destination writeback continuation.
    ///
    /// Called after the deferred FetchIRC has refilled IRC with the second
    /// AbsLong destination word. Consumes that word, computes the full
    /// destination address, and queues the write.
    fn move_writeback_abslong(&mut self) {
        let (size, _, _) = self.move_decode();
        // IRC holds the second AbsLong word (refilled by the deferred FetchIRC).
        // Consume the value WITHOUT queuing FetchIRC at the front — on the real
        // 68000, the IRC refill happens AFTER the write bus cycle, not before it.
        // Queuing FetchIRC before the write would add 4 cycles to the instruction
        // before any write AE fires, causing the AE handler to start too late.
        let lo = self.irc;
        self.irc_consumed_count += 1;
        self.addr2 = self.data2 | u32::from(lo);
        self.addr = self.addr2;
        self.abslong_pending = false;
        self.in_followup = false;
        self.followup_tag = 0;
        self.queue_write_ops(size);
        // FetchIRC refill queued AFTER the write — matches real 68000 bus order.
        self.micro_ops.push(MicroOp::FetchIRC);
    }

    // ================================================================
    // LEA
    // ================================================================
    //
    // Encoding: 0100 RRR 111 MMMRRR
    //   RRR = destination address register
    //   MMMRRR = source EA (control modes only)
    //
    // Loads effective address into An. No memory access, no flag changes.
    // Followup tag 30 = AbsLong second extension word.

    pub(crate) fn exec_lea(&mut self) {
        let dst_reg = ((self.ir >> 9) & 7) as usize;
        let src_mode = ((self.ir >> 3) & 7) as u8;
        let src_reg = (self.ir & 7) as u8;
        let src = match AddrMode::decode(src_mode, src_reg) {
            Some(m) => m,
            None => { self.illegal_instruction(); return; }
        };

        // Handle followup for AbsLong second extension word
        if self.in_followup && self.followup_tag == 30 {
            let lo = self.consume_irc();
            let addr = self.data2 | u32::from(lo);
            self.regs.set_a(dst_reg, addr);
            self.in_followup = false;
            self.followup_tag = 0;
            return;
        }

        match src {
            AddrMode::AddrInd(r) => {
                // LEA (An),Am — 4 cycles total. Instant compute.
                self.regs.set_a(dst_reg, self.regs.a(r as usize));
            }
            AddrMode::AddrIndDisp(r) => {
                // LEA d16(An),Am — 8 cycles.
                let disp = self.consume_irc() as i16;
                let addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(i32::from(disp)) as u32;
                self.regs.set_a(dst_reg, addr);
            }
            AddrMode::AddrIndIndex(r) => {
                // LEA d8(An,Xn),Am — 12 cycles.
                // Index calc takes 4 internal cycles (no overlap with bus ops).
                let ext = self.consume_irc();
                let addr = self.calc_index_ea(self.regs.a(r as usize), ext);
                self.regs.set_a(dst_reg, addr);
                self.micro_ops.push(MicroOp::Internal(4));
            }
            AddrMode::AbsShort => {
                // LEA xxx.w,Am — 8 cycles.
                let addr = self.consume_irc() as i16 as i32 as u32;
                self.regs.set_a(dst_reg, addr);
            }
            AddrMode::AbsLong => {
                // LEA xxx.l,Am — 12 cycles (two extension words).
                self.data2 = u32::from(self.consume_irc()) << 16;
                self.in_followup = true;
                self.followup_tag = 30;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::PcDisp => {
                // LEA d16(PC),Am — 8 cycles.
                let base = self.irc_addr;
                let disp = self.consume_irc() as i16;
                let addr = (base as i32).wrapping_add(i32::from(disp)) as u32;
                self.regs.set_a(dst_reg, addr);
            }
            AddrMode::PcIndex => {
                // LEA d8(PC,Xn),Am — 12 cycles.
                let base = self.irc_addr;
                let ext = self.consume_irc();
                let addr = self.calc_index_ea(base, ext);
                self.regs.set_a(dst_reg, addr);
                self.micro_ops.push(MicroOp::Internal(4));
            }
            _ => {
                // Invalid EA for LEA: Dn, An, (An)+, -(An), Immediate
                self.illegal_instruction();
            }
        }
    }
}
