//! Exception handling for the 68000.

use super::Cpu68000;
use super::microcode::MicroOp;
use crate::common::addressing::AddrMode;
use crate::common::alu::Size;
use crate::common::flags::{self, S};

impl Cpu68000 {
    /// Trigger an exception.
    pub(super) fn exception(&mut self, vector: u8) {
        self.pending_exception = Some(vector);
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::BeginException);
        self.cycle = 0;
        self.movem_long_phase = 0;
        self.recipe_reset();
    }

    /// Trigger an address error exception (vector 3).
    pub(super) fn address_error(&mut self, addr: u32, is_read: bool, is_instruction: bool) {
        // Clear any deferred post-increment — on address error the register
        // should not be modified (the memory access failed).
        self.deferred_postinc = None;

        // Calculate function code based on supervisor mode and access type.
        // PC-relative addressing modes use program-space FC even for data reads.
        let supervisor = self.regs.sr & flags::S != 0;
        let is_program = is_instruction || self.program_space_access;
        self.fault_fc = match (supervisor, is_program) {
            (false, false) => 1, // User data
            (false, true) => 2,  // User program
            (true, false) => 5,  // Supervisor data
            (true, true) => 6,   // Supervisor program
        };
        self.fault_addr = addr;
        self.fault_read = is_read;
        self.fault_in_instruction = is_instruction;
        self.exception(3);
    }

    /// Apply any pending deferred post-increment.
    pub(super) fn apply_deferred_postinc(&mut self) {
        if let Some((reg, inc)) = self.deferred_postinc.take() {
            let addr = self.regs.a(reg as usize);
            self.regs.set_a(reg as usize, addr.wrapping_add(inc));
        }
    }

    /// Begin exception processing.
    ///
    /// Group 0 exceptions (bus error, address error) use a 14-byte stack frame.
    /// Other exceptions use a standard 6-byte frame (PC + SR).
    #[allow(clippy::too_many_lines)]
    pub(super) fn begin_exception(&mut self) {
        let Some(vec) = self.pending_exception.take() else {
            return;
        };
        self.current_exception = Some(vec);

        // Save SR and enter supervisor mode
        let old_sr = self.regs.sr;
        self.regs.enter_supervisor();
        self.regs.sr &= !flags::T; // Clear trace

        let is_group_0 = vec == 2 || vec == 3;

        if is_group_0 {
            self.movem_long_phase = 0;

            // Internal cycles for exception processing
            self.internal_cycles = 13;
            self.micro_ops.push(MicroOp::Internal);

            let is_move = matches!((self.opcode >> 12) & 0xF, 1 | 2 | 3);
            let is_predec_dst = matches!(
                self.dst_mode,
                Some(AddrMode::AddrIndPreDec(_))
            );
            let effective_ir = self.opcode;

            // Build access info word (special status word)
            let access_info: u16 = (effective_ir & 0xFF00)
                | (effective_ir & 0x00E0)
                | (if self.fault_read { 0x10 } else { 0 })
                | (if self.fault_in_instruction { 0x08 } else { 0 })
                | u16::from(self.fault_fc & 0x07);

            // Compute PC for exception frame
            self.data = if let Some(pc) = self.exception_pc_override.take() {
                pc
            } else {
                let src_ext_words = match self.src_mode {
                    Some(mode) => self.ext_words_for_mode(mode),
                    None => 0u8,
                };

                if is_move && self.size == Size::Long && !self.fault_read {
                    // MOVE.l destination write AE
                    if self.uses_predec_mode() {
                        self.instr_start_pc
                            .wrapping_add(u32::from(src_ext_words) * 2)
                    } else {
                        let is_reg_src = matches!(
                            self.src_mode,
                            Some(AddrMode::DataReg(_)) | Some(AddrMode::AddrReg(_))
                        );
                        let dst_adj =
                            if matches!(self.dst_mode, Some(AddrMode::AbsLong)) && is_reg_src {
                                2u32
                            } else {
                                0
                            };
                        self.instr_start_pc
                            .wrapping_add(u32::from(src_ext_words) * 2 + dst_adj)
                    }
                } else if is_move && self.size == Size::Long && self.fault_read {
                    // MOVE.l source-read AE
                    let is_abs_src = matches!(
                        self.src_mode,
                        Some(AddrMode::AbsShort) | Some(AddrMode::AbsLong)
                    );
                    if is_abs_src {
                        self.instr_start_pc
                            .wrapping_add(u32::from(src_ext_words.saturating_sub(1)) * 2)
                    } else {
                        self.instr_start_pc.wrapping_sub(2)
                    }
                } else if is_move && !self.fault_read {
                    // MOVE.w/b destination write AE
                    let is_reg_src = matches!(
                        self.src_mode,
                        Some(AddrMode::DataReg(_)) | Some(AddrMode::AddrReg(_))
                    );
                    let dst_adj =
                        if matches!(self.dst_mode, Some(AddrMode::AbsLong)) && is_reg_src {
                            2u32
                        } else {
                            0
                        };
                    self.instr_start_pc
                        .wrapping_add(u32::from(src_ext_words) * 2 + dst_adj)
                } else {
                    // Non-MOVE instructions (and MOVE source-read AEs)
                    let is_movem = (self.opcode & 0xFB80) == 0x4880;
                    let is_absolute = matches!(
                        self.src_mode,
                        Some(AddrMode::AbsShort) | Some(AddrMode::AbsLong)
                    );
                    let is_immediate = matches!(self.src_mode, Some(AddrMode::Immediate));
                    let is_cmpm = (self.opcode & 0xF138) == 0xB108;
                    let is_src_predec = matches!(
                        self.src_mode,
                        Some(AddrMode::AddrIndPreDec(_))
                    );

                    if is_move && self.fault_read && is_predec_dst && !is_src_predec {
                        if is_absolute {
                            self.instr_start_pc
                                .wrapping_add(u32::from(src_ext_words.saturating_sub(1)) * 2)
                        } else {
                            self.instr_start_pc.wrapping_sub(2)
                        }
                    } else if self.uses_predec_mode() || is_cmpm || is_movem {
                        self.regs.pc
                    } else if is_immediate {
                        let dst_abs_ext = match self.dst_mode {
                            Some(AddrMode::AbsShort) => 1u8,
                            Some(AddrMode::AbsLong) => 2u8,
                            _ => 0,
                        };
                        self.instr_start_pc
                            .wrapping_sub(2)
                            .wrapping_add(u32::from(src_ext_words + dst_abs_ext) * 2)
                    } else if is_absolute {
                        self.instr_start_pc
                            .wrapping_add(u32::from(src_ext_words.saturating_sub(1)) * 2)
                    } else {
                        self.instr_start_pc.wrapping_sub(2)
                    }
                }
            };
            self.data2 = u32::from(old_sr);

            // Push PC (4 bytes)
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);

            // Push SR (2 bytes)
            self.micro_ops.push(MicroOp::SetDataFromData2);
            self.micro_ops.push(MicroOp::PushWord);

            // Push IR (2 bytes)
            self.addr2 = u32::from(effective_ir);
            self.micro_ops.push(MicroOp::PushGroup0IR);

            // For MOVE.l -(An) destination write AE, the 68000 reports the
            // fault address as An - 2 (word-sized initial decrement).
            if is_move && !self.fault_read && self.size == Size::Long && is_predec_dst {
                self.fault_addr = self.fault_addr.wrapping_add(2);
            }

            // On MOVE.l destination write AE with predecrement, the 68000
            // does not commit the address register decrement.
            if is_move && !self.fault_read && self.size == Size::Long && is_predec_dst {
                let dst_reg = ((self.opcode >> 9) & 7) as usize;
                self.regs
                    .set_a(dst_reg, self.regs.a(dst_reg).wrapping_add(4));
            }

            // Push fault address (4 bytes)
            self.micro_ops.push(MicroOp::PushGroup0FaultAddr);

            // Push access info (2 bytes)
            self.group0_access_info = access_info;
            self.micro_ops.push(MicroOp::PushGroup0AccessInfo);
        } else {
            // Standard 6-byte frame for other exceptions
            let saved_pc = match vec {
                4 | 8 | 10 | 11 => self.regs.pc.wrapping_sub(2),
                5 | 6 | 7 | 32..=47 => self.regs.pc,
                _ => self.regs.pc,
            };
            self.data = saved_pc;
            self.data2 = u32::from(old_sr);

            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            self.micro_ops.push(MicroOp::SetDataFromData2);
            self.micro_ops.push(MicroOp::PushWord);
        }

        // Queue vector read
        self.micro_ops.push(MicroOp::ReadVector);
    }

    // --- Address error triggers for specific instructions ---

    pub(super) fn trigger_rts_address_error(&mut self, addr: u32) {
        self.fault_fc = if self.regs.sr & S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    pub(super) fn trigger_branch_address_error(&mut self, addr: u32) {
        self.fault_fc = if self.regs.sr & S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    pub(super) fn trigger_rte_address_error(&mut self, addr: u32) {
        self.fault_fc = if self.regs.sr & S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    pub(super) fn trigger_rtr_address_error(&mut self, addr: u32) {
        self.fault_fc = if self.regs.sr & S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    pub(super) fn trigger_jmp_address_error(&mut self, target: u32) {
        self.fault_addr = target;
        self.fault_fc = if self.regs.is_supervisor() { 6 } else { 2 };
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    pub(super) fn trigger_jsr_address_error(&mut self, target: u32) {
        self.fault_addr = target;
        self.fault_fc = if self.regs.is_supervisor() { 6 } else { 2 };
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }
}
