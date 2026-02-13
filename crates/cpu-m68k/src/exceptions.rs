//! Exception handling for the 68000.
//!
//! Exception groups:
//! - Group 0: Reset, bus error, address error (highest priority)
//! - Group 1: Trace, interrupt, illegal instruction, privilege violation
//! - Group 2: TRAP, TRAPV, CHK, zero divide
//!
//! Standard exception frame (6 bytes): PC (long) + SR (word)
//! Group 0 exception frame (14 bytes): PC + SR + IR + fault addr + access info

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::bus::FunctionCode;
use crate::cpu::Cpu68000;
use crate::microcode::MicroOp;

/// Saved exception state for building the frame in stages.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExceptionState {
    /// Old SR to push (before entering supervisor mode).
    pub old_sr: u16,
    /// PC value to push in exception frame.
    pub return_pc: u32,
    /// Vector table address (vector * 4).
    pub vector_addr: u32,
    /// For group 0: opcode value to push as "IR" in the frame.
    pub frame_ir: u16,
    /// For group 0: fault address.
    pub frame_fault_addr: u32,
    /// For group 0: access info word.
    pub frame_access_info: u16,
    /// True if this is a group 0 exception (14-byte frame).
    pub is_group0: bool,
    /// Stage counter for multi-stage exception frame building.
    pub stage: u8,
}

impl Cpu68000 {
    /// Trigger a standard exception (group 1/2) by vector number.
    ///
    /// `extra_internal` adds cycles to the base internal processing time.
    /// Used by CHK to add 2 comparison cycles that don't overlap with a FetchIRC.
    pub(crate) fn exception(&mut self, vector: u8, extra_internal: u8) {
        let old_sr = self.regs.sr;
        self.regs.sr |= 0x2000; // Set S (supervisor)
        self.regs.sr &= !0x8000; // Clear T (trace)

        let return_pc = self.exception_pc_override
            .take()
            .unwrap_or(self.instr_start_pc);

        self.exc = ExceptionState {
            old_sr,
            return_pc,
            vector_addr: u32::from(vector) * 4,
            is_group0: false,
            ..Default::default()
        };

        self.micro_ops.clear();

        // Internal processing time depends on exception type:
        // 34 cycles total (6 internal): TRAP, TRAPV, illegal, privilege, trace, line-A/F
        // 38 cycles total (10 internal): zero divide, CHK base
        // CHK adds 2 extra via extra_internal for the bound comparison
        let internal = match vector {
            5 | 6 => 10,       // Zero divide, CHK
            _ => 6,            // TRAP, TRAPV, illegal, privilege, trace, line-A/F
        } + extra_internal;
        self.micro_ops.push(MicroOp::Internal(internal));

        // Push PC
        self.data = return_pc;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // Stage: push SR, then read vector
        self.in_followup = true;
        self.followup_tag = 0xFE;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Trigger an address error (group 0, vector 3).
    ///
    /// The 68000 address error fires when a word/long access targets an odd
    /// address. The exception frame is 14 bytes:
    ///   PC (4) + SR (2) + IR (2) + fault addr (4) + access info (2)
    pub(crate) fn address_error(&mut self, addr: u32, is_read: bool) {
        self.processing_group0 = true;

        // For UNLK: undo the A7 ← An modification.
        if let Some((was_supervisor, original_sp)) = self.sp_undo.take() {
            if was_supervisor {
                self.regs.ssp = original_sp;
            } else {
                self.regs.usp = original_sp;
            }
        }

        // For JSR/BSR: undo the return PC push. The real 68000 restores the
        // active SP to its pre-push value when FetchIRC at the odd target
        // triggers AE. Must restore the correct SP (SSP or USP) depending on
        // which mode the JSR/BSR ran in.
        if is_read {
            if let Some((was_supervisor, original_sp)) = self.jsr_push_undo.take() {
                if was_supervisor {
                    self.regs.ssp = original_sp;
                } else {
                    self.regs.usp = original_sp;
                }
            }
        }
        self.jsr_push_undo = None;

        // For DBcc: undo the Dn.w decrement. The real 68000 restores the
        // original Dn value when the branch to an odd target triggers AE.
        if is_read {
            if let Some((r, original_w)) = self.dbcc_dn_undo.take() {
                self.regs.d[r as usize] =
                    (self.regs.d[r as usize] & 0xFFFF_0000) | u32::from(original_w);
            }
        }
        self.dbcc_dn_undo = None;

        // For source (An)+, undo the post-increment on read AE.
        // The real 68000 doesn't apply the increment when the read faults.
        if is_read {
            if let Some((r, inc)) = self.src_postinc_undo.take() {
                let a = self.regs.a(r as usize);
                self.regs.set_a(r as usize, a.wrapping_sub(inc));
            }
            // For source -(An), undo the predecrement on read AE.
            // The real 68000 undoes the predecrement when the read faults.
            if let Some((r, original)) = self.src_predec_undo.take() {
                self.regs.set_a(r as usize, original);
            }
        }
        self.src_postinc_undo = None;
        self.src_predec_undo = None;

        // For write AE, undo destination register changes and conditionally
        // restore SR.
        if !is_read {
            if let Some((r, orig)) = self.dst_reg_undo.take() {
                self.regs.set_a(r as usize, orig);
            }
            // For MOVE.l reg/imm → (An)/(An)+/-(An): full SR restore.
            // The real 68000 doesn't evaluate ANY flags before the write bus
            // cycle for these 0-extension-word destination modes.
            if let Some(sr) = self.pre_move_sr.take() {
                self.regs.sr = sr;
            }
            // For MOVE.l reg/imm → d16(An)/d8(An,Xn)/abs: partial restore.
            // N,Z are evaluated during the destination extension word FetchIRC,
            // but V,C clearing happens during the write cycle (which was aborted).
            // Restore V,C from pre-instruction state, keep N,Z as computed.
            else if let Some(sr) = self.pre_move_vc.take() {
                let pre_vc = sr & 0x03; // V and C bits from before set_flags_move
                self.regs.sr = (self.regs.sr & !0x03) | pre_vc;
            }
        }
        self.dst_reg_undo = None;
        self.pre_move_sr = None;
        self.pre_move_vc = None;

        let old_sr = self.regs.sr;
        self.regs.sr |= 0x2000; // Set S
        self.regs.sr &= !0x8000; // Clear T

        // Frame IR: usually the current opcode. For MOVE.w write AE with -(An)
        // destination, the pipeline advance (IR ← IRC) happens before the write,
        // so the frame IR is the current IRC value at AE time.
        let frame_ir = if !is_read
            && (self.ir >> 12) == 3
            && ((self.ir >> 6) & 7) == 4
        {
            self.irc
        } else {
            self.ir
        };

        // Frame PC: complex formula derived from emu-m68k reference and DL tests.
        // Depends on instruction type, size, read/write, and addressing modes.
        let return_pc = self.compute_ae_frame_pc(is_read);

        // Function code
        let is_program = self.program_space_access;
        let fc = FunctionCode::from_flags(
            old_sr & 0x2000 != 0,
            is_program,
        );

        // Access info word: IR bits [15:5] + R/W + FC
        let access_info: u16 = (frame_ir & 0xFFE0)
            | (if is_read { 0x10 } else { 0 })
            | u16::from(fc.bits() & 0x07);

        // Fault address: for MOVE.l -(An) dest write AE, the 68000 reports
        // the address as An-2 (word-aligned initial decrement, not full long).
        let fault_addr = self.adjust_ae_fault_addr(addr, is_read);

        self.exc = ExceptionState {
            old_sr,
            return_pc,
            vector_addr: 3 * 4,
            frame_ir,
            frame_fault_addr: fault_addr,
            frame_access_info: access_info,
            is_group0: true,
            stage: 0,
        };

        self.micro_ops.clear();
        // Internal processing: 13 cycles base.
        // Together with the 1-tick AE detection (Execute + bus op cycle 0 → odd),
        // the total AE handler is 1 + 13 + 44 = 58 ticks for the simplest case
        // (no pre-AE extension word fetches).
        // Bus breakdown: 7 frame writes + 2 vector reads + 2 prefetches = 44 cycles.
        // Verified: Internal(12) caused the CPU to start executing the handler's
        // first instruction within the test budget; Internal(13) prevents this.
        //
        // MOVE to -(An) write AE: +4 extra cycles. On the real 68000, the
        // predecrement calculation and address bus setup take a full bus period
        // (4 cycles) longer than direct addressing modes. The 68000's two-stage
        // address latch means the decremented address reaches the bus pins one
        // bus cycle later, delaying the odd-address check accordingly.
        // Verified across 136 DL test cases: without this adjustment, PC ends up
        // 2 bytes too high (one extra handler FetchIRC completes within the test
        // cycle budget).
        let mut internal: u8 = 13;
        if !is_read {
            let top = (self.ir >> 12) & 0xF;
            let dst_mode = (self.ir >> 6) & 7;
            if matches!(top, 1 | 2 | 3) && dst_mode == 4 {
                // -(An): predecrement address setup adds 4 cycles.
                internal += 4;
            }
        }
        self.micro_ops.push(MicroOp::Internal(internal));

        // Push PC
        self.data = return_pc;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // Continue via staged followup
        self.in_followup = true;
        self.followup_tag = 0xFE;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Compute the frame PC for a MOVE address error.
    ///
    /// Empirically derived from 2500+ DL test cases per instruction.
    /// The formula depends on whether the fault is a read or write AE,
    /// and on the source/destination addressing modes.
    ///
    /// **Read AE** (source read faults on odd address):
    /// - Absolute source (xxx.w, xxx.l): `ISP + 2 + src_ext * 2`
    /// - -(An) + Size::Long: `ISP + 2`
    /// - -(An) + Size::Word: `ISP + 4`
    /// - All others: `ISP + 2`
    ///
    /// **Write AE** (destination write faults on odd address):
    /// - Register/immediate source: `ISP + 4 + (src_ext + max(dst_ext-1, 0)) * 2`
    /// - Memory source: `ISP + 4 + src_ext * 2`
    fn compute_ae_frame_pc(&mut self, is_read: bool) -> u32 {
        let top = (self.ir >> 12) & 0xF;
        let is_move = matches!(top, 1 | 2 | 3);
        if !is_move {
            return self.compute_ae_frame_pc_non_move();
        }
        // MOVE doesn't branch, so ae_from_fetch_irc should be false,
        // but clear it for safety.
        self.ae_from_fetch_irc = false;

        let size = Size::from_move_bits(((self.ir >> 12) & 3) as u8)
            .unwrap_or(Size::Word);
        let src = AddrMode::decode(((self.ir >> 3) & 7) as u8, (self.ir & 7) as u8)
            .unwrap_or(AddrMode::DataReg(0));
        let src_ext = Self::ext_words(&src, size);

        if is_read {
            // Read AE: fault during source operand fetch
            match src {
                AddrMode::AbsShort | AddrMode::AbsLong => {
                    // Absolute sources: PC advanced past consumed ext words
                    self.instr_start_pc
                        .wrapping_add(2 + u32::from(src_ext) * 2)
                }
                AddrMode::AddrIndPreDec(_) => {
                    // -(An): size-dependent (verified empirically)
                    if size == Size::Long {
                        self.instr_start_pc.wrapping_add(2)
                    } else {
                        self.instr_start_pc.wrapping_add(4)
                    }
                }
                _ => {
                    // All other sources: ISP + 2
                    self.instr_start_pc.wrapping_add(2)
                }
            }
        } else {
            // Write AE: fault during destination write
            let dst_mode = ((self.ir >> 6) & 7) as u8;
            let dst_reg = ((self.ir >> 9) & 7) as u8;
            let dst = AddrMode::decode(dst_mode, dst_reg)
                .unwrap_or(AddrMode::DataReg(0));
            let dst_ext = Self::ext_words(&dst, size);

            let src_is_register = matches!(
                src,
                AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
            );

            if src_is_register {
                // Register/immediate source: destination ext words affect frame PC,
                // but the first dst ext word doesn't add to the offset (it shares
                // the IRC slot with the pending FetchIRC).
                let extra = src_ext + dst_ext.saturating_sub(1);
                self.instr_start_pc.wrapping_add(4 + u32::from(extra) * 2)
            } else {
                // Memory source: only source ext words affect frame PC.
                // Destination ext words are "unwound" from the reported position.
                self.instr_start_pc.wrapping_add(4 + u32::from(src_ext) * 2)
            }
        }
    }

    /// Compute frame PC for non-MOVE address errors.
    ///
    /// Empirically derived from DL test cases across 44 instruction files.
    /// The base is ISP + 2, with adjustments for:
    /// - Immediate ext words (group 0): adds imm_words * 2
    /// - Absolute addressing: abs.w adds 2, abs.l adds 4
    /// - -(An) with word-size access: adds 2
    /// - ADDX/SUBX/CMPM: always ISP + 4
    /// - FetchIRC AE (branch/jump to odd target): self.regs.pc
    fn compute_ae_frame_pc_non_move(&mut self) -> u32 {
        // BSR FetchIRC AE: BSR jumps to an odd target. The real 68000 puts the
        // TARGET ADDRESS (= current PC) in the frame, not ISP + offset. BSR is
        // unique among branch/jump instructions in this regard — verified by DL
        // tests showing expected frame PC = target for all 618 BSR AE cases.
        // Other instructions (BRA, Bcc, JMP, JSR, RTS, RTE, RTR) use ISP+offset.
        if self.ae_from_fetch_irc {
            self.ae_from_fetch_irc = false;
            let top = (self.ir >> 12) & 0xF;
            let cond = (self.ir >> 8) & 0xF;
            if top == 0x6 {
                if cond == 1 {
                    // BSR: frame PC = target address (= current PC).
                    // Verified by DL tests: all 618 BSR AE cases match.
                    return self.regs.pc;
                }
                // BRA/Bcc: frame PC = ISP + 2 regardless of displacement size.
                // The displacement was consumed via consume_irc_deferred (16-bit)
                // or not at all (8-bit), but in both cases the pipeline hasn't
                // advanced past the extension word before the branch invalidates it.
                return self.instr_start_pc.wrapping_add(2);
            }
        }

        // MOVEM with PC-relative EA sets program_space_access for the function
        // code, but its frame PC uses the MOVEM formula (below), not the generic
        // program-space formula. Check MOVEM before the program_space_access branch.
        if self.program_space_access && (self.ir & 0xFB80) == 0x4880 {
            // Fall through to the MOVEM handler below.
        } else if self.program_space_access {
            let top = (self.ir >> 12) & 0xF;
            return match top {
                0x5 => {
                    // Group 5: ADDQ/SUBQ/Scc/DBcc. If this is DBcc (ea_mode=001),
                    // the displacement word was consumed: ISP + 4.
                    let ea_mode = ((self.ir >> 3) & 7) as u8;
                    if ea_mode == 1 {
                        self.instr_start_pc.wrapping_add(4)
                    } else {
                        self.instr_start_pc.wrapping_add(2)
                    }
                }
                0x6 => {
                    // Group 6: Bcc/BRA/BSR. 16-bit displacement (disp8=0) consumed
                    // the ext word: ISP + 4. 8-bit displacement: ISP + 2.
                    let disp8 = self.ir & 0xFF;
                    if disp8 == 0 {
                        self.instr_start_pc.wrapping_add(4)
                    } else {
                        self.instr_start_pc.wrapping_add(2)
                    }
                }
                _ => {
                    // JSR: frame PC = return address = ISP + 2 + ea_ext * 2.
                    // The JSR push was undone (jsr_push_undo), so the frame PC
                    // records where execution would have continued on return.
                    // JSR encoding: 0100 1110 10 MMMRRR ($4E80-$4EBF).
                    if self.ir & 0xFFC0 == 0x4E80 {
                        let ea_mode = ((self.ir >> 3) & 7) as u8;
                        let ea_reg = (self.ir & 7) as u8;
                        let ea_ext: u32 = match ea_mode {
                            5 | 6 => 1,  // d16(An), d8(An,Xn)
                            7 => match ea_reg {
                                0 | 2 | 3 => 1,  // abs.w, d16(PC), d8(PC,Xn)
                                1 => 2,           // abs.l
                                _ => 0,
                            },
                            _ => 0,  // (An): no ext words
                        };
                        return self.instr_start_pc.wrapping_add(2 + ea_ext * 2);
                    }
                    // JMP, RTS, RTE, RTR, etc.: ISP + 2
                    self.instr_start_pc.wrapping_add(2)
                }
            };
        }

        let top = (self.ir >> 12) & 0xF;
        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;

        // ADDX/SUBX -(An),-(An) and CMPM (An)+,(An)+: always ISP + 4.
        // These are groups 9/B/D, opmode 4-6, ea_mode 1 (memory operand).
        if matches!(top, 0x9 | 0xB | 0xD) {
            let opmode = (self.ir >> 6) & 7;
            if opmode >= 4 && opmode <= 6 && ea_mode == 1 {
                return self.instr_start_pc.wrapping_add(4);
            }
        }

        // UNLK: frame PC = ISP + 4. UNLK reads from the stack without consuming
        // any extension words. The real 68000 reports PC past both the opcode and
        // the prefetched IRC word. Opcode range: 0x4E58-0x4E5F.
        if self.ir & 0xFFF8 == 0x4E58 {
            return self.instr_start_pc.wrapping_add(4);
        }

        // -(An) with word-size data access adds 2 to the base offset.
        // The predecrement's Internal(2) lets the pipeline advance PC by one
        // extra word before the AE fires. Long-size doesn't add because the
        // first ReadLongHi starts before the extra FetchIRC completes.
        let predec_adj: u32 = if ea_mode == 4 {
            if self.ae_access_size() == Size::Word { 2 } else { 0 }
        } else {
            0
        };

        // Absolute addressing ext words count toward the frame PC offset.
        // Displacement ext words (d16, d8+idx) do NOT count — the 68000's
        // internal PC doesn't advance past these before the AE fires.
        let abs_adj: u32 = if ea_mode == 7 {
            match ea_reg {
                0 => 2, // abs.w: 1 ext word
                1 => 4, // abs.l: 2 ext words
                _ => 0,
            }
        } else {
            0
        };

        // MOVEM: consumes register mask ext word before the data EA.
        // The frame PC formula is completely different from other instructions:
        // ISP + 6 + ea_ext_words * 2 (empirically derived from DL tests).
        // Detect MOVEM: 0100 1x00 1x MMMRRR = (ir & 0xFB80) == 0x4880.
        if (self.ir & 0xFB80) == 0x4880 {
            let movem_ea_ext: u32 = match ea_mode {
                5 | 6 => 2,  // d16(An), d8(An,Xn): 1 ext word
                7 => match ea_reg {
                    0 => 2,   // abs.w: 1 ext word
                    1 => 4,   // abs.l: 2 ext words
                    2 | 3 => 2, // d16(PC), d8(PC,Xn): 1 ext word
                    _ => 0,
                },
                _ => 0,
            };
            return self.instr_start_pc.wrapping_add(6 + movem_ea_ext);
        }

        match top {
            // Group 0: immediate ops have immediate ext words in the base.
            // Static bit ops (#n) always have 1 ext word.
            // ALU immediate (ADDI etc.) has 1 for byte/word, 2 for long.
            0x0 => {
                let imm_ext = self.group0_imm_ext_words();
                self.instr_start_pc
                    .wrapping_add(2 + imm_ext * 2 + predec_adj + abs_adj)
            }
            // All other groups: base ISP + 2 with adjustments.
            _ => self.instr_start_pc
                .wrapping_add(2 + predec_adj + abs_adj),
        }
    }

    /// Determine the data access size from the instruction encoding.
    /// Used only for AE frame PC -(An) word detection.
    fn ae_access_size(&self) -> Size {
        let top = (self.ir >> 12) & 0xF;
        match top {
            // Standard size encoding in bits 7-6
            0x0 | 0x5 | 0xE => {
                Size::from_bits(((self.ir >> 6) & 3) as u8).unwrap_or(Size::Word)
            }
            // Group 4: many instructions with non-standard size encodings.
            0x4 => {
                // MOVEM (0100 1x00 1x MMMRRR): size_bits 2=Word, 3=Long.
                if (self.ir & 0xFB80) == 0x4880 {
                    if (self.ir >> 6) & 1 == 1 { Size::Long } else { Size::Word }
                }
                // CHK (0100 xxx 110): always word-size read.
                else if (self.ir >> 6) & 7 == 6 { Size::Word }
                else { Size::from_bits(((self.ir >> 6) & 3) as u8).unwrap_or(Size::Word) }
            }
            // Groups 8-D: opmode determines size
            0x8 | 0x9 | 0xB | 0xC | 0xD => {
                match (self.ir >> 6) & 7 {
                    0 | 4 => Size::Byte,
                    1 | 5 => Size::Word,
                    2 | 6 => Size::Long,
                    3 => Size::Word,  // DIVU / MULU / ADDA.w / CMPA.w / SUBA.w
                    7 => {
                        // Groups 8/C opmode 7 = DIVS/MULS: word-size memory read.
                        // Groups 9/B/D opmode 7 = SUBA.l/CMPA.l/ADDA.l: long access.
                        if top == 0x8 || top == 0xC { Size::Word } else { Size::Long }
                    }
                    _ => Size::Word,
                }
            }
            _ => Size::Word,
        }
    }

    /// Count immediate extension words for group 0 instructions.
    fn group0_imm_ext_words(&self) -> u32 {
        let secondary = ((self.ir >> 8) & 0xF) as u8;
        if secondary == 8 {
            1 // Static bit ops (BTST/BCHG/BCLR/BSET #n): always 1
        } else if ((self.ir >> 6) & 3) == 2 {
            2 // ALU immediate long: 2 ext words
        } else {
            1 // ALU immediate byte/word: 1 ext word
        }
    }

    /// Count extension words for a given addressing mode and size.
    fn ext_words(mode: &AddrMode, size: Size) -> u8 {
        match mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_)
            | AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_)
            | AddrMode::AddrIndPreDec(_) => 0,
            AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_)
            | AddrMode::AbsShort | AddrMode::PcDisp | AddrMode::PcIndex => 1,
            AddrMode::AbsLong => 2,
            AddrMode::Immediate => if size == Size::Long { 2 } else { 1 },
        }
    }

    /// Adjust fault address for MOVE.l -(An) destination write AE.
    /// The 68000 reports the address as An-2 (word-sized initial decrement).
    fn adjust_ae_fault_addr(&self, addr: u32, is_read: bool) -> u32 {
        if is_read {
            return addr;
        }
        let top = (self.ir >> 12) & 0xF;
        let is_move = matches!(top, 1 | 2 | 3);
        if !is_move {
            return addr;
        }
        let size = Size::from_move_bits(((self.ir >> 12) & 3) as u8)
            .unwrap_or(Size::Word);
        let dst = AddrMode::decode(((self.ir >> 6) & 7) as u8, ((self.ir >> 9) & 7) as u8);
        if size == Size::Long && matches!(dst, Some(AddrMode::AddrIndPreDec(_))) {
            addr.wrapping_add(2)
        } else {
            addr
        }
    }

    /// Continue exception processing after PC has been pushed.
    /// Called from decode_and_execute when followup_tag == 0xFE.
    pub(crate) fn exception_continue(&mut self) {
        // Push old SR
        self.data = u32::from(self.exc.old_sr);
        self.micro_ops.push(MicroOp::PushWord);

        if self.exc.is_group0 {
            // Group 0: additional frame data via staged followup
            self.followup_tag = 0xFC; // Group 0 continuation
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // Standard exception: read vector
            self.addr = self.exc.vector_addr;
            self.micro_ops.push(MicroOp::ReadLongHi);
            self.micro_ops.push(MicroOp::ReadLongLo);
            self.followup_tag = 0xFF;
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Group 0 exception continuation: push IR, fault addr, access info.
    /// Called from decode_and_execute when followup_tag == 0xFC.
    pub(crate) fn exception_group0_continue(&mut self) {
        // Push IR (opcode / pipeline state)
        self.data = u32::from(self.exc.frame_ir);
        self.micro_ops.push(MicroOp::PushWord);

        // Push fault address (long)
        self.data2 = self.exc.frame_fault_addr;
        self.followup_tag = 0xFB; // Next stage: push fault addr + access info
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Group 0 exception: push fault address and access info, then read vector.
    /// Called from decode_and_execute when followup_tag == 0xFB.
    pub(crate) fn exception_group0_finish(&mut self) {
        // Push fault address high word
        let sp_addr = self.regs.push_long();
        self.addr = sp_addr;
        self.data = self.data2; // fault_addr
        self.micro_ops.push(MicroOp::WriteLongHi);
        self.micro_ops.push(MicroOp::WriteLongLo);

        // Push access info word
        self.data2 = u32::from(self.exc.frame_access_info);
        self.followup_tag = 0xFA; // Final stage
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Group 0 exception: push access info, read vector.
    /// Called from decode_and_execute when followup_tag == 0xFA.
    pub(crate) fn exception_group0_vector(&mut self) {
        // Push access info
        self.data = self.data2;
        self.micro_ops.push(MicroOp::PushWord);

        // Read exception vector
        self.addr = self.exc.vector_addr;
        self.micro_ops.push(MicroOp::ReadLongHi);
        self.micro_ops.push(MicroOp::ReadLongLo);

        self.followup_tag = 0xFF; // Vector jump
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Finish exception processing: jump to vector address.
    /// Called from decode_and_execute when followup_tag == 0xFF.
    pub(crate) fn exception_jump_vector(&mut self) {
        self.regs.pc = self.data;
        self.in_followup = false;
        self.followup_tag = 0;

        // Fill prefetch pipeline at new PC
        self.micro_ops.push(MicroOp::FetchIRC);
        self.followup_tag = 0xFD;
        self.in_followup = true;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Fill prefetch after exception vector jump.
    /// Called from decode_and_execute when followup_tag == 0xFD.
    pub(crate) fn exception_fill_prefetch(&mut self) {
        self.ir = self.irc;
        self.instr_start_pc = self.irc_addr;
        self.in_followup = false;
        self.followup_tag = 0;
        self.processing_group0 = false;
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
    }
}
