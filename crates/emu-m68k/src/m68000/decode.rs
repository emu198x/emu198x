//! Instruction decode for the 68000.
//!
//! Decodes opcodes into recipe sequences. Each decoder reads opcode fields,
//! sets up size/src_mode/dst_mode, and builds a recipe via recipe_begin/push/commit.
//! Unrecognised opcodes trigger an illegal instruction exception.
//!
//! This file is being rebuilt incrementally per docs/decode-rewrite-plan.md.
//! Each instruction group is added, tested against single-step tests, and
//! verified at 100% before the next group is added.

#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]

use super::Cpu68000;
use super::recipe::{EaSide, RecipeAlu, RecipeOp};
use crate::common::addressing::AddrMode;
use crate::common::alu::Size;

impl Cpu68000 {
    /// Decode the current opcode and build a recipe for execution.
    pub(super) fn decode_and_execute(&mut self) {
        let op = self.opcode;

        match op >> 12 {
            // Phase 1: Data movement
            0x1 => self.decode_move(op, Size::Byte),
            0x2 => self.decode_move(op, Size::Long),
            0x3 => self.decode_move(op, Size::Word),
            0x4 => self.decode_group_4(op),
            0x7 => self.decode_moveq(op),
            // Everything else: illegal (phases 2+ will fill in)
            _ => self.illegal_instruction(),
        }
    }

    /// Trigger an illegal instruction exception (vector 4).
    pub(super) fn illegal_instruction(&mut self) {
        self.exception(4);
    }

    // ======================================================================
    // Helpers — proven correct, reused across phases
    // ======================================================================

    /// Decode a 6-bit EA field (mode 3 bits + reg 3 bits).
    fn decode_ea(mode: u8, reg: u8) -> Option<AddrMode> {
        AddrMode::decode(mode & 7, reg & 7)
    }

    /// Count extension words needed for an EA and current size.
    fn ext_count_for(&self, mode: AddrMode) -> u8 {
        self.ext_words_for_mode(mode)
    }

    /// Internal cycles for EA calculation (index modes need extra time).
    fn ea_internal(mode: AddrMode) -> u8 {
        match mode {
            AddrMode::AddrIndIndex(_) | AddrMode::PcIndex => 4,
            _ => 0,
        }
    }

    /// Internal cycles for a register-to-register ALU op (after FetchOpcode).
    #[allow(dead_code)]
    fn alu_reg_internal(size: Size) -> u8 {
        match size {
            Size::Long => 4,
            _ => 0,
        }
    }

    /// Build a recipe for: read source EA → ALU op → write to Dn.
    #[allow(dead_code)]
    fn build_alu_to_reg(&mut self, op: RecipeAlu, reg: u8, src: AddrMode) {
        let ext = self.ext_count_for(src);
        let ea_int = Self::ea_internal(src);
        let reg_int = Self::alu_reg_internal(self.size);
        let is_mem = !matches!(src, AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate);
        let int_cycles = ea_int + if !is_mem { reg_int } else { 0 };

        self.recipe_begin();
        if ext > 0 { self.recipe_push(RecipeOp::FetchExtWords(ext)); }
        self.recipe_push(RecipeOp::CalcEa(EaSide::Src));
        self.recipe_push(RecipeOp::ReadEa(EaSide::Src));
        self.recipe_push(RecipeOp::AluReg { op, reg });
        if int_cycles > 0 { self.recipe_push(RecipeOp::Internal(int_cycles)); }
        self.recipe_commit();
    }

    /// Build a recipe for: read source EA → ALU op → write back to EA.
    #[allow(dead_code)]
    fn build_alu_to_mem(&mut self, op: RecipeAlu, reg: u8, dst: AddrMode) {
        let ext = self.ext_count_for(dst);
        let ea_int = Self::ea_internal(dst);

        self.recipe_begin();
        if ext > 0 { self.recipe_push(RecipeOp::FetchExtWords(ext)); }
        self.data = self.read_data_reg(reg, self.size);
        self.recipe_push(RecipeOp::CalcEa(EaSide::Dst));
        self.recipe_push(RecipeOp::ReadEa(EaSide::Dst));
        self.recipe_push(RecipeOp::AluMem { op });
        self.recipe_push(RecipeOp::WriteEa(EaSide::Dst));
        if ea_int > 0 { self.recipe_push(RecipeOp::Internal(ea_int)); }
        self.recipe_commit();
    }

    // ======================================================================
    // Phase 1: Groups 1/2/3 — MOVE.b/w/l, MOVEA.w/l
    // ======================================================================

    fn decode_move(&mut self, op: u16, size: Size) {
        self.size = size;

        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;

        let Some(src) = Self::decode_ea(src_mode, src_reg) else {
            self.illegal_instruction();
            return;
        };
        let Some(dst) = Self::decode_ea(dst_mode, dst_reg) else {
            self.illegal_instruction();
            return;
        };

        // MOVEA: destination is address register
        let is_movea = matches!(dst, AddrMode::AddrReg(_));
        if is_movea && size == Size::Byte {
            self.illegal_instruction();
            return;
        }

        self.src_mode = Some(src);
        self.dst_mode = Some(dst);

        let src_ext = self.ext_count_for(src);
        let dst_ext = self.ext_count_for(dst);
        let total_ext = src_ext + dst_ext;
        let src_int = Self::ea_internal(src);
        let dst_int = Self::ea_internal(dst);

        self.recipe_begin();
        if total_ext > 0 { self.recipe_push(RecipeOp::FetchExtWords(total_ext)); }
        if src_int > 0 { self.recipe_push(RecipeOp::Internal(src_int)); }
        self.recipe_push(RecipeOp::CalcEa(EaSide::Src));
        self.recipe_push(RecipeOp::ReadEa(EaSide::Src));
        if !is_movea {
            self.recipe_push(RecipeOp::SetFlagsMove);
        }
        // No SkipExt needed: CalcEa/ReadEa for Src already consume src ext
        // words via next_ext_word(), advancing ext_idx to the dst position.
        if dst_int > 0 { self.recipe_push(RecipeOp::Internal(dst_int)); }
        self.recipe_push(RecipeOp::CalcEa(EaSide::Dst));
        self.recipe_push(RecipeOp::WriteEa(EaSide::Dst));
        self.recipe_commit();
    }

    // ======================================================================
    // Phase 1: Group 7 — MOVEQ
    // ======================================================================

    fn decode_moveq(&mut self, op: u16) {
        if op & 0x0100 != 0 {
            self.illegal_instruction();
            return;
        }
        let reg = ((op >> 9) & 7) as u8;
        let data = op as i8 as i32 as u32;
        self.regs.d[reg as usize] = data;
        self.size = Size::Long;
        self.set_flags_move(data, Size::Long);

        self.recipe_begin();
        self.recipe_push(RecipeOp::Internal(0));
        self.recipe_commit();
    }

    // ======================================================================
    // Phase 1: Group 4 — LEA only (rest illegal until later phases)
    // ======================================================================

    fn decode_group_4(&mut self, op: u16) {
        let mode = ((op >> 3) & 7) as u8;
        let reg = (op & 7) as u8;

        // LEA: 0100_rrr_111_mmmsss (opmode = 7)
        if op & 0xF1C0 == 0x41C0 {
            let an = ((op >> 9) & 7) as u8;
            let Some(ea) = Self::decode_ea(mode, reg) else {
                self.illegal_instruction();
                return;
            };
            self.size = Size::Long;
            self.src_mode = Some(ea);
            self.dst_mode = Some(AddrMode::AddrReg(an));
            let ext = self.ext_count_for(ea);
            let ea_int = Self::ea_internal(ea);

            self.recipe_begin();
            if ext > 0 { self.recipe_push(RecipeOp::FetchExtWords(ext)); }
            if ea_int > 0 { self.recipe_push(RecipeOp::Internal(ea_int)); }
            self.recipe_push(RecipeOp::CalcEa(EaSide::Src));
            self.recipe_push(RecipeOp::LoadEaAddr(EaSide::Src));
            self.recipe_push(RecipeOp::WriteEa(EaSide::Dst));
            self.recipe_commit();
            return;
        }

        self.illegal_instruction();
    }
}
