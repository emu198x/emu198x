//! Instruction decode and execution for the 68000.
//!
//! The 68000 instruction set is decoded primarily by the top 4 bits of the
//! opcode word, with further decoding based on other bit fields.

#![allow(clippy::too_many_lines)] // Decode functions are intentionally verbose.
#![allow(clippy::single_match_else)] // Match arms will expand with more cases.
#![allow(clippy::if_not_else)] // Privilege check reads more naturally this way.
#![allow(clippy::verbose_bit_mask)] // Explicit bit operations for clarity.
#![allow(clippy::doc_markdown)] // Instruction mnemonics don't need backticks.
#![allow(clippy::manual_swap)] // Will use swap when register access is simpler.
#![allow(clippy::manual_rotate)] // SWAP is conceptually a register swap, not rotation.
#![allow(clippy::manual_range_patterns)] // Explicit values for instruction decode clarity.

use crate::cpu::{
    AddrMode, EaSide, InstrPhase, M68000, RecipeAlu, RecipeOp, RecipeUnary, Size,
};
use crate::flags::{Status, C, N, V, X, Z};
use crate::microcode::MicroOp;
use std::sync::{Mutex, OnceLock};

fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("EMU68000_TRACE")
            .map(|v| v != "0")
            .unwrap_or(false)
    })
}

fn trace_jump_targets() -> Option<&'static Vec<u32>> {
    static TARGETS: OnceLock<Option<Vec<u32>>> = OnceLock::new();
    TARGETS
        .get_or_init(|| {
            let Ok(spec) = std::env::var("EMU68000_TRACE_JUMP_TO") else {
                return None;
            };
            let mut targets = Vec::new();
            for part in spec.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                let hex = part.trim_start_matches("0x").trim_start_matches("0X");
                let Ok(value) = u32::from_str_radix(hex, 16) else {
                    return None;
                };
                targets.push(value);
            }
            if targets.is_empty() { None } else { Some(targets) }
        })
        .as_ref()
}

fn trace_add_pc_target() -> Option<u32> {
    static TARGET: OnceLock<Option<u32>> = OnceLock::new();
    *TARGET.get_or_init(|| {
        let Ok(spec) = std::env::var("EMU68000_TRACE_ADD_PC") else {
            return None;
        };
        let hex = spec.trim().trim_start_matches("0x").trim_start_matches("0X");
        u32::from_str_radix(hex, 16).ok()
    })
}

fn recipe_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("EMU68000_RECIPE")
            .map(|v| v != "0")
            .unwrap_or(false)
    })
}

fn trace_legacy_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("EMU68000_TRACE_LEGACY").is_ok())
}

fn recipe_accepts_table() -> Option<&'static Vec<bool>> {
    static TABLE: OnceLock<Option<Vec<bool>>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            if !trace_legacy_enabled() {
                return None;
            }
            let mut cpu = M68000::new();
            let mut table = vec![false; 0x1_0000];
            for op in 0u32..=0xFFFF {
                if cpu.recipe_accepts_opcode(op as u16) {
                    table[op as usize] = true;
                }
            }
            Some(table)
        })
        .as_ref()
}

fn legacy_seen_table() -> &'static Mutex<Vec<bool>> {
    static SEEN: OnceLock<Mutex<Vec<bool>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(vec![false; 0x1_0000]))
}

impl M68000 {
    fn trace_sr_update(&self, kind: &str, new_sr: u16) {
        if std::env::var("EMU68000_TRACE_SR").is_err() {
            return;
        }
        let op_pc = self.instr_start_pc.wrapping_sub(2);
        eprintln!(
            "[CPU] SR {kind} pc=${op_pc:08X} op=${:04X} old=${:04X} new=${:04X}",
            self.opcode,
            self.regs.sr,
            new_sr
        );
    }

    fn trace_jump_to(&self, kind: &str, target: u32) {
        let Some(targets) = trace_jump_targets() else {
            return;
        };
        if !targets.contains(&target) {
            return;
        }
        let op_pc = self.instr_start_pc.wrapping_sub(2);
        eprintln!(
            "[CPU] {kind} target=${target:08X} pc=${:08X} op_pc=${op_pc:08X}",
            self.regs.pc
        );
    }

    fn trace_legacy_opcode(&self, op: u16) {
        if !trace_legacy_enabled() {
            return;
        }
        let Some(table) = recipe_accepts_table() else {
            return;
        };
        if table[op as usize] {
            return;
        }

        let mut seen = legacy_seen_table().lock().expect("legacy trace lock");
        if seen[op as usize] {
            return;
        }
        seen[op as usize] = true;

        let op_pc = self.instr_start_pc.wrapping_sub(2);
        eprintln!(
            "[CPU] LEGACY op=${op:04X} pc=${op_pc:08X}"
        );
    }

    /// Decode and execute the current instruction.
    pub(super) fn decode_and_execute(&mut self) {
        use crate::cpu::InstrPhase;

        let queue_before = self.micro_ops.len();
        let pos_before = self.micro_ops.pos();

        // If we're in a follow-up phase, continue the current instruction
        if self.instr_phase != InstrPhase::Initial {
            self.continue_instruction();
            let queue_after = self.micro_ops.len();
            if trace_enabled() {
                eprintln!("[CPU] CONT op=${:04X} phase={:?} queue {}/{}→{}/{}",
                    self.opcode, self.instr_phase, pos_before, queue_before, self.micro_ops.pos(), queue_after);
            }
            return;
        }

        // Extract common fields from opcode
        let op = self.opcode;

        self.trace_legacy_opcode(op);

        if recipe_enabled() && self.try_recipe(op) {
            return;
        }

        // Top 4 bits determine instruction group
        match op >> 12 {
            0x0 => self.decode_group_0(op),
            0x1 => self.decode_move_byte(op),
            0x2 => self.decode_move_long(op),
            0x3 => self.decode_move_word(op),
            0x4 => self.decode_group_4(op),
            0x5 => self.decode_group_5(op),
            0x6 => self.decode_group_6(op),
            0x7 => self.decode_moveq(op),
            0x8 => self.decode_group_8(op),
            0x9 => self.decode_sub(op),
            0xA => self.decode_line_a(op),
            0xB => self.decode_group_b(op),
            0xC => self.decode_group_c(op),
            0xD => self.decode_add(op),
            0xE => self.decode_shift_rotate(op),
            0xF => self.decode_line_f(op),
            _ => unreachable!(),
        }
        if trace_enabled() {
            let queue_after = self.micro_ops.len();
            eprintln!("[CPU] EXEC op=${:04X} PC=${:08X} queue {}/{}→{}/{}",
                op, self.regs.pc, pos_before, queue_before, self.micro_ops.pos(), queue_after);
        }
    }

    /// Continue executing an instruction that's in a follow-up phase.
    fn continue_instruction(&mut self) {
        let op = self.opcode;
        if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
            let op_pc = self.instr_start_pc.wrapping_sub(2);
            let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
            if log_this {
                eprintln!(
                    "[CPU] continue op=${op:04X} op_pc=${op_pc:08X} phase={:?}",
                    self.instr_phase
                );
            }
        }

        // Re-dispatch to the same instruction handler
        match op >> 12 {
            0x0 => {
                // Group 0 - check for MOVEP (bit 8 set + mode 1) vs immediate ops
                let mode = ((op >> 3) & 7) as u8;
                if op & 0x0100 != 0 && mode == 1 {
                    // MOVEP continuation
                    self.movep_continuation();
                } else {
                    // Immediate operations continuation
                    self.immediate_op_continuation();
                }
            }
            0x1 => self.decode_move_byte(op),
            0x2 => self.decode_move_long(op),
            0x3 => self.decode_move_word(op),
            0x4 => {
                // Group 4 continuations
                // PEA: 0x4840-0x487F (bits 11-8 = 8, bits 7-6 = 01)
                // MOVEM to memory: 0x4880-0x48FF (bits 11-8 = 8, bit 7 = 1)
                // MOVEM from memory: 0x4C80-0x4CFF (bits 11-8 = C, bit 7 = 1)
                // LEA: 0x41C0-0x4FFF with bit 8 set and bits 7-6 = 11
                // MOVE to CCR: 0x44C0-0x44FF (bits 11-8 = 4, bits 7-6 = 11)
                // MOVE to SR: 0x46C0-0x46FF (bits 11-8 = 6, bits 7-6 = 11)
                // RTS: 0x4E75 (bits 11-8 = E, bits 7-6 = 01)
                // JSR: 0x4E80-0x4EBF (bits 11-8 = E, bits 7-6 = 10)
                // JMP: 0x4EC0-0x4EFF (bits 11-8 = E, bits 7-6 = 11)
                let subfield = (op >> 8) & 0xF;
                let bits_7_6 = (op >> 6) & 3;
                if subfield == 0x4 && bits_7_6 == 3 {
                    // MOVE to CCR continuation
                    self.exec_move_to_ccr_continuation();
                } else if subfield == 0x6 && bits_7_6 == 3 {
                    // MOVE to SR continuation
                    self.exec_move_to_sr_continuation();
                } else if subfield == 0x8 && bits_7_6 == 1 {
                    // PEA continuation (0x4840-0x487F)
                    self.exec_pea_continuation();
                } else if subfield == 0x8 && op & 0x0080 != 0 {
                    // MOVEM to memory continuation
                    self.exec_movem_to_mem_continuation();
                } else if subfield == 0xC && op & 0x0080 != 0 {
                    // MOVEM from memory continuation
                    self.exec_movem_from_mem_continuation();
                } else if subfield == 0xE && bits_7_6 == 1 && op & 0x3F == 0x33 {
                    // RTE continuation (0x4E73)
                    self.exec_rte_continuation();
                } else if subfield == 0xE && bits_7_6 == 1 && op & 0x3F == 0x35 {
                    // RTS continuation (0x4E75)
                    self.exec_rts_continuation();
                } else if subfield == 0xE && bits_7_6 == 1 && op & 0x3F == 0x37 {
                    // RTR continuation (0x4E77)
                    self.exec_rtr_continuation();
                } else if subfield == 0xE && (op >> 4) & 0xF == 5 && op & 8 == 0 {
                    // LINK continuation (0x4E50-0x4E57)
                    self.exec_link((op & 7) as u8);
                } else if subfield == 0xE && (op >> 4) & 0xF == 5 && op & 8 != 0 {
                    // UNLK continuation (0x4E58-0x4E5F)
                    self.exec_unlk_continuation();
                } else if subfield == 0xE && bits_7_6 == 2 {
                    // JSR continuation
                    self.exec_jsr_continuation();
                } else if subfield == 0xE && bits_7_6 == 3 {
                    // JMP continuation
                    self.exec_jmp_continuation();
                } else if op & 0x01C0 == 0x01C0 {
                    // LEA continuation
                    self.exec_lea_continuation();
                } else {
                    self.instr_phase = crate::cpu::InstrPhase::Initial;
                }
            }
            0x5 => {
                // Group 5 continuations: DBcc uses its own continuation,
                // Scc/ADDQ/SUBQ re-dispatch through decode_group_5.
                let mode = ((op >> 3) & 7) as u8;
                if mode == 1 {
                    // DBcc continuation
                    self.dbcc_continuation();
                } else {
                    self.decode_group_5(op);
                }
            }
            0x6 => {
                // Branch instructions with word displacement
                self.branch_continuation();
            }
            0x7 => self.decode_moveq(op),
            0x8 => self.decode_group_8(op),
            0x9 => self.decode_sub(op),
            0xA => self.decode_line_a(op),
            0xB => self.decode_group_b(op),
            0xC => self.decode_group_c(op),
            0xD => self.decode_add(op),
            0xE => self.decode_shift_rotate(op),
            0xF => self.decode_line_f(op),
            _ => {
                // Instruction doesn't support phases, reset
                self.instr_phase = crate::cpu::InstrPhase::Initial;
            }
        }
    }

    fn try_recipe(&mut self, op: u16) -> bool {
        match op >> 12 {
            0x0 => self.recipe_group_0(op),
            0x1 => self.recipe_move(Size::Byte, op),
            0x2 => self.recipe_move(Size::Long, op),
            0x3 => self.recipe_move(Size::Word, op),
            0x5 => self.recipe_group_5(op),
            0x6 => self.recipe_branch(op),
            0x7 => self.recipe_moveq(op),
            0x4 => self.recipe_group_4(op),
            0x8 => self.recipe_group_8(op),
            0x9 => {
                if (op >> 6) & 3 == 3 {
                    self.recipe_suba(op)
                } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
                    self.recipe_subx(op)
                } else {
                    self.recipe_sub(op)
                }
            }
            0xB => self.recipe_group_b(op),
            0xC => self.recipe_group_c(op),
            0xD => {
                if (op >> 6) & 3 == 3 {
                    self.recipe_adda(op)
                } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
                    self.recipe_addx(op)
                } else {
                    self.recipe_add(op)
                }
            }
            0xE => self.recipe_shift_rotate(op),
            _ => false,
        }
    }

    /// Check whether the recipe decoder accepts an opcode (for coverage tooling).
    pub fn recipe_accepts_opcode(&mut self, op: u16) -> bool {
        self.opcode = op;
        self.instr_start_pc = 0;
        self.ext_count = 0;
        self.ext_idx = 0;
        self.src_mode = None;
        self.dst_mode = None;
        self.micro_ops.clear();
        self.recipe_reset();
        self.try_recipe(op)
    }

    fn recipe_moveq(&mut self, op: u16) -> bool {
        let reg = ((op >> 9) & 7) as u8;
        let imm = (op & 0xFF) as i8 as i32 as u32;

        self.size = Size::Long;
        self.src_mode = None;
        self.dst_mode = Some(AddrMode::DataReg(reg));
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;
        self.recipe_imm = imm;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::LoadImm) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::WriteEa(EaSide::Dst)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::SetFlagsMove) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_branch(&mut self, op: u16) -> bool {
        let condition = ((op >> 8) & 0xF) as u8;
        let disp8 = (op & 0xFF) as i8;

        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if disp8 == 0 {
            if !self.recipe_push(RecipeOp::FetchExtWords(1)) {
                self.recipe_reset();
                return false;
            }
        }
        if !self.recipe_push(RecipeOp::Branch { condition, disp8 }) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_group_5(&mut self, op: u16) -> bool {
        let data = ((op >> 9) & 7) as u8;
        let data = if data == 0 { 8 } else { data };

        if (op >> 6) & 3 == 3 {
            // Scc, DBcc
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;
            let condition = ((op >> 8) & 0xF) as u8;

            if mode == 1 {
                return self.recipe_dbcc(condition, ea_reg);
            }
            return self.recipe_scc(condition, mode, ea_reg);
        }

        // ADDQ, SUBQ
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let add = op & 0x0100 == 0;
        self.recipe_addq_subq(add, u32::from(data), size, mode, ea_reg)
    }

    fn recipe_addq_subq(
        &mut self,
        add: bool,
        imm: u32,
        size: Size,
        mode: u8,
        ea_reg: u8,
    ) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::Immediate) {
            return false;
        }
        if !addr_mode.is_data_alterable() && !matches!(addr_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        let is_addr = matches!(addr_mode, AddrMode::AddrReg(_));
        let op = if add { RecipeAlu::Add } else { RecipeAlu::Sub };

        self.size = if is_addr { Size::Long } else { size };
        self.src_mode = None;
        self.dst_mode = Some(addr_mode);
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;
        self.recipe_imm = imm;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::LoadImm) {
            self.recipe_reset();
            return false;
        }

        let ok = match addr_mode {
            AddrMode::AddrReg(r) => self.recipe_push(RecipeOp::AddrArith { reg: r, add }),
            AddrMode::DataReg(r) => self.recipe_push(RecipeOp::AluReg { op, reg: r }),
            _ => {
                self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
                    && self.recipe_push(RecipeOp::ReadEa(EaSide::Dst))
                    && self.recipe_push(RecipeOp::AluMem { op })
                    && self.recipe_push(RecipeOp::WriteEa(EaSide::Dst))
            }
        };

        if !ok {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_scc(&mut self, condition: u8, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if !addr_mode.is_data_alterable() {
            return false;
        }

        self.size = Size::Byte;
        self.src_mode = None;
        self.dst_mode = Some(addr_mode);
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }

        if !matches!(addr_mode, AddrMode::DataReg(_))
            && !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
        {
            self.recipe_reset();
            return false;
        }

        if !self.recipe_push(RecipeOp::Scc { condition }) {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_dbcc(&mut self, condition: u8, reg: u8) -> bool {
        if self.ext_words.is_empty() {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::FetchExtWords(1))
            || !self.recipe_push(RecipeOp::Dbcc { condition, reg })
        {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_group_0(&mut self, op: u16) -> bool {
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if op & 0x0100 != 0 {
            if mode == 1 {
                return false; // MOVEP
            }
            return self.recipe_bit_reg(op, mode, ea_reg);
        }

        let is_ccr_sr = mode == 7 && ea_reg == 4;
        match (op >> 9) & 7 {
            4 => self.recipe_bit_imm(op, mode, ea_reg),
            0 => {
                if is_ccr_sr {
                    self.recipe_imm_to_ccr_sr(op, RecipeAlu::Or)
                } else {
                    self.recipe_imm_op(op, RecipeAlu::Or, mode, ea_reg)
                }
            }
            1 => {
                if is_ccr_sr {
                    self.recipe_imm_to_ccr_sr(op, RecipeAlu::And)
                } else {
                    self.recipe_imm_op(op, RecipeAlu::And, mode, ea_reg)
                }
            }
            2 => self.recipe_imm_op(op, RecipeAlu::Sub, mode, ea_reg),
            3 => self.recipe_imm_op(op, RecipeAlu::Add, mode, ea_reg),
            5 => {
                if is_ccr_sr {
                    self.recipe_imm_to_ccr_sr(op, RecipeAlu::Eor)
                } else {
                    self.recipe_imm_op(op, RecipeAlu::Eor, mode, ea_reg)
                }
            }
            6 => self.recipe_cmpi(op, mode, ea_reg),
            _ => false,
        }
    }

    fn recipe_imm_to_ccr_sr(&mut self, op: u16, kind: RecipeAlu) -> bool {
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };
        if size == Size::Long {
            return false;
        }

        self.size = size;
        self.src_mode = Some(AddrMode::Immediate);
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::FetchExtWords(1))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
        {
            self.recipe_reset();
            return false;
        }

        let ok = if size == Size::Byte {
            self.recipe_push(RecipeOp::LogicCcr { op: kind })
        } else {
            self.recipe_push(RecipeOp::LogicSr { op: kind })
        };
        if !ok {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_bit_reg(&mut self, op: u16, mode: u8, ea_reg: u8) -> bool {
        let reg = ((op >> 9) & 7) as u8;
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        let op_kind = ((op >> 6) & 3) as u8; // 0=BTST,1=BCHG,2=BCLR,3=BSET
        let is_reg = matches!(addr_mode, AddrMode::DataReg(_));

        self.size = if is_reg { Size::Long } else { Size::Byte };
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;

        let ext_needed = self.ext_words_for_mode(addr_mode);
        if (ext_needed as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_needed > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_needed)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::ReadBitReg {
            reg,
            mem: !is_reg,
        }) {
            self.recipe_reset();
            return false;
        }

        if is_reg {
            if !self.recipe_push(RecipeOp::BitReg {
                reg: ea_reg,
                op: op_kind,
            }) {
                self.recipe_reset();
                return false;
            }
        } else {
            if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
                || !self.recipe_push(RecipeOp::BitMem { op: op_kind })
            {
                self.recipe_reset();
                return false;
            }
        }

        self.recipe_commit()
    }

    fn recipe_imm_op(
        &mut self,
        op: u16,
        kind: RecipeAlu,
        mode: u8,
        ea_reg: u8,
    ) -> bool {
        if mode == 7 && ea_reg == 4 {
            return false; // CCR/SR special cases handled by legacy path
        }
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };

        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_) | AddrMode::Immediate) {
            return false;
        }
        if !addr_mode.is_data_alterable() && !matches!(addr_mode, AddrMode::DataReg(_)) {
            return false;
        }

        self.size = size;
        self.src_mode = Some(AddrMode::Immediate);
        self.dst_mode = Some(addr_mode);

        let src_ext = self.ext_words_for_mode(AddrMode::Immediate);
        let dst_ext = self.ext_words_for_mode(addr_mode);
        if (src_ext as usize + dst_ext as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc.wrapping_add(u32::from(src_ext) * 2);

        let ext_total = src_ext + dst_ext;
        if ext_total > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_total)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::ReadEa(EaSide::Src)) {
            self.recipe_reset();
            return false;
        }

        if let AddrMode::DataReg(r) = addr_mode {
            if !self.recipe_push(RecipeOp::AluReg { op: kind, reg: r }) {
                self.recipe_reset();
                return false;
            }
        } else {
            if !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
                || !self.recipe_push(RecipeOp::ReadEa(EaSide::Dst))
                || !self.recipe_push(RecipeOp::AluMem { op: kind })
                || !self.recipe_push(RecipeOp::WriteEa(EaSide::Dst))
            {
                self.recipe_reset();
                return false;
            }
        }

        self.recipe_commit()
    }

    fn recipe_cmpi(&mut self, op: u16, mode: u8, ea_reg: u8) -> bool {
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };

        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_) | AddrMode::Immediate) {
            return false;
        }

        self.size = size;
        self.src_mode = Some(AddrMode::Immediate);
        self.dst_mode = Some(addr_mode);

        let src_ext = self.ext_words_for_mode(AddrMode::Immediate);
        let dst_ext = self.ext_words_for_mode(addr_mode);
        if (src_ext as usize + dst_ext as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc.wrapping_add(u32::from(src_ext) * 2);

        let ext_total = src_ext + dst_ext;
        if ext_total > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_total)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Dst))
            || !self.recipe_push(RecipeOp::CmpEa)
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_bit_imm(&mut self, op: u16, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        let op_kind = ((op >> 6) & 3) as u8; // 0=BTST,1=BCHG,2=BCLR,3=BSET
        let is_reg = matches!(addr_mode, AddrMode::DataReg(_));

        self.size = if is_reg { Size::Long } else { Size::Byte };
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;

        let ext_needed = 1 + self.ext_words_for_mode(addr_mode);
        if (ext_needed as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc.wrapping_add(2);
        self.recipe_dst_pc_at_ext = self.instr_start_pc.wrapping_add(2);

        if !self.recipe_push(RecipeOp::FetchExtWords(ext_needed))
            || !self.recipe_push(RecipeOp::ReadBitImm { reg: is_reg })
        {
            self.recipe_reset();
            return false;
        }

        if is_reg {
            if !self.recipe_push(RecipeOp::BitReg {
                reg: ea_reg,
                op: op_kind,
            }) {
                self.recipe_reset();
                return false;
            }
        } else {
            if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
                || !self.recipe_push(RecipeOp::BitMem { op: op_kind })
            {
                self.recipe_reset();
                return false;
            }
        }

        self.recipe_commit()
    }

    fn recipe_move(&mut self, size: Size, op: u16) -> bool {
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;

        let Some(src_mode) = AddrMode::decode(src_mode, src_reg) else {
            return false;
        };
        let Some(dst_mode) = AddrMode::decode(dst_mode, dst_reg) else {
            return false;
        };

        // MOVE.B to address register is illegal.
        if matches!(dst_mode, AddrMode::AddrReg(_)) && size == Size::Byte {
            return false;
        }
        if !dst_mode.is_data_alterable() && !matches!(dst_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        self.size = size;
        self.src_mode = Some(src_mode);
        self.dst_mode = Some(dst_mode);

        self.recipe_begin();

        let src_ext = self.ext_words_for_mode(src_mode);
        let dst_ext = self.ext_words_for_mode(dst_mode);
        if (src_ext as usize + dst_ext as usize) > self.ext_words.len() {
            self.recipe_reset();
            return false;
        }
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc.wrapping_add(u32::from(src_ext) * 2);

        let ext_total = src_ext + dst_ext;
        if ext_total > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_total)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::ReadEa(EaSide::Src)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::WriteEa(EaSide::Dst)) {
            self.recipe_reset();
            return false;
        }
        if !matches!(dst_mode, AddrMode::AddrReg(_)) {
            if !self.recipe_push(RecipeOp::SetFlagsMove) {
                self.recipe_reset();
                return false;
            }
        }

        self.recipe_commit()
    }

    fn recipe_group_4(&mut self, op: u16) -> bool {
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if op & 0x0100 != 0 {
            // CHK / LEA
            if (op >> 6) & 7 == 7 {
                let reg = ((op >> 9) & 7) as u8;
                return self.recipe_lea(reg, mode, ea_reg);
            }
            return false;
        }

        match (op >> 8) & 0xF {
            0x0 => {
                if (op >> 6) & 3 == 3 {
                    return false; // MOVE from SR
                }
                let size = Size::from_bits(((op >> 6) & 3) as u8);
                return self.recipe_unary(RecipeUnary::Negx, size, mode, ea_reg);
            }
            0x2 => {
                let size = Size::from_bits(((op >> 6) & 3) as u8);
                return self.recipe_clr(size, mode, ea_reg);
            }
            0x4 => {
                if (op >> 6) & 3 == 3 {
                    return self.recipe_move_to_ccr(mode, ea_reg);
                }
                let size = Size::from_bits(((op >> 6) & 3) as u8);
                return self.recipe_unary(RecipeUnary::Neg, size, mode, ea_reg);
            }
            0x6 => {
                if (op >> 6) & 3 == 3 {
                    return self.recipe_move_to_sr(mode, ea_reg);
                }
                let size = Size::from_bits(((op >> 6) & 3) as u8);
                return self.recipe_unary(RecipeUnary::Not, size, mode, ea_reg);
            }
            0x8 => {
                // NBCD, SWAP, PEA, EXT, MOVEM to mem
                match (op >> 6) & 3 {
                    1 => {
                        if mode == 0 {
                            return self.recipe_swap(ea_reg);
                        }
                        return self.recipe_pea(mode, ea_reg);
                    }
                    2 | 3 => {
                        if mode == 0 {
                            let size = if (op >> 6) & 1 == 0 {
                                Size::Word
                            } else {
                                Size::Long
                            };
                            return self.recipe_ext(size, ea_reg);
                        }
                        self.recipe_movem_to_mem(op, mode, ea_reg)
                    }
                    _ => false,
                }
            }
            0xA => {
                if (op >> 6) & 3 == 3 {
                    return false; // TAS/ILLEGAL via legacy
                }
                let size = Size::from_bits(((op >> 6) & 3) as u8);
                return self.recipe_tst(size, mode, ea_reg);
            }
            0xC => self.recipe_movem_from_mem(op, mode, ea_reg),
            0xE => {
                let subop = (op >> 6) & 3;
                if subop == 2 {
                    return self.recipe_jsr(mode, ea_reg);
                }
                if subop == 3 {
                    return self.recipe_jmp(mode, ea_reg);
                }

                match (op >> 4) & 0xF {
                    0x4 => self.recipe_trap(op),
                    0x5 => {
                        if op & 8 != 0 {
                            self.recipe_unlk((op & 7) as u8)
                        } else {
                            self.recipe_link((op & 7) as u8)
                        }
                    }
                    0x6 => self.recipe_move_usp(op),
                    0x7 => match op & 0xF {
                        0 => self.recipe_reset_inst(),
                        1 => self.recipe_nop(),
                        2 => self.recipe_stop(),
                        3 => self.recipe_rte(),
                        5 => self.recipe_rts(),
                        6 => self.recipe_trapv(),
                        7 => self.recipe_rtr(),
                        _ => false,
                    },
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn recipe_jsr(&mut self, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(
            addr_mode,
            AddrMode::DataReg(_)
                | AddrMode::AddrReg(_)
                | AddrMode::Immediate
                | AddrMode::AddrIndPostInc(_)
                | AddrMode::AddrIndPreDec(_)
        ) {
            return false;
        }

        self.size = Size::Long;
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::Jsr)
        {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_jmp(&mut self, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(
            addr_mode,
            AddrMode::DataReg(_)
                | AddrMode::AddrReg(_)
                | AddrMode::Immediate
                | AddrMode::AddrIndPostInc(_)
                | AddrMode::AddrIndPreDec(_)
        ) {
            return false;
        }

        self.size = Size::Long;
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::Jmp)
        {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_link(&mut self, reg: u8) -> bool {
        if self.ext_words.is_empty() {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::FetchExtWords(1))
            || !self.recipe_push(RecipeOp::LinkStart { reg })
            || !self.recipe_push(RecipeOp::LinkFinish)
        {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_unlk(&mut self, reg: u8) -> bool {
        self.size = Size::Long;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::UnlkStart { reg })
            || !self.recipe_push(RecipeOp::UnlkFinish)
        {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_move_usp(&mut self, op: u16) -> bool {
        let reg = (op & 7) as u8;
        let to_usp = op & 0x0008 == 0;

        self.size = Size::Long;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::MoveUsp { reg, to_usp }) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_rts(&mut self) -> bool {
        self.size = Size::Long;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::RtsPop) || !self.recipe_push(RecipeOp::RtsFinish) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_rte(&mut self) -> bool {
        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::RtePopSr)
            || !self.recipe_push(RecipeOp::RtePopPc)
            || !self.recipe_push(RecipeOp::RteFinish)
        {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_rtr(&mut self) -> bool {
        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::RtrPopCcr)
            || !self.recipe_push(RecipeOp::RtrPopPc)
            || !self.recipe_push(RecipeOp::RtrFinish)
        {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_trap(&mut self, op: u16) -> bool {
        let vector = 32 + (op & 0xF) as u8;

        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::Trap { vector }) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_trapv(&mut self) -> bool {
        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::Trapv) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_reset_inst(&mut self) -> bool {
        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::Reset) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_nop(&mut self) -> bool {
        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::Internal(4)) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_stop(&mut self) -> bool {
        if self.ext_words.is_empty() {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::FetchExtWords(1)) || !self.recipe_push(RecipeOp::Stop) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_pea(&mut self, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        if matches!(
            addr_mode,
            AddrMode::DataReg(_)
                | AddrMode::AddrReg(_)
                | AddrMode::Immediate
                | AddrMode::AddrIndPostInc(_)
                | AddrMode::AddrIndPreDec(_)
        ) {
            return false;
        }

        self.size = Size::Long;
        self.src_mode = None;
        self.dst_mode = Some(addr_mode);

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }

        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
            || !self.recipe_push(RecipeOp::LoadEaAddr(EaSide::Dst))
        {
            self.recipe_reset();
            return false;
        }

        let internal = match addr_mode {
            AddrMode::AddrInd(_) => 4,
            AddrMode::AddrIndDisp(_) | AddrMode::AbsShort | AddrMode::PcDisp => 8,
            AddrMode::AddrIndIndex(_) | AddrMode::AbsLong | AddrMode::PcIndex => 12,
            _ => 0,
        };

        if !self.recipe_push(RecipeOp::Internal(internal))
            || !self.recipe_push(RecipeOp::PushLong)
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_swap(&mut self, reg: u8) -> bool {
        self.size = Size::Long;
        self.src_mode = None;
        self.dst_mode = Some(AddrMode::DataReg(reg));

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::SwapReg { reg }) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_ext(&mut self, size: Size, reg: u8) -> bool {
        self.size = size;
        self.src_mode = None;
        self.dst_mode = Some(AddrMode::DataReg(reg));

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::Ext { size, reg }) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_move_to_ccr(&mut self, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }

        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::WriteCcr)
            || !self.recipe_push(RecipeOp::Internal(12))
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_move_to_sr(&mut self, mode: u8, ea_reg: u8) -> bool {
        if !self.regs.is_supervisor() {
            self.exception(8);
            return true;
        }

        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }

        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::WriteSr)
            || !self.recipe_push(RecipeOp::Internal(12))
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_unary(
        &mut self,
        op: RecipeUnary,
        size: Option<Size>,
        mode: u8,
        ea_reg: u8,
    ) -> bool {
        let Some(size) = size else {
            return false;
        };
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_) | AddrMode::Immediate) {
            return false;
        }
        if !addr_mode.is_data_alterable() && !matches!(addr_mode, AddrMode::DataReg(_)) {
            return false;
        }

        self.size = size;
        self.src_mode = None;
        self.dst_mode = Some(addr_mode);

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }

        match addr_mode {
            AddrMode::DataReg(r) => {
                if !self.recipe_push(RecipeOp::UnaryReg { op, reg: r }) {
                    self.recipe_reset();
                    return false;
                }
            }
            _ => {
                if !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
                    || !self.recipe_push(RecipeOp::UnaryMem { op })
                {
                    self.recipe_reset();
                    return false;
                }
            }
        }

        self.recipe_commit()
    }

    fn recipe_lea(&mut self, reg: u8, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(
            addr_mode,
            AddrMode::DataReg(_)
                | AddrMode::AddrReg(_)
                | AddrMode::Immediate
                | AddrMode::AddrIndPostInc(_)
                | AddrMode::AddrIndPreDec(_)
        ) {
            return false;
        }

        self.size = Size::Long;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::AddrReg(reg));

        let src_ext = self.ext_words_for_mode(addr_mode);
        if (src_ext as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if src_ext > 0 && !self.recipe_push(RecipeOp::FetchExtWords(src_ext)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::LoadEaAddr(EaSide::Src))
            || !self.recipe_push(RecipeOp::WriteEa(EaSide::Dst))
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_movem_to_mem(&mut self, op: u16, mode: u8, ea_reg: u8) -> bool {
        let size = if op & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };

        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(
            addr_mode,
            AddrMode::DataReg(_)
                | AddrMode::AddrReg(_)
                | AddrMode::Immediate
                | AddrMode::PcDisp
                | AddrMode::PcIndex
        ) {
            return false;
        }

        self.size = size;
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;

        let ext_needed = 1 + self.ext_words_for_mode(addr_mode);
        if (ext_needed as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc.wrapping_add(2);
        self.recipe_dst_pc_at_ext = self.instr_start_pc.wrapping_add(2);

        if !self.recipe_push(RecipeOp::FetchExtWords(ext_needed))
            || !self.recipe_push(RecipeOp::SkipExt(1))
            || !self.recipe_push(RecipeOp::MovemToMem)
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_movem_from_mem(&mut self, op: u16, mode: u8, ea_reg: u8) -> bool {
        let size = if op & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };

        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        self.size = size;
        self.src_mode = None;
        self.dst_mode = Some(addr_mode);

        let ext_needed = 1 + self.ext_words_for_mode(addr_mode);
        if (ext_needed as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc.wrapping_add(2);
        self.recipe_dst_pc_at_ext = self.instr_start_pc.wrapping_add(2);

        if !self.recipe_push(RecipeOp::FetchExtWords(ext_needed))
            || !self.recipe_push(RecipeOp::SkipExt(1))
            || !self.recipe_push(RecipeOp::MovemFromMem)
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_clr(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) -> bool {
        let Some(size) = size else {
            return false;
        };
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_) | AddrMode::Immediate) {
            return false;
        }
        if !addr_mode.is_data_alterable() && !matches!(addr_mode, AddrMode::DataReg(_)) {
            return false;
        }

        self.size = size;
        self.src_mode = None;
        self.dst_mode = Some(addr_mode);
        self.recipe_imm = 0;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
            || !self.recipe_push(RecipeOp::LoadImm)
            || !self.recipe_push(RecipeOp::WriteEa(EaSide::Dst))
            || !self.recipe_push(RecipeOp::SetFlagsMove)
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_tst(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) -> bool {
        let Some(size) = size else {
            return false;
        };
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        self.size = size;
        self.src_mode = Some(addr_mode);
        self.dst_mode = None;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::SetFlagsMove)
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_or(&mut self, op: u16) -> bool {
        if (op >> 6) & 3 == 3 {
            return false; // DIV
        }
        if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            return false; // SBCD
        }
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let to_ea = op & 0x0100 != 0;
        self.recipe_binop(size, RecipeAlu::Or, reg, mode, ea_reg, to_ea)
    }

    fn recipe_group_8(&mut self, op: u16) -> bool {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // DIVU, DIVS
            if op & 0x0100 != 0 {
                return self.recipe_div(true, reg, mode, ea_reg);
            }
            return self.recipe_div(false, reg, mode, ea_reg);
        }

        if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            return self.recipe_sbcd(op);
        }

        self.recipe_or(op)
    }

    fn recipe_div(&mut self, signed: bool, reg: u8, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::DataReg(reg));
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::Div { signed, reg })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_sbcd(&mut self, op: u16) -> bool {
        let rx = (op & 7) as u8;
        let ry = ((op >> 9) & 7) as u8;
        let rm = op & 0x0008 != 0;

        self.size = Size::Byte;
        if rm {
            self.src_mode = Some(AddrMode::AddrIndPreDec(rx));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(ry));
        } else {
            self.src_mode = Some(AddrMode::DataReg(rx));
            self.dst_mode = Some(AddrMode::DataReg(ry));
        }
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        let ok = if rm {
            self.recipe_push(RecipeOp::ExtendMem {
                op: 1,
                src: rx,
                dst: ry,
            })
        } else {
            self.recipe_push(RecipeOp::SbcdReg { src: rx, dst: ry })
        };
        if !ok {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_sub(&mut self, op: u16) -> bool {
        if (op >> 6) & 3 == 3 {
            return false; // SUBA
        }
        if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            return false; // SUBX
        }
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let to_ea = op & 0x0100 != 0;
        if !to_ea {
            return self.recipe_sub_to_reg(size, reg, mode, ea_reg);
        }
        self.recipe_binop(size, RecipeAlu::Sub, reg, mode, ea_reg, to_ea)
    }

    fn recipe_sub_to_reg(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
    ) -> bool {
        let Some(size) = size else {
            return false;
        };
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        self.size = size;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::DataReg(reg));

        let src_ext = self.ext_words_for_mode(addr_mode);
        if (src_ext as usize) > self.ext_words.len() {
            self.recipe_reset();
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if src_ext > 0 && !self.recipe_push(RecipeOp::FetchExtWords(src_ext)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::AluReg {
                op: RecipeAlu::Sub,
                reg,
            })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_subx(&mut self, op: u16) -> bool {
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };
        let rm = op & 0x0008 != 0;
        let src = (op & 7) as u8;
        let dst = ((op >> 9) & 7) as u8;

        self.size = size;
        if rm {
            self.src_mode = Some(AddrMode::AddrIndPreDec(src));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(dst));
        } else {
            self.src_mode = Some(AddrMode::DataReg(src));
            self.dst_mode = Some(AddrMode::DataReg(dst));
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        let ok = if rm {
            self.recipe_push(RecipeOp::ExtendMem {
                op: 3,
                src,
                dst,
            })
        } else {
            self.recipe_push(RecipeOp::SubxReg { src, dst })
        };

        if !ok {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_add(&mut self, op: u16) -> bool {
        if (op >> 6) & 3 == 3 {
            return false; // ADDA
        }
        if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            return false; // ADDX
        }
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let to_ea = op & 0x0100 != 0;
        if !to_ea {
            return self.recipe_add_to_reg(size, reg, mode, ea_reg);
        }
        self.recipe_binop(size, RecipeAlu::Add, reg, mode, ea_reg, to_ea)
    }

    fn recipe_add_to_reg(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
    ) -> bool {
        let Some(size) = size else {
            return false;
        };
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        self.size = size;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::DataReg(reg));

        let src_ext = self.ext_words_for_mode(addr_mode);
        if (src_ext as usize) > self.ext_words.len() {
            self.recipe_reset();
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if src_ext > 0 && !self.recipe_push(RecipeOp::FetchExtWords(src_ext)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::AluReg {
                op: RecipeAlu::Add,
                reg,
            })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_addx(&mut self, op: u16) -> bool {
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };
        let rm = op & 0x0008 != 0;
        let src = (op & 7) as u8;
        let dst = ((op >> 9) & 7) as u8;

        self.size = size;
        if rm {
            self.src_mode = Some(AddrMode::AddrIndPreDec(src));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(dst));
        } else {
            self.src_mode = Some(AddrMode::DataReg(src));
            self.dst_mode = Some(AddrMode::DataReg(dst));
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        let ok = if rm {
            self.recipe_push(RecipeOp::ExtendMem {
                op: 2,
                src,
                dst,
            })
        } else {
            self.recipe_push(RecipeOp::AddxReg { src, dst })
        };

        if !ok {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_adda(&mut self, op: u16) -> bool {
        let size = if op & 0x0100 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        self.recipe_addr_arith(size, reg, mode, ea_reg, true)
    }

    fn recipe_suba(&mut self, op: u16) -> bool {
        let size = if op & 0x0100 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        self.recipe_addr_arith(size, reg, mode, ea_reg, false)
    }

    fn recipe_shift_rotate(&mut self, op: u16) -> bool {
        let size_bits = ((op >> 6) & 3) as u8;
        if size_bits == 3 {
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;
            let direction = (op >> 8) & 1 != 0;
            let kind = ((op >> 9) & 3) as u8;
            return self.recipe_shift_mem(kind, direction, mode, ea_reg);
        }

        let size = Size::from_bits(size_bits);
        let Some(size) = size else {
            return false;
        };
        let count_or_reg = ((op >> 9) & 7) as u8;
        let reg = (op & 7) as u8;
        let direction = (op >> 8) & 1 != 0;
        let immediate = (op >> 5) & 1 == 0;
        let kind = ((op >> 9) & 3) as u8;
        self.recipe_shift_reg(kind, direction, count_or_reg, reg, size, immediate)
    }

    fn recipe_shift_reg(
        &mut self,
        kind: u8,
        direction: bool,
        count_or_reg: u8,
        reg: u8,
        size: Size,
        immediate: bool,
    ) -> bool {
        self.size = size;
        self.src_mode = None;
        self.dst_mode = Some(AddrMode::DataReg(reg));

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if !self.recipe_push(RecipeOp::ShiftReg {
            kind,
            direction,
            count_or_reg,
            reg,
            immediate,
        }) {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_shift_mem(
        &mut self,
        kind: u8,
        direction: bool,
        mode: u8,
        ea_reg: u8,
    ) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if !addr_mode.is_memory_alterable() {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = None;
        self.dst_mode = Some(addr_mode);

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
            || !self.recipe_push(RecipeOp::ShiftMem { kind, direction })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_addr_arith(
        &mut self,
        size: Size,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        add: bool,
    ) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        self.size = size;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::AddrReg(reg));

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::AddrArith { reg, add })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_group_b(&mut self, op: u16) -> bool {
        let mode = ((op >> 3) & 7) as u8;

        if (op >> 6) & 3 == 3 {
            return self.recipe_cmpa(op);
        }
        if op & 0x0100 != 0 {
            if mode == 1 {
                return self.recipe_cmpm(op);
            }
            return self.recipe_eor(op);
        }
        self.recipe_cmp(op)
    }

    fn recipe_cmpm(&mut self, op: u16) -> bool {
        let ay = (op & 7) as u8;
        let ax = ((op >> 9) & 7) as u8;
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };

        self.size = size;
        self.src_mode = Some(AddrMode::AddrIndPostInc(ay));
        self.dst_mode = Some(AddrMode::AddrIndPostInc(ax));
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::Cmpm { ax, ay }) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_cmp(&mut self, op: u16) -> bool {
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            return false;
        };
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        self.size = size;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::DataReg(reg));

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::CmpReg { reg, addr: false })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_cmpa(&mut self, op: u16) -> bool {
        let size = if op & 0x0100 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };

        self.size = size;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::AddrReg(reg));

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::CmpReg { reg, addr: true })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_and(&mut self, op: u16) -> bool {
        if (op >> 6) & 3 == 3 {
            return false; // MUL
        }
        if op & 0x0100 != 0 {
            let opmode = (op >> 3) & 0x1F;
            match opmode {
                0x08 | 0x09 | 0x11 | 0x00 | 0x01 => return false, // EXG/ABCD
                _ => {}
            }
        }
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        let to_ea = op & 0x0100 != 0;
        self.recipe_binop(size, RecipeAlu::And, reg, mode, ea_reg, to_ea)
    }

    fn recipe_group_c(&mut self, op: u16) -> bool {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // MULU, MULS
            if op & 0x0100 != 0 {
                return self.recipe_mul(true, reg, mode, ea_reg);
            }
            return self.recipe_mul(false, reg, mode, ea_reg);
        }

        if op & 0x0100 != 0 {
            let opmode = (op >> 3) & 0x1F;
            match opmode {
                0x08 | 0x09 | 0x11 => return self.recipe_exg(op),
                0x00 | 0x01 => return self.recipe_abcd(op),
                _ => {}
            }
        }

        self.recipe_and(op)
    }

    fn recipe_mul(&mut self, signed: bool, reg: u8, mode: u8, ea_reg: u8) -> bool {
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::AddrReg(_)) {
            return false;
        }

        self.size = Size::Word;
        self.src_mode = Some(addr_mode);
        self.dst_mode = Some(AddrMode::DataReg(reg));
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        let ext_count = self.ext_words_for_mode(addr_mode);
        if (ext_count as usize) > self.ext_words.len() {
            return false;
        }

        self.recipe_begin();
        if ext_count > 0 && !self.recipe_push(RecipeOp::FetchExtWords(ext_count)) {
            self.recipe_reset();
            return false;
        }
        if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
            || !self.recipe_push(RecipeOp::Mul { signed, reg })
        {
            self.recipe_reset();
            return false;
        }

        self.recipe_commit()
    }

    fn recipe_exg(&mut self, op: u16) -> bool {
        let rx = ((op >> 9) & 7) as u8;
        let ry = (op & 7) as u8;
        let mode = (op >> 3) & 0x1F;

        if !matches!(mode, 0x08 | 0x09 | 0x11) {
            return false;
        }

        self.size = Size::Long;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        if !self.recipe_push(RecipeOp::Exg {
            kind: mode as u8,
            rx,
            ry,
        }) {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_abcd(&mut self, op: u16) -> bool {
        let rx = (op & 7) as u8;
        let ry = ((op >> 9) & 7) as u8;
        let rm = op & 0x0008 != 0;

        self.size = Size::Byte;
        if rm {
            self.src_mode = Some(AddrMode::AddrIndPreDec(rx));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(ry));
        } else {
            self.src_mode = Some(AddrMode::DataReg(rx));
            self.dst_mode = Some(AddrMode::DataReg(ry));
        }
        self.recipe_src_pc_at_ext = self.instr_start_pc;
        self.recipe_dst_pc_at_ext = self.instr_start_pc;

        self.recipe_begin();
        let ok = if rm {
            self.recipe_push(RecipeOp::ExtendMem {
                op: 0,
                src: rx,
                dst: ry,
            })
        } else {
            self.recipe_push(RecipeOp::AbcdReg { src: rx, dst: ry })
        };
        if !ok {
            self.recipe_reset();
            return false;
        }
        self.recipe_commit()
    }

    fn recipe_eor(&mut self, op: u16) -> bool {
        if (op >> 6) & 3 == 3 {
            return false; // CMPA
        }
        if op & 0x0100 == 0 {
            return false; // CMP
        }
        let mode = ((op >> 3) & 7) as u8;
        if mode == 1 {
            return false; // CMPM
        }
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let reg = ((op >> 9) & 7) as u8;
        let ea_reg = (op & 7) as u8;
        // EOR is always Dn -> <ea>
        self.recipe_binop(size, RecipeAlu::Eor, reg, mode, ea_reg, true)
    }

    fn recipe_binop(
        &mut self,
        size: Option<Size>,
        op: RecipeAlu,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        to_ea: bool,
    ) -> bool {
        let Some(size) = size else {
            return false;
        };
        let Some(addr_mode) = AddrMode::decode(mode, ea_reg) else {
            return false;
        };
        if matches!(addr_mode, AddrMode::Immediate | AddrMode::AddrReg(_)) {
            return false;
        }
        if to_ea && !addr_mode.is_data_alterable() {
            return false;
        }

        self.size = size;
        self.recipe_begin();

        if to_ea {
            self.src_mode = Some(AddrMode::DataReg(reg));
            self.dst_mode = Some(addr_mode);

            let dst_ext = self.ext_words_for_mode(addr_mode);
            if (dst_ext as usize) > self.ext_words.len() {
                self.recipe_reset();
                return false;
            }
            self.recipe_src_pc_at_ext = self.instr_start_pc;
            self.recipe_dst_pc_at_ext = self.instr_start_pc;

            if dst_ext > 0 && !self.recipe_push(RecipeOp::FetchExtWords(dst_ext)) {
                self.recipe_reset();
                return false;
            }
            if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
                || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
                || !self.recipe_push(RecipeOp::CalcEa(EaSide::Dst))
                || !self.recipe_push(RecipeOp::ReadEa(EaSide::Dst))
                || !self.recipe_push(RecipeOp::AluMem { op })
                || !self.recipe_push(RecipeOp::WriteEa(EaSide::Dst))
            {
                self.recipe_reset();
                return false;
            }
        } else {
            self.src_mode = Some(addr_mode);
            self.dst_mode = Some(AddrMode::DataReg(reg));

            let src_ext = self.ext_words_for_mode(addr_mode);
            if (src_ext as usize) > self.ext_words.len() {
                self.recipe_reset();
                return false;
            }
            self.recipe_src_pc_at_ext = self.instr_start_pc;
            self.recipe_dst_pc_at_ext = self.instr_start_pc;

            if src_ext > 0 && !self.recipe_push(RecipeOp::FetchExtWords(src_ext)) {
                self.recipe_reset();
                return false;
            }
            if !self.recipe_push(RecipeOp::CalcEa(EaSide::Src))
                || !self.recipe_push(RecipeOp::ReadEa(EaSide::Src))
                || !self.recipe_push(RecipeOp::AluReg { op, reg })
            {
                self.recipe_reset();
                return false;
            }
        }

        self.recipe_commit()
    }

    /// Group 0: Bit manipulation, MOVEP, Immediate
    fn decode_group_0(&mut self, op: u16) {
        // Encoding guide for Group 0:
        // - Bit 8 set + mode=001: MOVEP (move peripheral data)
        // - Bit 8 set + mode!=001: Dynamic bit operations (BTST/BCHG/BCLR/BSET with register)
        // - Bits 11-9 = 100: Static bit operations (BTST/BCHG/BCLR/BSET with immediate)
        // - Otherwise: Immediate arithmetic/logic (ORI, ANDI, SUBI, ADDI, EORI, CMPI)

        let mode = ((op >> 3) & 7) as u8;

        if op & 0x0100 != 0 {
            if mode == 1 {
                // MOVEP - Move Peripheral Data
                // Encoding: 0000_rrr1_0sD0_1aaa
                // rrr = data register, s = size (0=word, 1=long)
                // D = direction (0=mem to reg, 1=reg to mem), aaa = address register
                self.exec_movep(op);
            } else {
                // Bit operations with register (dynamic bit number in Dn)
                let reg = ((op >> 9) & 7) as u8;
                let ea_reg = (op & 7) as u8;

                match (op >> 6) & 3 {
                    0 => self.exec_btst_reg(reg, mode, ea_reg),
                    1 => self.exec_bchg_reg(reg, mode, ea_reg),
                    2 => self.exec_bclr_reg(reg, mode, ea_reg),
                    3 => self.exec_bset_reg(reg, mode, ea_reg),
                    _ => unreachable!(),
                }
            }
        } else {
            // Check bits 11-9 for operation type
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;

            match (op >> 9) & 7 {
                0 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_ori(size, mode, ea_reg);
                }
                1 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_andi(size, mode, ea_reg);
                }
                2 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_subi(size, mode, ea_reg);
                }
                3 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_addi(size, mode, ea_reg);
                }
                4 => {
                    // Static bit operations with immediate bit number
                    match (op >> 6) & 3 {
                        0 => self.exec_btst_imm(mode, ea_reg),
                        1 => self.exec_bchg_imm(mode, ea_reg),
                        2 => self.exec_bclr_imm(mode, ea_reg),
                        3 => self.exec_bset_imm(mode, ea_reg),
                        _ => unreachable!(),
                    }
                }
                5 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_eori(size, mode, ea_reg);
                }
                6 => {
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_cmpi(size, mode, ea_reg);
                }
                7 => self.illegal_instruction(),
                _ => unreachable!(),
            }
        }
    }

    /// MOVE.B instruction
    fn decode_move_byte(&mut self, op: u16) {
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;

        self.exec_move(Size::Byte, src_mode, src_reg, dst_mode, dst_reg);
    }

    /// MOVE.L instruction
    fn decode_move_long(&mut self, op: u16) {
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;

        self.exec_move(Size::Long, src_mode, src_reg, dst_mode, dst_reg);
    }

    /// MOVE.W instruction
    fn decode_move_word(&mut self, op: u16) {
        let dst_reg = ((op >> 9) & 7) as u8;
        let dst_mode = ((op >> 6) & 7) as u8;
        let src_mode = ((op >> 3) & 7) as u8;
        let src_reg = (op & 7) as u8;

        self.exec_move(Size::Word, src_mode, src_reg, dst_mode, dst_reg);
    }

    /// Group 4: Miscellaneous
    fn decode_group_4(&mut self, op: u16) {
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if op & 0x0100 != 0 {
            // CHK, LEA
            if (op >> 6) & 7 == 7 {
                let reg = ((op >> 9) & 7) as u8;
                self.exec_lea(reg, mode, ea_reg);
            } else {
                // CHK
                self.exec_chk(op);
            }
        } else {
            match (op >> 8) & 0xF {
                0x0 => {
                    // NEGX, MOVE from SR
                    if (op >> 6) & 3 == 3 {
                        self.exec_move_from_sr(mode, ea_reg);
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_negx(size, mode, ea_reg);
                    }
                }
                0x2 => {
                    // CLR
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_clr(size, mode, ea_reg);
                }
                0x4 => {
                    // NEG, MOVE to CCR
                    if (op >> 6) & 3 == 3 {
                        self.exec_move_to_ccr(mode, ea_reg);
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_neg(size, mode, ea_reg);
                    }
                }
                0x6 => {
                    // NOT, MOVE to SR
                    if (op >> 6) & 3 == 3 {
                        self.exec_move_to_sr(mode, ea_reg);
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_not(size, mode, ea_reg);
                    }
                }
                0x8 => {
                    // NBCD, SWAP, PEA, EXT, MOVEM
                    match (op >> 6) & 3 {
                        0 => self.exec_nbcd(mode, ea_reg),
                        1 => {
                            if mode == 0 {
                                self.exec_swap(ea_reg);
                            } else {
                                self.exec_pea(mode, ea_reg);
                            }
                        }
                        2 | 3 => {
                            if mode == 0 {
                                let size = if (op >> 6) & 1 == 0 {
                                    Size::Word
                                } else {
                                    Size::Long
                                };
                                self.exec_ext(size, ea_reg);
                            } else {
                                self.exec_movem_to_mem(op);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                0xA => {
                    // TST, TAS, ILLEGAL
                    if (op >> 6) & 3 == 3 {
                        if op == 0x4AFC {
                            self.illegal_instruction();
                        } else {
                            self.exec_tas(mode, ea_reg);
                        }
                    } else {
                        let size = Size::from_bits(((op >> 6) & 3) as u8);
                        self.exec_tst(size, mode, ea_reg);
                    }
                }
                0xC => {
                    // MOVEM from memory
                    self.exec_movem_from_mem(op);
                }
                0xE => {
                    // JSR, JMP, Trap, LINK, UNLK, MOVE USP, etc.
                    // Differentiated by bits 7-6
                    let subop = (op >> 6) & 3;
                    if subop == 2 {
                        // JSR <ea> (0x4E80-0x4EBF)
                        self.exec_jsr(mode, ea_reg);
                    } else if subop == 3 {
                        // JMP <ea> (0x4EC0-0x4EFF)
                        self.exec_jmp(mode, ea_reg);
                    } else {
                        // TRAP, LINK, UNLK, MOVE USP, misc (0x4E00-0x4E7F)
                        match (op >> 4) & 0xF {
                            0x4 => self.exec_trap(op),
                            0x5 => {
                                if op & 8 != 0 {
                                    self.exec_unlk((op & 7) as u8);
                                } else {
                                    self.exec_link((op & 7) as u8);
                                }
                            }
                            0x6 => self.exec_move_usp(op),
                            0x7 => match op & 0xF {
                                0 => self.exec_reset(),
                                1 => self.exec_nop(),
                                2 => self.exec_stop(),
                                3 => self.exec_rte(),
                                5 => self.exec_rts(),
                                6 => self.exec_trapv(),
                                7 => self.exec_rtr(),
                                _ => self.illegal_instruction(),
                            },
                            _ => self.illegal_instruction(),
                        }
                    }
                }
                _ => self.illegal_instruction(),
            }
        }
    }

    /// Group 5: ADDQ, SUBQ, Scc, DBcc
    fn decode_group_5(&mut self, op: u16) {
        let data = ((op >> 9) & 7) as u8;
        let data = if data == 0 { 8 } else { data }; // 0 encodes as 8

        if (op >> 6) & 3 == 3 {
            // Scc, DBcc
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;
            let condition = ((op >> 8) & 0xF) as u8;

            if mode == 1 {
                // DBcc
                self.exec_dbcc(condition, ea_reg);
            } else {
                // Scc
                self.exec_scc(condition, mode, ea_reg);
            }
        } else {
            // ADDQ, SUBQ
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;

            if op & 0x0100 != 0 {
                self.exec_subq(size, data, mode, ea_reg);
            } else {
                self.exec_addq(size, data, mode, ea_reg);
            }
        }
    }

    /// Group 6: Bcc, BSR, BRA
    fn decode_group_6(&mut self, op: u16) {
        let condition = ((op >> 8) & 0xF) as u8;
        let displacement = (op & 0xFF) as i8;

        match condition {
            0 => self.exec_bra(displacement),  // BRA
            1 => self.exec_bsr(displacement),  // BSR
            _ => self.exec_bcc(condition, displacement),
        }
    }

    /// MOVEQ instruction
    fn decode_moveq(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let data = (op & 0xFF) as i8;
        self.exec_moveq(reg, data);
    }

    /// Group 8: OR, DIV, SBCD
    fn decode_group_8(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // DIVU, DIVS
            if op & 0x0100 != 0 {
                self.exec_divs(reg, mode, ea_reg);
            } else {
                self.exec_divu(reg, mode, ea_reg);
            }
        } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            // SBCD
            self.exec_sbcd(op);
        } else {
            // OR
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_or(size, reg, mode, ea_reg, op & 0x0100 != 0);
        }
    }

    /// SUB instruction
    fn decode_sub(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // SUBA
            let size = if op & 0x0100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            self.exec_suba(size, reg, mode, ea_reg);
        } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            // SUBX
            self.exec_subx(op);
        } else {
            // SUB
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_sub(size, reg, mode, ea_reg, op & 0x0100 != 0);
        }
    }

    /// Line A: Unimplemented (used for OS calls on some systems)
    fn decode_line_a(&mut self, _op: u16) {
        self.exception(10); // Line A exception
    }

    /// Group B: CMP, EOR
    fn decode_group_b(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // CMPA
            let size = if op & 0x0100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            self.exec_cmpa(size, reg, mode, ea_reg);
        } else if op & 0x0100 != 0 {
            if mode == 1 {
                // CMPM
                self.exec_cmpm(op);
            } else {
                // EOR
                let size = Size::from_bits(((op >> 6) & 3) as u8);
                self.exec_eor(size, reg, mode, ea_reg);
            }
        } else {
            // CMP
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_cmp(size, reg, mode, ea_reg);
        }
    }

    /// Group C: AND, MUL, ABCD, EXG
    fn decode_group_c(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if (op >> 6) & 3 == 3 {
            // MULU, MULS
            if op & 0x0100 != 0 {
                self.exec_muls(reg, mode, ea_reg);
            } else {
                self.exec_mulu(reg, mode, ea_reg);
            }
        } else if op & 0x0100 != 0 {
            // Check opmode field (bits 3-7)
            let opmode = (op >> 3) & 0x1F;
            match opmode {
                0x08 => {
                    // EXG Dx,Dy: opmode = 01000
                    self.exec_exg(op);
                }
                0x09 => {
                    // EXG Ax,Ay: opmode = 01001
                    self.exec_exg(op);
                }
                0x11 => {
                    // EXG Dx,Ay: opmode = 10001
                    self.exec_exg(op);
                }
                0x00 | 0x01 => {
                    // ABCD: opmode = 0000x (bit 3 is R/M)
                    self.exec_abcd(op);
                }
                _ => {
                    // AND Dn,<ea>
                    let size = Size::from_bits(((op >> 6) & 3) as u8);
                    self.exec_and(size, reg, mode, ea_reg, true);
                }
            }
        } else {
            // AND <ea>,Dn
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_and(size, reg, mode, ea_reg, false);
        }
    }

    /// ADD instruction
    fn decode_add(&mut self, op: u16) {
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
            let op_pc = self.instr_start_pc.wrapping_sub(2);
            let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
            if self.instr_phase != InstrPhase::Initial {
                eprintln!(
                    "[CPU] decode_add CONT op=${op:04X} pc=${op_pc:08X} mode={mode} reg={reg} ea_reg={ea_reg} phase={:?}",
                    self.instr_phase
                );
            } else if log_this {
                eprintln!(
                    "[CPU] decode_add op=${op:04X} pc=${op_pc:08X} mode={mode} reg={reg} ea_reg={ea_reg} phase={:?}",
                    self.instr_phase
                );
            }
        }
        if (op >> 6) & 3 == 3 {
            // ADDA
            let size = if op & 0x0100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            self.exec_adda(size, reg, mode, ea_reg);
        } else if op & 0x0100 != 0 && (op >> 4) & 3 == 0 {
            // ADDX
            self.exec_addx(op);
        } else {
            // ADD
            let size = Size::from_bits(((op >> 6) & 3) as u8);
            self.exec_add(size, reg, mode, ea_reg, op & 0x0100 != 0);
        }
    }

    /// Shift and rotate instructions
    fn decode_shift_rotate(&mut self, op: u16) {
        let count_or_reg = ((op >> 9) & 7) as u8;
        let reg = (op & 7) as u8;
        let size = Size::from_bits(((op >> 6) & 3) as u8);

        if (op >> 6) & 3 == 3 {
            // Memory shift/rotate (always word size, shift by 1)
            let mode = ((op >> 3) & 7) as u8;
            let ea_reg = (op & 7) as u8;
            let direction = (op >> 8) & 1 != 0; // 0=right, 1=left
            let kind = ((op >> 9) & 3) as u8;
            self.exec_shift_mem(kind, direction, mode, ea_reg);
        } else {
            // Register shift/rotate
            let direction = (op >> 8) & 1 != 0;
            let immediate = (op >> 5) & 1 == 0; // 0=count in opcode, 1=count in register
            let kind = ((op >> 3) & 3) as u8;
            self.exec_shift_reg(kind, direction, count_or_reg, reg, size, immediate);
        }
    }

    /// Line F: Unimplemented (used for coprocessor on 68020+)
    fn decode_line_f(&mut self, _op: u16) {
        if std::env::var("EMU68000_TRACE_LINEF").is_ok() {
            use std::sync::atomic::{AtomicBool, Ordering};
            static LOGGED: AtomicBool = AtomicBool::new(false);
            if !LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "[CPU] Line F exception: op=${:04X} pc=${:08X}",
                    self.opcode,
                    self.regs.pc
                );
            }
        }
        self.exception(11); // Line F exception
    }

    /// Trigger illegal instruction exception.
    fn illegal_instruction(&mut self) {
        if std::env::var("EMU68000_TRACE_ILLEGAL").is_ok() {
            eprintln!(
                "[CPU] Illegal instruction: op=${:04X} pc=${:08X}",
                self.opcode,
                self.regs.pc
            );
        }
        self.exception(4); // Illegal instruction vector
    }

    // === Instruction implementations ===

    fn exec_move(&mut self, size: Size, src_mode: u8, src_reg: u8, dst_mode: u8, dst_reg: u8) {
        use crate::cpu::InstrPhase;

        let src = AddrMode::decode(src_mode, src_reg);
        let dst = AddrMode::decode(dst_mode, dst_reg);

        let Some(src_mode) = src else {
            self.illegal_instruction();
            return;
        };
        let Some(dst_mode) = dst else {
            self.illegal_instruction();
            return;
        };

        // Check for MOVEA (destination is address register)
        let is_movea = matches!(dst_mode, AddrMode::AddrReg(_));

        match self.instr_phase {
            InstrPhase::Initial => {
                // Setup
                self.src_mode = Some(src_mode);
                self.dst_mode = Some(dst_mode);
                self.size = size;
                self.ext_idx = 0;

                // Calculate how many extension words we need for source
                let src_ext = self.ext_words_for_mode(src_mode);
                let dst_ext = self.ext_words_for_mode(dst_mode);

                if src_ext + dst_ext == 0 {
                    // No extension words needed - direct register operations
                    self.exec_move_direct(src_mode, dst_mode, is_movea);
                } else {
                    // Queue fetching all extension words
                    for _ in 0..src_ext {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    for _ in 0..dst_ext {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
            InstrPhase::SrcEACalc => {
                // Extension words are fetched, calculate source EA
                let src_mode = self.src_mode.expect("src_mode should be set");
                let dst_mode = self.dst_mode.expect("dst_mode should be set");

                // PC value at start of extension words (after opcode)
                let pc_at_ext = self.regs.pc.wrapping_sub(
                    2 * u32::from(self.ext_count)
                );

                if std::env::var("EMU68000_TRACE_MOVE_IMM").is_ok()
                    && matches!(src_mode, AddrMode::Immediate)
                    && self.size == Size::Long
                    && self.opcode == 0x2C3C
                {
                    use std::sync::atomic::{AtomicBool, Ordering};
                    static LOGGED: AtomicBool = AtomicBool::new(false);
                    if !LOGGED.swap(true, Ordering::Relaxed) {
                        eprintln!(
                            "[CPU] MOVE.L #imm (2C3C): pc=${:08X} ext_count={} ext_words={:04X} {:04X}",
                            self.regs.pc,
                            self.ext_count,
                            self.ext_words[0],
                            self.ext_words[1]
                        );
                    }
                }

                // Handle source
                match src_mode {
                    AddrMode::DataReg(r) => {
                        self.data = self.read_data_reg(r, self.size);
                        self.instr_phase = InstrPhase::DstEACalc;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(r) => {
                        self.data = self.regs.a(r as usize);
                        self.instr_phase = InstrPhase::DstEACalc;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::Immediate => {
                        self.data = self.read_immediate();
                        self.instr_phase = InstrPhase::DstEACalc;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        // Memory source - calculate EA and queue read
                        let (addr, _is_reg) = self.calc_ea(src_mode, pc_at_ext);
                        self.addr = addr;
                        self.queue_read_ops(self.size);
                        self.instr_phase = InstrPhase::SrcRead;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }

                // Precompute destination EA only for modes that need extension words.
                // Avoid side effects (e.g., pre-decrement/post-increment) here.
                let dst_needs_ext = matches!(
                    dst_mode,
                    AddrMode::AddrIndDisp(_)
                        | AddrMode::AddrIndIndex(_)
                        | AddrMode::AbsShort
                        | AddrMode::AbsLong
                );
                if dst_needs_ext {
                    let pc_for_dst = pc_at_ext.wrapping_add(
                        2 * u32::from(self.ext_words_for_mode(src_mode))
                    );
                    let (addr, _) = self.calc_ea(dst_mode, pc_for_dst);
                    self.addr2 = addr;
                }
            }
            InstrPhase::SrcRead => {
                // Source data is now in self.data from memory read
                self.instr_phase = InstrPhase::DstEACalc;
                self.micro_ops.push(MicroOp::Execute);
            }
            InstrPhase::DstEACalc => {
                // Write to destination.
                // Reset program_space_access: the source EA may have used PC-relative
                // addressing (PcDisp/PcIndex) which sets this flag. The destination
                // write always uses data space, so clear it before any potential AE.
                self.program_space_access = false;
                let dst_mode = self.dst_mode.expect("dst_mode should be set");
                let value = self.data;

                match dst_mode {
                    AddrMode::DataReg(r) => {
                        self.write_data_reg(r, value, self.size);
                        if !is_movea {
                            self.set_flags_move(value, self.size);
                        }
                        self.instr_phase = InstrPhase::Complete;
                    }
                    AddrMode::AddrReg(r) => {
                        // MOVEA - sign extend word to long, don't affect flags
                        let value = if self.size == Size::Word {
                            value as i16 as i32 as u32
                        } else {
                            value
                        };
                        self.regs.set_a(r as usize, value);
                        self.instr_phase = InstrPhase::Complete;
                    }
                    AddrMode::AddrInd(r) => {
                        // Simple indirect
                        self.addr = self.regs.a(r as usize);
                        self.set_move_ae_flags_and_queue_write(value, is_movea);
                    }
                    AddrMode::AddrIndPostInc(r) => {
                        // Post-increment: defer until write succeeds.
                        // On AE, address_error() clears deferred_postinc so the
                        // register stays at its original value.
                        let addr = self.regs.a(r as usize);
                        let inc = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                        self.deferred_postinc = Some((r, inc));
                        self.addr = addr;
                        self.set_move_ae_flags_and_queue_write(value, is_movea);
                    }
                    AddrMode::AddrIndPreDec(r) => {
                        // Pre-decrement: modify register immediately. Unlike (An)+,
                        // the 68000 decrements the register BEFORE the write attempt,
                        // so it stays decremented even if an address error occurs.
                        let dec = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                        if std::env::var("EMU68000_TRACE_MOVE_PREDEC").is_ok() {
                            eprintln!(
                                "[CPU] MOVE PREDEC op=${:04X} size={:?} dec={} A{}=${:08X}",
                                self.opcode,
                                self.size,
                                dec,
                                r,
                                self.regs.a(r as usize)
                            );
                        }
                        let addr = self.regs.a(r as usize).wrapping_sub(dec);
                        self.regs.set_a(r as usize, addr);
                        self.addr = addr;
                        self.set_move_ae_flags_and_queue_write(value, is_movea);
                    }
                    AddrMode::AddrIndDisp(r) => {
                        let _ = r; // Addr was already computed in SrcEACalc
                        self.addr = self.addr2;
                        self.set_move_ae_flags_and_queue_write(value, is_movea);
                    }
                    AddrMode::AddrIndIndex(r) => {
                        let _ = r; // Addr was already computed in SrcEACalc
                        self.addr = self.addr2;
                        self.set_move_ae_flags_and_queue_write(value, is_movea);
                    }
                    AddrMode::AbsShort => {
                        self.addr = self.addr2;
                        self.set_move_ae_flags_and_queue_write(value, is_movea);
                    }
                    AddrMode::AbsLong => {
                        self.addr = self.addr2;
                        self.set_move_ae_flags_and_queue_write(value, is_movea);
                    }
                    _ => {
                        // PC-relative and Immediate are invalid destination modes
                        self.illegal_instruction();
                    }
                }
            }
            InstrPhase::DstWrite => {
                // Write completed — set MOVE flags now.
                // For MOVE.l destination writes, the 68000 computes flags after
                // both write bus cycles. If the first cycle AEs, this phase is
                // never reached, so the old flags are preserved in the frame.
                if !is_movea {
                    self.set_flags_move(self.data, self.size);
                }
                self.instr_phase = InstrPhase::Complete;
            }
            InstrPhase::Complete => {
                // Done
            }
        }
    }

    /// Set "AE-compatible" flags for a MOVE.l memory destination write.
    ///
    /// On the real 68000, MOVE.l flag computation timing depends on the source
    /// and destination addressing modes. When a destination write causes an
    /// address error, the flags in the exception frame reflect how far the
    /// microcode had progressed:
    ///
    /// - Memory source: full MOVE flags (NZVC from data, X preserved)
    /// - Register/immediate source + extension-word destination: N and Z from
    ///   data, V and C preserved from pre-instruction state
    /// - Register/immediate source + simple destination: all flags preserved
    ///
    /// This method sets the "partial" flags that would be in the frame on AE.
    /// After the write completes successfully, the DstWrite phase sets full
    /// MOVE flags — overwriting the partial state for the non-AE case.
    ///
    /// For Byte/Word, the flag computation and single write bus cycle are
    /// effectively atomic on the real 68000, so full MOVE flags are set
    /// eagerly (before the write). For Long, flags are deferred using a
    /// two-phase approach because the two write bus cycles create a window
    /// where an AE can capture partially-computed flags.
    fn set_move_ae_flags_and_queue_write(
        &mut self,
        value: u32,
        is_movea: bool,
    ) {
        if self.size != Size::Long || is_movea {
            // Byte/Word or MOVEA: set full flags eagerly, no deferred step needed
            self.queue_write_ops(self.size);
            if !is_movea {
                self.set_flags_move(value, self.size);
            }
            self.instr_phase = crate::cpu::InstrPhase::Complete;
            return;
        }

        // Size::Long — set partial "AE-compatible" flags before the write.
        // The partial state depends on source and destination modes.
        let src_mode = self.src_mode.expect("src_mode should be set for MOVE");
        let dst_mode = self.dst_mode.expect("dst_mode should be set for MOVE");
        let is_reg_or_imm_src = matches!(
            src_mode,
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
        );

        if is_reg_or_imm_src {
            let is_predec_dst = matches!(dst_mode, AddrMode::AddrIndPreDec(_));
            let is_abs_dst = matches!(
                dst_mode,
                AddrMode::AbsShort | AddrMode::AbsLong
            );
            if is_predec_dst || is_abs_dst {
                // Predecrement and absolute destinations: full MOVE flags.
                // The 68000 has fully committed the flag computation by this point.
                self.set_flags_move(value, self.size);
            } else {
                let dst_needs_ext = matches!(
                    dst_mode,
                    AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_)
                );
                if dst_needs_ext {
                    // Displacement/index destinations: N and Z from data, V and C preserved.
                    self.set_flags_move_nz_only(value, self.size);
                }
                // Other simple destinations (direct, postinc, indirect): all flags preserved.
            }
        } else {
            // Memory source MOVE.l: flag computation depends on destination mode.
            // For simple destinations with no extra EA cycles ((An), (An)+),
            // the last bus read was the source low word, so N/Z reflect word-sized
            // computation. For destinations with extra EA cycles (predecrement
            // internal cycle, extension word reads for d(An)/d(An,Xn)/abs.w),
            // the 68000 has time to compute full long-sized flags from the
            // assembled value.
            let is_simple_dst = matches!(
                dst_mode,
                AddrMode::AddrInd(_)
                    | AddrMode::AddrIndPostInc(_)
                    | AddrMode::AbsLong
            );
            if is_simple_dst {
                self.set_flags_move(value, Size::Word);
            } else {
                self.set_flags_move(value, self.size);
            }
        }

        // Queue write ops, then defer final flag setting to DstWrite
        self.queue_write_ops(self.size);
        self.micro_ops.push(MicroOp::Execute);
        self.instr_phase = crate::cpu::InstrPhase::DstWrite;
    }

    /// Execute direct register-to-register MOVE (no extension words).
    fn exec_move_direct(&mut self, src_mode: AddrMode, dst_mode: AddrMode, is_movea: bool) {
        // Read source
        let value = match src_mode {
            AddrMode::DataReg(r) => self.read_data_reg(r, self.size),
            AddrMode::AddrReg(r) => self.regs.a(r as usize),
            AddrMode::AddrInd(r) => {
                // Simple indirect - queue memory read
                self.addr = self.regs.a(r as usize);
                self.dst_mode = Some(dst_mode);
                self.queue_read_ops(self.size);
                self.instr_phase = crate::cpu::InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::AddrIndPostInc(r) => {
                let addr = self.regs.a(r as usize);
                let inc = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                // Defer the increment until after a successful read.
                self.deferred_postinc = Some((r, inc));
                self.addr = addr;
                self.dst_mode = Some(dst_mode);
                self.queue_read_ops(self.size);
                self.instr_phase = crate::cpu::InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                if std::env::var("EMU68000_TRACE_MOVE_PREDEC").is_ok() {
                    eprintln!(
                        "[CPU] MOVE PREDEC (src) op=${:04X} size={:?} dec={} A{}=${:08X}",
                        self.opcode,
                        self.size,
                        dec,
                        r,
                        self.regs.a(r as usize)
                    );
                }
                let addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, addr);
                self.addr = addr;
                self.dst_mode = Some(dst_mode);
                self.queue_read_ops(self.size);
                self.instr_phase = crate::cpu::InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            _ => {
                // Modes requiring extension words shouldn't reach here
                self.illegal_instruction();
                return;
            }
        };

        // Write destination
        match dst_mode {
            AddrMode::DataReg(r) => {
                self.write_data_reg(r, value, self.size);
                if !is_movea {
                    self.set_flags_move(value, self.size);
                }
            }
            AddrMode::AddrReg(r) => {
                let value = if self.size == Size::Word {
                    value as i16 as i32 as u32
                } else {
                    value
                };
                self.regs.set_a(r as usize, value);
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.data = value;
                self.set_move_ae_flags_and_queue_write(value, is_movea);
            }
            AddrMode::AddrIndPostInc(r) => {
                let addr = self.regs.a(r as usize);
                let inc = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                self.deferred_postinc = Some((r, inc));
                self.addr = addr;
                self.data = value;
                self.set_move_ae_flags_and_queue_write(value, is_movea);
            }
            AddrMode::AddrIndPreDec(r) => {
                // Pre-decrement: modify register immediately (see DstEACalc comment).
                let dec = if self.size == Size::Byte && r == 7 { 2 } else { self.size_increment() };
                if std::env::var("EMU68000_TRACE_MOVE_PREDEC").is_ok() {
                    eprintln!(
                        "[CPU] MOVE PREDEC (dst) op=${:04X} size={:?} dec={} A{}=${:08X}",
                        self.opcode,
                        self.size,
                        dec,
                        r,
                        self.regs.a(r as usize)
                    );
                }
                let addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, addr);
                self.addr = addr;
                self.data = value;
                self.set_move_ae_flags_and_queue_write(value, is_movea);
            }
            _ => {
                self.illegal_instruction();
            }
        }
    }

    /// MOVEP - Move Peripheral Data
    /// Transfers data between a data register and alternate bytes in memory.
    /// Used for accessing 8-bit peripherals on the 16-bit bus.
    fn exec_movep(&mut self, op: u16) {
        // Encoding: 0000_rrr1_0sD0_1aaa + 16-bit displacement
        // rrr = data register, s = size (0=word, 1=long)
        // D = direction (0=mem to reg, 1=reg to mem), aaa = address register
        let data_reg = ((op >> 9) & 7) as u8;
        let addr_reg = (op & 7) as u8;
        let is_long = op & 0x0040 != 0;
        let to_memory = op & 0x0080 != 0;

        // Need to fetch the displacement extension word first
        // Store fields in data2: bits 0-2=addr_reg, bits 4-6=data_reg, bit 8=is_long, bit 9=to_memory
        self.data2 = u32::from(addr_reg)
            | (u32::from(data_reg) << 4)
            | (if is_long { 0x100 } else { 0 })
            | (if to_memory { 0x200 } else { 0 });
        self.movem_long_phase = 0;
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.micro_ops.push(MicroOp::Execute); // Continue after fetching displacement
        self.instr_phase = InstrPhase::SrcRead;
    }

    fn exec_movep_with_disp(&mut self, data_reg: u8, addr_reg: u8, is_long: bool, to_memory: bool, displacement: i32) {
        let base_addr = (self.regs.a(addr_reg as usize) as i32).wrapping_add(displacement) as u32;

        // Update data2 for phase handler
        self.data2 = ((data_reg as u32) << 4) | (if to_memory { 0x200 } else { 0 });

        if to_memory {
            // Register to memory: write bytes to alternate addresses
            let value = self.regs.d[data_reg as usize];
            if is_long {
                // MOVEP.L Dn,d(An): 4 byte writes
                self.addr = base_addr;
                self.addr2 = value;
                self.data = (value >> 24) & 0xFF;
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W Dn,d(An): 2 byte writes
                self.addr = base_addr;
                self.addr2 = value;
                self.data = (value >> 8) & 0xFF;
                self.movem_long_phase = 4;
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            }
        } else {
            // Memory to register: read bytes from alternate addresses
            if is_long {
                // MOVEP.L d(An),Dn: 4 byte reads
                self.addr = base_addr;
                self.addr2 = 0;
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W d(An),Dn: 2 byte reads
                self.addr = base_addr;
                self.addr2 = 0;
                self.movem_long_phase = 4;
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute);
                self.instr_phase = InstrPhase::DstWrite;
            }
        }
    }

    /// Continuation for MOVEP after fetching displacement.
    fn exec_movep_continuation(&mut self) {
        // Extract fields from data2
        let addr_reg = (self.data2 & 7) as usize;
        let data_reg = ((self.data2 >> 4) & 7) as usize;
        let is_long = self.data2 & 0x100 != 0;
        let to_memory = self.data2 & 0x200 != 0;
        // Displacement is always in ext_words[0] - it's the first (and only) extension word
        let displacement = self.ext_words[0] as i16 as i32;

        let base_addr = (self.regs.a(addr_reg) as i32).wrapping_add(displacement) as u32;

        // Update data2 for phase handler
        self.data2 = (data_reg << 4) as u32 | (if to_memory { 0x200 } else { 0 });

        if to_memory {
            // Register to memory: write bytes to alternate addresses
            let value = self.regs.d[data_reg];
            if is_long {
                // MOVEP.L Dn,d(An): 4 byte writes
                self.addr = base_addr;
                self.addr2 = value; // Store full value for later phases
                self.data = (value >> 24) & 0xFF; // Byte 3 (MSB) to write first
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after write
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W Dn,d(An): 2 byte writes
                self.addr = base_addr;
                self.addr2 = value; // Store full value for later phases
                self.data = (value >> 8) & 0xFF; // High byte of word to write first
                self.movem_long_phase = 4; // Start at phase 4 for word
                self.micro_ops.push(MicroOp::WriteByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after write
                self.instr_phase = InstrPhase::DstWrite;
            }
        } else {
            // Memory to register: read bytes from alternate addresses
            if is_long {
                // MOVEP.L d(An),Dn: 4 byte reads
                self.addr = base_addr;
                self.addr2 = 0; // Will accumulate result
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after read
                self.instr_phase = InstrPhase::DstWrite;
            } else {
                // MOVEP.W d(An),Dn: 2 byte reads
                self.addr = base_addr;
                self.addr2 = 0; // Will accumulate result
                self.movem_long_phase = 4; // Start at phase 4 for word
                self.micro_ops.push(MicroOp::ReadByte);
                self.micro_ops.push(MicroOp::Execute); // Continue after read
                self.instr_phase = InstrPhase::DstWrite;
            }
        }
    }

    /// Handle MOVEP read/write phases.
    fn exec_movep_phase(&mut self) {
        // Extract fields from data2: bits 4-6=data_reg, bit 9=to_memory
        let data_reg = ((self.data2 >> 4) & 7) as usize;
        let to_memory = self.data2 & 0x200 != 0;

        if to_memory {
            // Writing: advance to next byte
            match self.movem_long_phase {
                0 => {
                    // Just wrote byte 3, write byte 2
                    self.addr = self.addr.wrapping_add(2);
                    self.data = (self.addr2 >> 16) & 0xFF; // Byte 2
                    self.movem_long_phase = 1;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                1 => {
                    // Just wrote byte 2, write byte 1
                    self.addr = self.addr.wrapping_add(2);
                    self.data = (self.addr2 >> 8) & 0xFF; // Byte 1
                    self.movem_long_phase = 2;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                2 => {
                    // Just wrote byte 1, write byte 0
                    self.addr = self.addr.wrapping_add(2);
                    self.data = self.addr2 & 0xFF; // Byte 0 (LSB)
                    self.movem_long_phase = 3;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                3 => {
                    // Done with long write
                    self.instr_phase = InstrPhase::Complete;
                }
                4 => {
                    // Word: just wrote high byte (bits 15-8), write low byte (bits 7-0)
                    self.addr = self.addr.wrapping_add(2);
                    self.data = self.addr2 & 0xFF; // Low byte of word
                    self.movem_long_phase = 5;
                    self.micro_ops.push(MicroOp::WriteByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                5 => {
                    // Done with word write
                    self.instr_phase = InstrPhase::Complete;
                }
                _ => {
                    self.instr_phase = InstrPhase::Complete;
                }
            }
        } else {
            // Reading: accumulate bytes and advance
            let byte_read = self.data as u8;
            match self.movem_long_phase {
                0 => {
                    // Read byte 3 (MSB), read byte 2
                    self.addr2 = u32::from(byte_read) << 24;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                1 => {
                    // Read byte 2, read byte 1
                    self.addr2 |= u32::from(byte_read) << 16;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 2;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                2 => {
                    // Read byte 1, read byte 0
                    self.addr2 |= u32::from(byte_read) << 8;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 3;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                3 => {
                    // Read byte 0 (LSB), store to register
                    self.addr2 |= u32::from(byte_read);
                    self.regs.d[data_reg] = self.addr2;
                    self.instr_phase = InstrPhase::Complete;
                }
                4 => {
                    // Word: read high byte, read low byte
                    self.addr2 = u32::from(byte_read) << 8;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 5;
                    self.micro_ops.push(MicroOp::ReadByte);
                    self.micro_ops.push(MicroOp::Execute);
                }
                5 => {
                    // Word: read low byte, store to register (low word only)
                    self.addr2 |= u32::from(byte_read);
                    // MOVEP.W only affects low word of Dn
                    self.regs.d[data_reg] = (self.regs.d[data_reg] & 0xFFFF_0000) | self.addr2;
                    self.instr_phase = InstrPhase::Complete;
                }
                _ => {
                    self.instr_phase = InstrPhase::Complete;
                }
            }
        }
    }

    /// MOVEP continuation dispatcher.
    fn movep_continuation(&mut self) {
        use crate::cpu::InstrPhase;
        match self.instr_phase {
            InstrPhase::SrcRead => {
                // Just fetched the displacement - set up the actual transfers
                self.exec_movep_continuation();
            }
            InstrPhase::DstWrite => {
                // In the middle of read/write phases
                self.exec_movep_phase();
            }
            _ => {
                // Complete
                self.instr_phase = InstrPhase::Complete;
            }
        }
    }

    fn exec_moveq(&mut self, reg: u8, data: i8) {
        let value = data as i32 as u32; // Sign extend to 32 bits
        self.regs.d[reg as usize] = value;
        self.set_flags_move(value, Size::Long);
        self.queue_internal(4);
    }

    fn exec_lea(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // LEA calculates effective address and loads it into An
        // LEA does NOT access memory - it only calculates the address
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // LEA (An),Am - just copy address register
                    self.regs.set_a(reg as usize, self.regs.a(r as usize));
                    self.queue_internal(4);
                }
                AddrMode::AddrIndDisp(r) => {
                    // LEA d16(An),Am - need extension word
                    // Store info for continuation
                    self.addr = self.regs.a(r as usize);
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    // After fetch, calculate and store
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::AddrIndIndex(r) => {
                    // LEA d8(An,Xn),Am
                    self.addr = self.regs.a(r as usize);
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::AbsShort => {
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::AbsLong => {
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::PcDisp => {
                    // For PC-relative modes, base is the address of the extension word.
                    self.addr = self.regs.pc;
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                AddrMode::PcIndex => {
                    // For PC-relative modes, base is the address of the extension word.
                    self.addr = self.regs.pc;
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_lea_continuation(&mut self) {
        // Called after extension words are fetched for LEA
        let Some(src_mode) = self.src_mode else {
            return;
        };
        let dest_reg = self.addr2 as usize;

        let addr = match src_mode {
            AddrMode::AddrIndDisp(_) => {
                let disp = self.next_ext_word() as i16 as i32;
                (self.addr as i32).wrapping_add(disp) as u32
            }
            AddrMode::AddrIndIndex(_) => {
                let ext = self.next_ext_word();
                let disp = (ext & 0xFF) as i8 as i32;
                let xn = ((ext >> 12) & 7) as usize;
                let is_addr = ext & 0x8000 != 0;
                let is_long = ext & 0x0800 != 0;
                let idx_val = if is_addr {
                    self.regs.a(xn)
                } else {
                    self.regs.d[xn]
                };
                let idx_val = if is_long {
                    idx_val as i32
                } else {
                    idx_val as i16 as i32
                };
                (self.addr as i32)
                    .wrapping_add(disp)
                    .wrapping_add(idx_val) as u32
            }
            AddrMode::AbsShort => self.next_ext_word() as i16 as i32 as u32,
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                (u32::from(hi) << 16) | u32::from(lo)
            }
            AddrMode::PcDisp => {
                let disp = self.next_ext_word() as i16 as i32;
                (self.addr as i32).wrapping_add(disp) as u32
            }
            AddrMode::PcIndex => {
                let ext = self.next_ext_word();
                let disp = (ext & 0xFF) as i8 as i32;
                let xn = ((ext >> 12) & 7) as usize;
                let is_addr = ext & 0x8000 != 0;
                let is_long = ext & 0x0800 != 0;
                let idx_val = if is_addr {
                    self.regs.a(xn)
                } else {
                    self.regs.d[xn]
                };
                let idx_val = if is_long {
                    idx_val as i32
                } else {
                    idx_val as i16 as i32
                };
                (self.addr as i32)
                    .wrapping_add(disp)
                    .wrapping_add(idx_val) as u32
            }
            _ => return,
        };

        self.regs.set_a(dest_reg, addr);
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal(8);
    }

    fn exec_clr(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let continued = self.instr_phase != InstrPhase::Initial;
        let addr_mode = AddrMode::decode(mode, ea_reg);
        if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
            let op_pc = self.instr_start_pc.wrapping_sub(2);
            let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
            if log_this {
                eprintln!(
                    "[CPU] exec_add decode_result op_pc=${op_pc:08X} addr_mode={addr_mode:?} continued={continued}"
                );
            }
        }
        if let Some(addr_mode) = addr_mode {
            if !addr_mode.is_data_alterable() {
                self.illegal_instruction();
                return;
            }

            match addr_mode {
                AddrMode::DataReg(r) => {
                    self.regs.d[r as usize] = match size {
                        Size::Byte => self.regs.d[r as usize] & 0xFFFF_FF00,
                        Size::Word => self.regs.d[r as usize] & 0xFFFF_0000,
                        Size::Long => 0,
                    };
                    // CLR sets N=0, Z=1, V=0, C=0
                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_byte(self.regs.sr, 0);
                    self.queue_internal(4);
                }
                _ => {
                    if !continued {
                        let ext_count = self.ext_words_for_mode(addr_mode);
                        if ext_count > 0 {
                            for _ in 0..ext_count {
                                self.micro_ops.push(MicroOp::FetchExtWord);
                            }
                            self.size = size;
                            self.src_mode = Some(addr_mode);
                            self.instr_phase = InstrPhase::SrcEACalc;
                            self.micro_ops.push(MicroOp::Execute);
                            return;
                        }
                    }

                    let mode = if continued {
                        self.src_mode.unwrap_or(addr_mode)
                    } else {
                        addr_mode
                    };

                    // Memory destination - read-modify-write
                    // The 68000 reads from the address before writing (even
                    // though the read value is discarded). This affects timing.
                    // Flags are set in the RMW handler (after the read phase)
                    // to avoid modifying flags if an address error occurs.
                    self.size = size;
                    self.src_mode = Some(mode);
                    let pc_at_ext =
                        self.regs.pc.wrapping_sub(2 * u32::from(self.ext_count));
                    let (addr, _is_reg) = self.calc_ea(mode, pc_at_ext);
                    self.addr = addr;
                    self.data = 0;
                    self.data2 = 9; // 9 = CLR
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                    if continued {
                        self.instr_phase = InstrPhase::Complete;
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_nop(&mut self) {
        self.queue_internal(4);
    }

    fn exec_rts(&mut self) {
        // RTS = 16 cycles: 8 (pop) + 8 (prefetch at return address)
        // Pop return address from stack into self.data
        self.micro_ops.push(MicroOp::PopLongHi);
        self.micro_ops.push(MicroOp::PopLongLo);
        // Continue to set PC from popped value
        self.instr_phase = InstrPhase::SrcRead;
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_rts_continuation(&mut self) {
        // After pop, data contains the return address
        // Check for odd return address (triggers address error)
        if self.data & 1 != 0 {
            self.trigger_rts_address_error(self.data);
            return;
        }
        self.trace_jump_to("RTS", self.data);
        if std::env::var("EMU68000_TRACE_RTS").is_ok() {
            let op_pc = self.instr_start_pc.wrapping_sub(2);
            eprintln!(
                "[CPU] RTS @ ${op_pc:08X} -> ${:08X}",
                self.data
            );
        }
        if let Ok(spec) = std::env::var("EMU68000_TRACE_RTS_TO") {
            if let Ok(target) = u32::from_str_radix(spec.trim_start_matches("0x").trim_start_matches("0X"), 16) {
                if self.data == target {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    let sp_after = self.regs.a(7);
                    let ret_addr_loc = sp_after.wrapping_sub(4);
                    eprintln!(
                        "[CPU] RTS_TO hit @ ${op_pc:08X} -> ${:08X} (sp_after=${sp_after:08X} ret_addr_loc=${ret_addr_loc:08X})",
                        self.data
                    );
                }
            }
        }
        // PC should return to the exact address popped.
        self.regs.pc = self.data;
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal_no_pc(8); // Prefetch cycles
    }

    pub(super) fn trigger_rts_address_error(&mut self, addr: u32) {
        // RTS to odd address triggers address error.
        // Error detected during address calculation, before any bus cycle,
        // so I/N (fault_in_instruction) is 0.
        self.fault_fc = if self.regs.sr & crate::flags::S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    fn exec_bra(&mut self, displacement: i8) {
        if displacement == 0 {
            // Word displacement follows - need continuation after fetch
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.instr_phase = InstrPhase::SrcEACalc; // Signal word branch
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // Byte displacement
            // Displacement is relative to PC after fetching opcode.
            let offset = displacement as i32;
            let pc_for_branch = self.regs.pc;
            let target = (pc_for_branch as i32).wrapping_add(offset) as u32;
            // Check for odd target address
            if target & 1 != 0 {
                self.trigger_branch_address_error(target);
                return;
            }
            // Set PC to branch target (prefetch handled via set_jump_pc)
            self.set_jump_pc(target);
            self.queue_internal(4);
        }
    }

    fn exec_bsr(&mut self, displacement: i8) {
        // BSR = 18 cycles: 8 (push) + 2 (internal) + 8 (prefetch at target)
        // With prefetch model: PC already points past opcode.
        // Displacement is relative to PC after consuming any extension word.
        // Return address is the instruction after BSR.
        // After BSR, PC should be target + 4 for prefetch.
        if displacement == 0 {
            // Word displacement - BSR.W is 4 bytes (opcode + ext word)
            self.micro_ops.push(MicroOp::FetchExtWord);
            self.instr_phase = InstrPhase::SrcRead; // Signal BSR.W continuation
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // Byte displacement - BSR.B is 2 bytes (opcode only)
            // Displacement is relative to PC after fetching opcode.
            let pc_base = self.regs.pc as i32;
            let target = pc_base.wrapping_add(displacement as i32) as u32;

            // Return address: instruction after BSR = opcode + 2 = PC - 2
            self.data = self.regs.pc;

            // Check for odd target address
            // BSR pushes return address BEFORE detecting odd target
            if target & 1 != 0 {
                // BSR actually writes the return address to stack before address error
                // Queue the push operations - they will handle SP decrement
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                // Store target for address error
                self.addr2 = target;
                // Set flag to trigger address error after push
                self.data2 = 0x8000_0001;
                self.micro_ops.push(MicroOp::Internal);
                self.internal_cycles = 0;
                return;
            }

            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            // Set PC to branch target (prefetch handled via set_jump_pc)
            self.set_jump_pc(target);
            self.internal_cycles = 10; // 2 (internal) + 8 (prefetch)
            self.micro_ops.push(MicroOp::Internal);
        }
    }

    pub(super) fn trigger_branch_address_error(&mut self, addr: u32) {
        // Branch to odd address triggers address error.
        // The error is detected during address calculation, before any bus cycle,
        // so I/N (fault_in_instruction) is 0, not 1.
        // PC in exception frame: for BRA/Bcc, the standard calculation gives the
        // correct value (instruction PC). BSR has special handling.
        self.fault_fc = if self.regs.sr & crate::flags::S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    fn exec_bcc(&mut self, condition: u8, displacement: i8) {
        if Status::condition(self.regs.sr, condition) {
            if displacement == 0 {
                // Word displacement - fetch and continue
                // Store condition in data2 for continuation
                self.data2 = u32::from(condition) | 0x100; // Mark as conditional, taken
                self.micro_ops.push(MicroOp::FetchExtWord);
                self.instr_phase = InstrPhase::SrcEACalc;
                self.micro_ops.push(MicroOp::Execute);
            } else {
                // Byte displacement - compute target and check for odd address
                // Displacement is relative to PC after fetching opcode.
                let offset = displacement as i32;
                let pc_for_branch = self.regs.pc;
                let target = (pc_for_branch as i32).wrapping_add(offset) as u32;
                if target & 1 != 0 {
                    self.trigger_branch_address_error(target);
                    return;
                }
                // Set PC to branch target (prefetch handled via set_jump_pc)
                self.set_jump_pc(target);
                self.queue_internal(10); // Branch taken timing
            }
        } else {
            if displacement == 0 {
                // Skip word displacement
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            self.queue_internal(8); // Branch not taken timing
        }
    }

    /// Continuation for word displacement branches (BRA.W, BSR.W, Bcc.W).
    fn branch_continuation(&mut self) {
        // Extension word was fetched, apply word displacement
        let disp = self.ext_words[0] as i16 as i32;

        match self.instr_phase {
            InstrPhase::SrcEACalc => {
                // BRA.W or Bcc.W (taken)
                // PC already advanced past extension word; displacement is relative to
                // the extension word, so base is PC-2.
                let target = ((self.regs.pc as i32) - 2 + disp) as u32;
                // Check for odd target address
                if target & 1 != 0 {
                    self.trigger_branch_address_error(target);
                    return;
                }
                // Set PC to the actual branch target (prefetch handled via set_jump_pc)
                self.set_jump_pc(target);
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal(10); // advances PC by +2
            }
            InstrPhase::SrcRead => {
                // BSR.W - push return address (after ext word) then branch
                let target = ((self.regs.pc as i32) - 2 + disp) as u32;
                // Check for odd target address - BSR pushes first, then checks
                if target & 1 != 0 {
                    // Decrement stack pointer by 4 (for the return address that would be pushed)
                    let sp = self.regs.active_sp().wrapping_sub(4);
                    self.regs.set_active_sp(sp);
                    self.trigger_branch_address_error(target);
                    return;
                }
                // Return address is current PC (after ext word)
                self.data = self.regs.pc;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                // Branch: set PC to the actual target (prefetch handled via set_jump_pc)
                self.set_jump_pc(target);
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal(4); // advances PC by +2
            }
            _ => {
                self.instr_phase = InstrPhase::Initial;
            }
        }
    }

    // Bit operation implementations
    fn exec_btst_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BTST Dn,<ea> - test bit, set Z if bit was 0
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let value = self.regs.d[r as usize];
                    let was_zero = (value >> bit) & 1 == 0;
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(6);
                }
                AddrMode::Immediate => {
                    // BTST Dn,#imm - test bit in immediate byte
                    let value = (self.next_ext_word() & 0xFF) as u32;
                    let bit = (bit_num % 8) as u8;
                    let was_zero = (value >> bit) & 1 == 0;
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(10);
                }
                _ => {
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 0; // 0 = BTST
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_bchg_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BCHG Dn,<ea> - test and change (toggle) bit
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let mask = 1u32 << bit;
                    let value = self.regs.d[r as usize];
                    let was_zero = value & mask == 0;
                    self.regs.d[r as usize] = value ^ mask; // Toggle bit
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(6);
                }
                _ => {
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 1; // 1 = BCHG
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_bclr_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BCLR Dn,<ea> - test and clear bit
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let mask = 1u32 << bit;
                    let value = self.regs.d[r as usize];
                    let was_zero = value & mask == 0;
                    self.regs.d[r as usize] = value & !mask; // Clear bit
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(8);
                }
                _ => {
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 2; // 2 = BCLR
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_bset_reg(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // BSET Dn,<ea> - test and set bit
        let bit_num = self.regs.d[reg as usize];

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let bit = (bit_num % 32) as u8;
                    let mask = 1u32 << bit;
                    let value = self.regs.d[r as usize];
                    let was_zero = value & mask == 0;
                    self.regs.d[r as usize] = value | mask; // Set bit
                    self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                    self.queue_internal(6);
                }
                _ => {
                    // Memory operand - bit number mod 8
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = bit_num & 7; // mod 8 for memory
                    self.data2 = 3; // 3 = BSET
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_btst_imm(&mut self, mode: u8, ea_reg: u8) {
        // BTST #imm,<ea> - test bit (immediate bit number)
        let Some(dst_mode) = AddrMode::decode(mode, ea_reg) else {
            self.illegal_instruction();
            return;
        };
        if matches!(dst_mode, AddrMode::Immediate) {
            self.illegal_instruction();
            return;
        }
        // Fetch bit number then EA extension words
        self.micro_ops.push(MicroOp::FetchExtWord);
        let ea_count = self.ext_words_for_mode(dst_mode);
        for _ in 0..ea_count {
            self.micro_ops.push(MicroOp::FetchExtWord);
        }
        self.dst_mode = Some(dst_mode);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 0; // Mark as BTST (0)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bchg_imm(&mut self, mode: u8, ea_reg: u8) {
        // BCHG #imm,<ea> - test and change bit
        let Some(dst_mode) = AddrMode::decode(mode, ea_reg) else {
            self.illegal_instruction();
            return;
        };
        if matches!(dst_mode, AddrMode::Immediate) {
            self.illegal_instruction();
            return;
        }
        self.micro_ops.push(MicroOp::FetchExtWord);
        let ea_count = self.ext_words_for_mode(dst_mode);
        for _ in 0..ea_count {
            self.micro_ops.push(MicroOp::FetchExtWord);
        }
        self.dst_mode = Some(dst_mode);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 1; // Mark as BCHG (1)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bclr_imm(&mut self, mode: u8, ea_reg: u8) {
        // BCLR #imm,<ea> - test and clear bit
        let Some(dst_mode) = AddrMode::decode(mode, ea_reg) else {
            self.illegal_instruction();
            return;
        };
        if matches!(dst_mode, AddrMode::Immediate) {
            self.illegal_instruction();
            return;
        }
        self.micro_ops.push(MicroOp::FetchExtWord);
        let ea_count = self.ext_words_for_mode(dst_mode);
        for _ in 0..ea_count {
            self.micro_ops.push(MicroOp::FetchExtWord);
        }
        self.dst_mode = Some(dst_mode);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 2; // Mark as BCLR (2)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bset_imm(&mut self, mode: u8, ea_reg: u8) {
        // BSET #imm,<ea> - test and set bit
        let Some(dst_mode) = AddrMode::decode(mode, ea_reg) else {
            self.illegal_instruction();
            return;
        };
        if matches!(dst_mode, AddrMode::Immediate) {
            self.illegal_instruction();
            return;
        }
        self.micro_ops.push(MicroOp::FetchExtWord);
        let ea_count = self.ext_words_for_mode(dst_mode);
        for _ in 0..ea_count {
            self.micro_ops.push(MicroOp::FetchExtWord);
        }
        self.dst_mode = Some(dst_mode);
        self.instr_phase = InstrPhase::SrcRead;
        self.data2 = 3; // Mark as BSET (3)
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_bit_imm_continuation(&mut self) {
        // Continuation for BTST/BCHG/BCLR/BSET with immediate bit number.
        // Use next_ext_word() to properly consume the extension word and advance PC.
        let bit_num = (self.next_ext_word() & 0xFF) as u32;
        let op_type = self.data2; // 0=BTST, 1=BCHG, 2=BCLR, 3=BSET

        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                // For data registers, bit number is mod 32
                let bit = (bit_num % 32) as u8;
                let value = self.regs.d[r as usize];
                let mask = 1u32 << bit;
                let was_zero = (value & mask) == 0;

                // Set Z flag based on original bit
                self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, was_zero);
                if std::env::var("EMU68000_TRACE_BTST_LEGACY").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    if op_pc == 0x00FC_2246 || op_pc == 0x00FC_2282 {
                        use std::sync::atomic::{AtomicUsize, Ordering};
                        static COUNT: AtomicUsize = AtomicUsize::new(0);
                        let count = COUNT.fetch_add(1, Ordering::Relaxed);
                        if !was_zero || count < 20 {
                            eprintln!(
                                "[CPU] LEG BTST imm pc=${op_pc:08X} bit={} value=${:08X} mask=${:08X} was_zero={} sr=${:04X}",
                                bit,
                                value,
                                mask,
                                was_zero,
                                self.regs.sr
                            );
                        }
                    }
                }
                if op_type == 0 && std::env::var("EMU68000_TRACE_BTST").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    if op_pc == 0x00FC_2246 {
                        use std::sync::atomic::{AtomicUsize, Ordering};
                        static COUNT: AtomicUsize = AtomicUsize::new(0);
                        let count = COUNT.fetch_add(1, Ordering::Relaxed);
                        if !was_zero || count < 20 {
                            eprintln!(
                                "[CPU] BTST imm bit={} value=${:08X} mask=${:08X} was_zero={} sr=${:04X}",
                                bit,
                                value,
                                mask,
                                was_zero,
                                self.regs.sr
                            );
                        }
                    }
                }

                // Modify bit if not BTST
                match op_type {
                    0 => {} // BTST - test only
                    1 => self.regs.d[r as usize] ^= mask,  // BCHG - toggle
                    2 => self.regs.d[r as usize] &= !mask, // BCLR - clear
                    3 => self.regs.d[r as usize] |= mask,  // BSET - set
                    _ => {}
                }

                // Timing for #n,Dn: BTST=10, BCHG=10, BCLR=12, BSET=10
                self.queue_internal(if op_type == 2 { 12 } else { 10 });
            }
            _ => {
                // Memory operand - bit number is mod 8, always byte-sized
                self.size = Size::Byte;
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.src_mode = Some(dst_mode);
                self.data = bit_num & 7; // mod 8 for memory
                self.data2 = op_type;
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::BitMemOp);
                self.instr_phase = InstrPhase::Complete;
                return;
            }
        }
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_ori(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // ORI #imm,<ea> - Inclusive OR Immediate
        // Special case: mode=7, reg=4 means CCR (byte) or SR (word)
        if mode == 7 && ea_reg == 4 {
            match size {
                Some(Size::Byte) => {
                    // ORI #xx,CCR - 20 cycles
                    // Get immediate from prefetch queue (only low byte used)
                    let imm = (self.next_ext_word() & 0x1F) as u8; // Mask to valid CCR bits
                    let ccr = self.regs.ccr() & 0x1F; // CCR bits 5-7 always read as 0
                    self.regs.set_ccr(ccr | imm);
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.queue_internal_no_pc(20);
                }
                Some(Size::Word) => {
                    // ORI #xx,SR - privileged, 20 cycles
                    if !self.regs.is_supervisor() {
                        self.exception(8); // Privilege violation
                        return;
                    }
                    let imm = self.next_ext_word();
                    // Apply OR then mask to valid SR bits (68000 has reserved bits)
                    let new_sr = (self.regs.sr | imm) & crate::flags::SR_MASK;
                    self.trace_sr_update("ORI->SR", new_sr);
                    self.regs.sr = new_sr;
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.queue_internal_no_pc(20);
                }
                _ => self.illegal_instruction(),
            }
            return;
        }

        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if matches!(addr_mode, AddrMode::Immediate) {
                self.illegal_instruction();
                return;
            }
            self.size = size;
            // Queue immediate value and EA extension words
            let imm_count = if size == Size::Long { 2 } else { 1 };
            let ea_count = self.ext_words_for_mode(addr_mode);
            for _ in 0..imm_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            for _ in 0..ea_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_ori_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Get immediate value using next_ext_word() to properly track consumption
        let imm = if self.size == Size::Long {
            let hi = self.next_ext_word();
            let lo = self.next_ext_word();
            (u32::from(hi) << 16) | u32::from(lo)
        } else {
            let word = self.next_ext_word();
            if self.size == Size::Word {
                u32::from(word)
            } else {
                u32::from(word & 0xFF)
            }
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst | imm;
                self.write_data_reg(r, result, self.size);
                self.set_flags_move(result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.src_mode = Some(AddrMode::Immediate);
                self.data = imm; // Immediate as source
                self.data2 = 3;  // 3 = OR
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
                // Don't set Complete yet - AluMemRmw will handle it
            }
        }
    }

    fn exec_andi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // ANDI #imm,<ea> - AND Immediate
        // Special case: mode=7, reg=4 means CCR (byte) or SR (word)
        if mode == 7 && ea_reg == 4 {
            match size {
                Some(Size::Byte) => {
                    // ANDI #xx,CCR - 20 cycles
                    // Get immediate from prefetch queue (low byte, masked to CCR bits)
                    let imm = (self.next_ext_word() & 0x1F) as u8;
                    let ccr = self.regs.ccr() & 0x1F;
                    self.regs.set_ccr(ccr & imm);
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.queue_internal_no_pc(20);
                }
                Some(Size::Word) => {
                    // ANDI #xx,SR - privileged, 20 cycles
                    if !self.regs.is_supervisor() {
                        self.exception(8); // Privilege violation
                        return;
                    }
                    let imm = self.next_ext_word();
                    // Apply AND then mask to valid SR bits (68000 has reserved bits)
                    let new_sr = (self.regs.sr & imm) & crate::flags::SR_MASK;
                    self.trace_sr_update("ANDI->SR", new_sr);
                    self.regs.sr = new_sr;
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.queue_internal_no_pc(20);
                }
                _ => self.illegal_instruction(),
            }
            return;
        }

        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if matches!(addr_mode, AddrMode::Immediate) {
                self.illegal_instruction();
                return;
            }
            self.size = size;
            let imm_count = if size == Size::Long { 2 } else { 1 };
            let ea_count = self.ext_words_for_mode(addr_mode);
            for _ in 0..imm_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            for _ in 0..ea_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_andi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Get immediate value using next_ext_word() to properly track consumption
        let imm = if self.size == Size::Long {
            let hi = self.next_ext_word();
            let lo = self.next_ext_word();
            (u32::from(hi) << 16) | u32::from(lo)
        } else {
            let word = self.next_ext_word();
            if self.size == Size::Word {
                u32::from(word)
            } else {
                u32::from(word & 0xFF)
            }
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst & imm;
                self.write_data_reg(r, result, self.size);
                self.set_flags_move(result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.src_mode = Some(AddrMode::Immediate);
                self.data = imm; // Immediate as source
                self.data2 = 2;  // 2 = AND
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
    }

    fn exec_subi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // SUBI #imm,<ea> - Subtract Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if matches!(addr_mode, AddrMode::Immediate) {
                self.illegal_instruction();
                return;
            }
            self.size = size;
            let imm_count = if size == Size::Long { 2 } else { 1 };
            let ea_count = self.ext_words_for_mode(addr_mode);
            for _ in 0..imm_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            for _ in 0..ea_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_subi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Get immediate value using next_ext_word() to properly track consumption
        let src = if self.size == Size::Long {
            let hi = self.next_ext_word();
            let lo = self.next_ext_word();
            (u32::from(hi) << 16) | u32::from(lo)
        } else {
            let word = self.next_ext_word();
            if self.size == Size::Word {
                u32::from(word)
            } else {
                u32::from(word & 0xFF)
            }
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst.wrapping_sub(src);
                self.write_data_reg(r, result, self.size);
                self.set_flags_sub(src, dst, result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.src_mode = Some(AddrMode::Immediate);
                self.data = src; // Immediate as source
                self.data2 = 1;  // 1 = SUB
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
    }

    fn exec_addi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // ADDI #imm,<ea> - Add Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if matches!(addr_mode, AddrMode::Immediate) {
                self.illegal_instruction();
                return;
            }
            self.size = size;
            let imm_count = if size == Size::Long { 2 } else { 1 };
            let ea_count = self.ext_words_for_mode(addr_mode);
            for _ in 0..imm_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            for _ in 0..ea_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_addi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Get immediate value using next_ext_word() to properly track consumption
        let src = if self.size == Size::Long {
            let hi = self.next_ext_word();
            let lo = self.next_ext_word();
            (u32::from(hi) << 16) | u32::from(lo)
        } else {
            let word = self.next_ext_word();
            if self.size == Size::Word {
                u32::from(word)
            } else {
                u32::from(word & 0xFF)
            }
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst.wrapping_add(src);
                self.write_data_reg(r, result, self.size);
                self.set_flags_add(src, dst, result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.src_mode = Some(AddrMode::Immediate);
                self.data = src; // Immediate as source
                self.data2 = 0;  // 0 = ADD
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
    }

    fn exec_eori(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // EORI #imm,<ea> - Exclusive OR Immediate
        // Special case: mode=7, reg=4 means CCR (byte) or SR (word)
        if mode == 7 && ea_reg == 4 {
            match size {
                Some(Size::Byte) => {
                    // EORI #xx,CCR - 20 cycles
                    // Get immediate from prefetch queue (low byte, masked to CCR bits)
                    let imm = (self.next_ext_word() & 0x1F) as u8;
                    let ccr = self.regs.ccr() & 0x1F;
                    self.regs.set_ccr(ccr ^ imm);
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.queue_internal_no_pc(20);
                }
                Some(Size::Word) => {
                    // EORI #xx,SR - privileged, 20 cycles
                    if !self.regs.is_supervisor() {
                        self.exception(8); // Privilege violation
                        return;
                    }
                    let imm = self.next_ext_word();
                    // Apply XOR then mask to valid SR bits (68000 has reserved bits)
                    let new_sr = (self.regs.sr ^ imm) & crate::flags::SR_MASK;
                    self.trace_sr_update("EORI->SR", new_sr);
                    self.regs.sr = new_sr;
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.queue_internal_no_pc(20);
                }
                _ => self.illegal_instruction(),
            }
            return;
        }

        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if matches!(addr_mode, AddrMode::Immediate) {
                self.illegal_instruction();
                return;
            }
            self.size = size;
            let imm_count = if size == Size::Long { 2 } else { 1 };
            let ea_count = self.ext_words_for_mode(addr_mode);
            for _ in 0..imm_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            for _ in 0..ea_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_eori_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Get immediate value using next_ext_word() to properly track consumption
        let imm = if self.size == Size::Long {
            let hi = self.next_ext_word();
            let lo = self.next_ext_word();
            (u32::from(hi) << 16) | u32::from(lo)
        } else {
            let word = self.next_ext_word();
            if self.size == Size::Word {
                u32::from(word)
            } else {
                u32::from(word & 0xFF)
            }
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst ^ imm;
                self.write_data_reg(r, result, self.size);
                self.set_flags_move(result, self.size);
                self.queue_internal(if self.size == Size::Long { 16 } else { 8 });
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read-modify-write
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.src_mode = Some(AddrMode::Immediate);
                self.data = imm; // Immediate as source
                self.data2 = 4;  // 4 = EOR
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemRmw);
            }
        }
    }

    fn exec_cmpi(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // CMPI #imm,<ea> - Compare Immediate
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if matches!(addr_mode, AddrMode::Immediate) {
                self.illegal_instruction();
                return;
            }
            self.size = size;
            let imm_count = if size == Size::Long { 2 } else { 1 };
            let ea_count = self.ext_words_for_mode(addr_mode);
            for _ in 0..imm_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            for _ in 0..ea_count {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            self.dst_mode = Some(addr_mode);
            self.instr_phase = InstrPhase::DstEACalc;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_cmpi_continuation(&mut self) {
        let Some(dst_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let trace_cmpi_pc = std::env::var("EMU68000_TRACE_CMPI_PC")
            .ok()
            .and_then(|spec| {
                let hex = spec.trim().trim_start_matches("0x").trim_start_matches("0X");
                u32::from_str_radix(hex, 16).ok()
            });

        // Get immediate value using next_ext_word() to properly track consumption
        let src = if self.size == Size::Long {
            let hi = self.next_ext_word();
            let lo = self.next_ext_word();
            (u32::from(hi) << 16) | u32::from(lo)
        } else {
            let word = self.next_ext_word();
            if self.size == Size::Word {
                u32::from(word)
            } else {
                u32::from(word & 0xFF)
            }
        };

        match dst_mode {
            AddrMode::DataReg(r) => {
                let dst = self.read_data_reg(r, self.size);
                let result = dst.wrapping_sub(src);
                // CMP only sets flags, doesn't store result
                self.set_flags_cmp(src, dst, result, self.size);
                if trace_cmpi_pc == Some(self.instr_start_pc.wrapping_sub(2)) {
                    eprintln!(
                        "[CPU] CMPI pc=${:08X} size={:?} src=${:08X} dst=${:08X} result=${:08X} SR=${:04X}",
                        self.instr_start_pc.wrapping_sub(2),
                        self.size,
                        src,
                        dst,
                        result,
                        self.regs.sr
                    );
                }
                self.queue_internal(if self.size == Size::Long { 14 } else { 8 });
                self.instr_phase = InstrPhase::Complete;
            }
            _ => {
                // Memory destination - read and compare
                let (addr, _is_reg) = self.calc_ea(dst_mode, self.regs.pc);
                self.addr = addr;
                self.src_mode = Some(AddrMode::Immediate);
                self.data = src;  // Immediate as source
                self.data2 = 14;  // 14 = CMPI
                self.movem_long_phase = 0;
                self.micro_ops.push(MicroOp::AluMemSrc);
            }
        }
    }

    /// Dispatch continuation for group 0 immediate operations.
    fn immediate_op_continuation(&mut self) {
        let op = self.opcode;
        // Immediate ops: top 3 bits of bits 11-9 determine operation
        match (op >> 9) & 7 {
            0 => self.exec_ori_continuation(),
            1 => self.exec_andi_continuation(),
            2 => self.exec_subi_continuation(),
            3 => self.exec_addi_continuation(),
            4 => self.exec_bit_imm_continuation(), // Static bit operations
            5 => self.exec_eori_continuation(),
            6 => self.exec_cmpi_continuation(),
            _ => {
                self.instr_phase = InstrPhase::Initial;
            }
        }
    }

    fn exec_move_from_sr(&mut self, mode: u8, ea_reg: u8) {
        // MOVE SR,<ea> - Copy status register to destination
        // On 68000, this is NOT privileged (unlike 68010+)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Store SR in low word of Dn
                    let sr = u32::from(self.regs.sr);
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_0000) | sr;
                    self.queue_internal(6);
                }
                _ => {
                    // Memory destination - read-modify-write
                    // The 68000 reads from the address before writing (even
                    // though the read value is discarded). This affects timing.
                    self.src_mode = Some(addr_mode);
                    self.size = Size::Word;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(self.regs.sr);
                    self.data2 = 10; // 10 = MOVEfromSR
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_negx(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // NEGX <ea> - Negate with extend (0 - dst - X)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    let x = u32::from(self.regs.sr & X != 0);
                    let result = 0u32.wrapping_sub(value).wrapping_sub(x);
                    self.write_data_reg(r, result, size);

                    // NEGX flags: like NEG but X affects result and Z is only cleared, never set
                    let (src_masked, result_masked, msb) = match size {
                        Size::Byte => (value & 0xFF, result & 0xFF, 0x80u32),
                        Size::Word => (value & 0xFFFF, result & 0xFFFF, 0x8000),
                        Size::Long => (value, result, 0x8000_0000),
                    };

                    let mut sr = self.regs.sr;
                    // N: set if result is negative
                    sr = Status::set_if(sr, N, result_masked & msb != 0);
                    // Z: cleared if result is non-zero, unchanged otherwise
                    if result_masked != 0 {
                        sr &= !Z;
                    }
                    // V: set if overflow
                    let overflow = (src_masked & msb) != 0 && (result_masked & msb) != 0;
                    sr = Status::set_if(sr, V, overflow);
                    // C and X: set if borrow
                    let carry = src_masked != 0 || x != 0;
                    sr = Status::set_if(sr, C, carry);
                    sr = Status::set_if(sr, X, carry);

                    self.regs.sr = sr;
                    self.queue_internal(if size == Size::Long { 6 } else { 4 });
                }
                _ => {
                    // Memory operand - use AluMemRmw
                    self.src_mode = Some(addr_mode);
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 7; // 7 = NEGX
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_ccr(&mut self, mode: u8, ea_reg: u8) {
        // MOVE <ea>,CCR - Copy source to condition code register (low byte of SR)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Only low 5 bits are CCR (XNZVC)
                    let ccr = (self.regs.d[r as usize] & 0x1F) as u16;
                    self.regs.sr = (self.regs.sr & 0xFF00) | ccr;
                    self.queue_internal(12);
                }
                AddrMode::Immediate => {
                    // MOVE #imm,CCR - need to fetch immediate
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::Immediate);
                    self.micro_ops.push(MicroOp::Execute);
                }
                _ => {
                    // Memory source - read word and apply to CCR
                    self.size = Size::Word;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::ReadWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::DataReg(0)); // Marker for memory source
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_ccr_continuation(&mut self) {
        // Continuation for MOVE <ea>,CCR
        // For immediate: consume extension word via next_ext_word() to advance PC
        // For memory: data is in self.data (from ReadWord)
        let src = if self.dst_mode == Some(AddrMode::Immediate) {
            self.next_ext_word()
        } else {
            self.data as u16
        };
        let ccr = src & 0x1F;
        self.regs.sr = (self.regs.sr & 0xFF00) | ccr;
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal(12);
    }

    fn exec_neg(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // NEG <ea> - negate (0 - destination)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    let result = 0u32.wrapping_sub(value);
                    self.write_data_reg(r, result, size);
                    // NEG flags: same as SUB 0 - src -> 0
                    self.set_flags_sub(value, 0, result, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory operand - read-modify-write
                    self.src_mode = Some(addr_mode);
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 5; // 5 = NEG
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_sr(&mut self, mode: u8, ea_reg: u8) {
        // MOVE <ea>,SR - Copy source to status register (privileged)
        if !self.regs.is_supervisor() {
            self.exception(8); // Privilege violation
            return;
        }

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Mask to valid SR bits (68000 has reserved bits)
                    let new_sr = (self.regs.d[r as usize] as u16) & crate::flags::SR_MASK;
                    self.trace_sr_update("MOVE->SR", new_sr);
                    self.regs.sr = new_sr;
                    self.queue_internal(12);
                }
                AddrMode::Immediate => {
                    // MOVE #imm,SR - need to fetch immediate
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::Immediate);
                    self.micro_ops.push(MicroOp::Execute);
                }
                _ => {
                    // Memory source - read word and apply to SR
                    self.size = Size::Word;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::ReadWord);
                    self.instr_phase = InstrPhase::SrcRead;
                    self.dst_mode = Some(AddrMode::DataReg(0)); // Marker for memory source
                    self.micro_ops.push(MicroOp::Execute);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_move_to_sr_continuation(&mut self) {
        // Continuation for MOVE <ea>,SR
        // For immediate: consume extension word via next_ext_word() to advance PC
        // For memory: data is in self.data (from ReadWord)
        let src = if self.dst_mode == Some(AddrMode::Immediate) {
            self.next_ext_word()
        } else {
            self.data as u16
        };
        // Mask to valid SR bits (68000 has reserved bits)
        let new_sr = src & crate::flags::SR_MASK;
        self.trace_sr_update("MOVE->SR", new_sr);
        self.regs.sr = new_sr;
        self.instr_phase = InstrPhase::Complete;
        self.queue_internal(12);
    }

    fn exec_not(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // NOT <ea> - ones complement
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    let result = !value;
                    self.write_data_reg(r, result, size);
                    self.set_flags_move(result, size); // NOT sets N,Z, clears V,C
                    self.queue_internal(4);
                }
                _ => {
                    // Memory operand - read-modify-write
                    self.src_mode = Some(addr_mode);
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 6; // 6 = NOT
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_nbcd(&mut self, mode: u8, ea_reg: u8) {
        // NBCD - Negate Decimal with Extend
        // Computes 0 - <ea> - X (BCD negation)
        // Format: 0100 1000 00 mode reg (byte only)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.regs.d[r as usize] as u8;
                    let x = u8::from(self.regs.sr & X != 0);

                    let (result, borrow, overflow) = self.nbcd(src, x);

                    // Write result to low byte
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(result);

                    // Set flags
                    let mut sr = self.regs.sr;
                    // Z: cleared if non-zero, unchanged otherwise
                    if result != 0 {
                        sr &= !Z;
                    }
                    // C and X: set if decimal borrow
                    sr = Status::set_if(sr, C, borrow);
                    sr = Status::set_if(sr, X, borrow);
                    // N: undefined, but set based on MSB
                    sr = Status::set_if(sr, N, result & 0x80 != 0);
                    // V: set when BCD correction flips bit 7
                    sr = Status::set_if(sr, V, overflow);
                    self.regs.sr = sr;

                    self.queue_internal(6);
                }
                _ => {
                    // Memory operand - read-modify-write with BCD negate
                    self.src_mode = Some(addr_mode);
                    self.size = Size::Byte; // NBCD is always byte
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data2 = 8; // 8 = NBCD
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_swap(&mut self, reg: u8) {
        let value = self.regs.d[reg as usize];
        let swapped = (value >> 16) | (value << 16);
        self.regs.d[reg as usize] = swapped;
        self.set_flags_move(swapped, Size::Long);
        self.queue_internal(4);
    }

    fn exec_pea(&mut self, mode: u8, ea_reg: u8) {
        // PEA <ea> - Push Effective Address onto stack
        // Uses prefetched extension words. Computes EA and pushes it.
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let ext_count = self.ext_words_for_mode(addr_mode);
            if ext_count > 0 {
                if self.instr_phase == InstrPhase::Initial {
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.src_mode = Some(addr_mode);
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
                if self.instr_phase != InstrPhase::SrcEACalc {
                    self.instr_phase = InstrPhase::Complete;
                    return;
                }
            }
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // PEA (An) = 12 cycles: 4 (internal) + 8 (push)
                    self.data = self.regs.a(r as usize);
                    self.internal_cycles = 4;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                AddrMode::AddrIndDisp(r) => {
                    // PEA d16(An) = 16 cycles: 8 (internal) + 8 (push)
                    let disp = self.next_ext_word() as i16 as i32;
                    let ea = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                    self.data = ea;
                    self.internal_cycles = 8;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                AddrMode::AddrIndIndex(r) => {
                    // PEA d8(An,Xn) = 20 cycles: 12 (internal) + 8 (push)
                    let ea = self.calc_index_ea(self.regs.a(r as usize));
                    self.data = ea;
                    self.internal_cycles = 12;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                AddrMode::AbsShort => {
                    // PEA addr.W = 16 cycles: 8 (internal) + 8 (push)
                    let ea = self.next_ext_word() as i16 as i32 as u32;
                    self.data = ea;
                    self.internal_cycles = 8;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                AddrMode::AbsLong => {
                    // PEA addr.L = 20 cycles: 12 (internal) + 8 (push)
                    let hi = u32::from(self.next_ext_word());
                    let lo = u32::from(self.next_ext_word());
                    let ea = (hi << 16) | lo;
                    self.data = ea;
                    self.internal_cycles = 12;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                AddrMode::PcDisp => {
                    // PEA d16(PC) = 16 cycles: 8 (internal) + 8 (push)
                    // PC for calculation is where the extension word was (PC - 2)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let disp = self.next_ext_word() as i16 as i32;
                    let ea = (pc_at_ext as i32).wrapping_add(disp) as u32;
                    self.data = ea;
                    self.internal_cycles = 8;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                AddrMode::PcIndex => {
                    // PEA d8(PC,Xn) = 20 cycles: 12 (internal) + 8 (push)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let ea = self.calc_index_ea(pc_at_ext);
                    self.data = ea;
                    self.internal_cycles = 12;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                }
                _ => self.illegal_instruction(),
            }
            if ext_count > 0 {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_pea_continuation(&mut self) {
        let Some(addr_mode) = self.src_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let pc_at_ext = self
            .regs
            .pc
            .wrapping_sub(2 * u32::from(self.ext_count));

        match addr_mode {
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let ea = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                self.data = ea;
                self.internal_cycles = 8;
            }
            AddrMode::AddrIndIndex(r) => {
                let ea = self.calc_index_ea(self.regs.a(r as usize));
                self.data = ea;
                self.internal_cycles = 12;
            }
            AddrMode::AbsShort => {
                let ea = self.next_ext_word() as i16 as i32 as u32;
                self.data = ea;
                self.internal_cycles = 8;
            }
            AddrMode::AbsLong => {
                let hi = u32::from(self.next_ext_word());
                let lo = u32::from(self.next_ext_word());
                let ea = (hi << 16) | lo;
                self.data = ea;
                self.internal_cycles = 12;
            }
            AddrMode::PcDisp => {
                let disp = self.next_ext_word() as i16 as i32;
                let ea = (pc_at_ext as i32).wrapping_add(disp) as u32;
                self.data = ea;
                self.internal_cycles = 8;
            }
            AddrMode::PcIndex => {
                let ea = self.calc_index_ea(pc_at_ext);
                self.data = ea;
                self.internal_cycles = 12;
            }
            _ => {
                self.instr_phase = InstrPhase::Initial;
                return;
            }
        }

        self.micro_ops.push(MicroOp::Internal);
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
        self.instr_phase = InstrPhase::Complete;
    }

    fn exec_ext(&mut self, size: Size, reg: u8) {
        let value = match size {
            Size::Word => {
                // Extend byte to word
                let byte = self.regs.d[reg as usize] as i8 as i16 as u16;
                self.regs.d[reg as usize] =
                    (self.regs.d[reg as usize] & 0xFFFF_0000) | u32::from(byte);
                u32::from(byte)
            }
            Size::Long => {
                // Extend word to long
                let word = self.regs.d[reg as usize] as i16 as i32 as u32;
                self.regs.d[reg as usize] = word;
                word
            }
            Size::Byte => unreachable!(),
        };
        self.set_flags_move(value, size);
        self.queue_internal(4);
    }

    fn exec_movem_to_mem(&mut self, op: u16) {
        // MOVEM registers to memory
        // Format: 0100 1000 1s ea (s=0: word, s=1: long)
        // Register mask follows in extension word
        let size = if op & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        // Fetch the register mask plus any EA extension words
        self.size = size;
        self.addr2 = u32::from(ea_reg); // Store EA register for address update
        let addr_mode = AddrMode::decode(mode, ea_reg);
        let mut ext_needed = 1; // mask
        if let Some(mode) = addr_mode {
            ext_needed += self.ext_words_for_mode(mode) as usize;
        }
        for _ in 0..ext_needed {
            self.micro_ops.push(MicroOp::FetchExtWord);
        }
        self.instr_phase = InstrPhase::SrcEACalc;
        self.src_mode = addr_mode;
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_movem_to_mem_continuation(&mut self) {
        // Get mask via next_ext_word to properly advance ext_idx
        // This ensures subsequent next_ext_word calls get the displacement, not the mask
        let mask = self.next_ext_word();
        if mask == 0 {
            // No registers to transfer
            self.queue_internal(8);
            self.instr_phase = InstrPhase::Initial;
            return;
        }

        let Some(addr_mode) = self.src_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // Determine addressing mode and starting address
        let is_predec = matches!(addr_mode, AddrMode::AddrIndPreDec(_));
        self.movem_predec = is_predec;
        self.movem_postinc = false;
        self.movem_long_phase = 0;

        let (start_addr, ea_reg) = match addr_mode {
            AddrMode::AddrIndPreDec(r) => {
                // For predecrement, start at An - (count * size) so writes can proceed upward.
                let dec_per_reg = if self.size == Size::Long { 4 } else { 2 };
                let count = mask.count_ones();
                let start = self
                    .regs
                    .a(r as usize)
                    .wrapping_sub(count * dec_per_reg);
                if std::env::var("EMU68000_TRACE_MOVEM_PREDEC").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    eprintln!(
                        "[CPU] MOVEM predec @ ${op_pc:08X} A{}=${:08X} mask=${:04X} count={} size={:?} start=${:08X}",
                        r,
                        self.regs.a(r as usize),
                        mask,
                        count,
                        self.size,
                        start
                    );
                }
                (start, r)
            }
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp) as u32, r)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, 0)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                ((u32::from(hi) << 16) | u32::from(lo), 0)
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.next_ext_word();
                let disp = (ext & 0xFF) as i8 as i32;
                let idx_reg = ((ext >> 12) & 7) as usize;
                let idx_is_addr = ext & 0x8000 != 0;
                let idx_is_long = ext & 0x0800 != 0;
                let idx_value = if idx_is_addr {
                    self.regs.a(idx_reg)
                } else {
                    self.regs.d[idx_reg]
                };
                let idx_value = if idx_is_long {
                    idx_value as i32
                } else {
                    idx_value as i16 as i32
                };
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp).wrapping_add(idx_value) as u32, r)
            }
            _ => {
                self.queue_internal(8);
                self.instr_phase = InstrPhase::Initial;
                return;
            }
        };

        self.addr = start_addr;
        self.addr2 = u32::from(ea_reg);

        // Find first register to write
        // For predecrement mode, the mask is reversed: bit 0 = A7, bit 15 = D0
        // We iterate from highest bit down so D0 is written first (to lowest address)
        // For other modes: bit 0 = D0, bit 15 = A7, iterate up
        let first_bit = if is_predec {
            self.find_first_movem_bit_down(mask)
        } else {
            self.find_first_movem_bit_up(mask)
        };

        if let Some(bit) = first_bit {
            self.data2 = bit as u32;
            self.micro_ops.push(MicroOp::MovemWrite);
        }

        self.instr_phase = InstrPhase::Initial;
    }

    fn exec_tas(&mut self, mode: u8, ea_reg: u8) {
        // TAS <ea> - Test And Set
        // Tests the byte, sets N and Z, then sets bit 7 (atomically on real hardware)
        // Format: 0100 1010 11 mode reg
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // For data register, test and set bit 7 of low byte
                    let value = (self.regs.d[r as usize] & 0xFF) as u8;
                    self.set_flags_move(u32::from(value), Size::Byte);
                    let new_value = value | 0x80;
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(new_value);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory operand - calculate EA then do read-modify-write
                    // TAS is always a byte operation - set size for correct (An)+/-(An) increment
                    self.size = Size::Byte;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.movem_long_phase = 0;
                    // Single micro-op handles both read and write phases
                    self.micro_ops.push(MicroOp::TasExecute);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_tst(&mut self, size: Option<Size>, mode: u8, ea_reg: u8) {
        // TST <ea> - test operand (sets N and Z, clears V and C)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                    if log_this {
                        eprintln!(
                            "[CPU] exec_add ext_count={ext_count} addr_mode={addr_mode:?} continued={continued} phase={:?} queue_len={}",
                            self.instr_phase,
                            self.micro_ops.len()
                        );
                    }
                }
                if ext_count > 0 {
                    if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                        let op_pc = self.instr_start_pc.wrapping_sub(2);
                        let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                        if log_this {
                            eprintln!(
                                "[CPU] exec_add queue ext_words={ext_count} phase={:?} queue_pos={}",
                                self.instr_phase,
                                self.micro_ops.pos()
                            );
                        }
                    }
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }

            match addr_mode {
                AddrMode::DataReg(r) => {
                    let value = self.read_data_reg(r, size);
                    self.set_flags_move(value, size);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    // TST.L An is valid (68020+), but test anyway
                    let value = self.regs.a(r as usize);
                    self.set_flags_move(value, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory operand - read and set flags
                    self.size = size;
                    self.src_mode = Some(addr_mode); // Save for exception PC calculation
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = 0; // Not used for TST
                    self.data2 = 8; // 8 = TST
                    self.movem_long_phase = 0;
                    // Add internal cycles for EA calculation based on addressing mode
                    let ea_cycles = self.ea_calc_cycles(addr_mode);
                    if ea_cycles > 0 {
                        self.internal_cycles = ea_cycles;
                        self.micro_ops.push(MicroOp::Internal);
                    }
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_movem_from_mem(&mut self, op: u16) {
        // MOVEM memory to registers
        // Format: 0100 1100 1s ea (s=0: word, s=1: long)
        // Register mask follows in extension word
        let size = if op & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        // Fetch the register mask plus any EA extension words
        self.size = size;
        self.addr2 = u32::from(ea_reg); // Store EA register for address update
        let addr_mode = AddrMode::decode(mode, ea_reg);
        let mut ext_needed = 1; // mask
        if let Some(mode) = addr_mode {
            ext_needed += self.ext_words_for_mode(mode) as usize;
        }
        for _ in 0..ext_needed {
            self.micro_ops.push(MicroOp::FetchExtWord);
        }
        self.instr_phase = InstrPhase::DstEACalc;
        self.dst_mode = addr_mode;
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_movem_from_mem_continuation(&mut self) {
        // Get mask via next_ext_word to properly advance ext_idx
        // This ensures subsequent next_ext_word calls get the displacement, not the mask
        let mask = self.next_ext_word();
        if mask == 0 {
            // No registers to transfer
            self.queue_internal(12);
            self.instr_phase = InstrPhase::Initial;
            return;
        }

        let Some(addr_mode) = self.dst_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        // For memory to registers, order is always D0-D7, A0-A7 (bits 0-15)
        let is_postinc = matches!(addr_mode, AddrMode::AddrIndPostInc(_));
        self.movem_predec = false;
        self.movem_postinc = is_postinc;
        self.movem_long_phase = 0;

        let (start_addr, ea_reg) = match addr_mode {
            AddrMode::AddrIndPostInc(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp) as u32, r)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, 0)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                ((u32::from(hi) << 16) | u32::from(lo), 0)
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.next_ext_word();
                let disp = (ext & 0xFF) as i8 as i32;
                let idx_reg = ((ext >> 12) & 7) as usize;
                let idx_is_addr = ext & 0x8000 != 0;
                let idx_is_long = ext & 0x0800 != 0;
                let idx_value = if idx_is_addr {
                    self.regs.a(idx_reg)
                } else {
                    self.regs.d[idx_reg]
                };
                let idx_value = if idx_is_long {
                    idx_value as i32
                } else {
                    idx_value as i16 as i32
                };
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp).wrapping_add(idx_value) as u32, r)
            }
            AddrMode::PcDisp => {
                self.program_space_access = true;
                // Base PC = address of the displacement word.
                let base_pc = self.regs.pc.wrapping_sub(2);
                let disp = self.next_ext_word() as i16 as i32;
                ((base_pc as i32).wrapping_add(disp) as u32, 0)
            }
            AddrMode::PcIndex => {
                self.program_space_access = true;
                let base_pc = self.regs.pc.wrapping_sub(2);
                let ext = self.next_ext_word();
                let disp = (ext & 0xFF) as i8 as i32;
                let idx_reg = ((ext >> 12) & 7) as usize;
                let idx_is_addr = ext & 0x8000 != 0;
                let idx_is_long = ext & 0x0800 != 0;
                let idx_value = if idx_is_addr {
                    self.regs.a(idx_reg)
                } else {
                    self.regs.d[idx_reg]
                };
                let idx_value = if idx_is_long {
                    idx_value as i32
                } else {
                    idx_value as i16 as i32
                };
                ((base_pc as i32).wrapping_add(disp).wrapping_add(idx_value) as u32, 0)
            }
            _ => {
                self.queue_internal(12);
                self.instr_phase = InstrPhase::Initial;
                return;
            }
        };

        self.addr = start_addr;
        self.addr2 = u32::from(ea_reg);

        // Find first register to read (always ascending for memory-to-registers)
        let first_bit = self.find_first_movem_bit_up(mask);

        if let Some(bit) = first_bit {
            self.data2 = bit as u32;
            self.micro_ops.push(MicroOp::MovemRead);
        }

        self.instr_phase = InstrPhase::Initial;
    }

    fn exec_trap(&mut self, op: u16) {
        let vector = 32 + (op & 0xF) as u8;
        // TRAP is a 1-word instruction.
        // After FetchOpcode: PC = opcode_addr + 2 (pointing to next instruction)
        // This is the correct return address to push.
        self.exception(vector);
    }

    fn exec_link(&mut self, reg: u8) {
        // LINK An,#displacement (16 cycles)
        // 1. Push An onto stack
        // 2. Copy SP to An (An becomes frame pointer)
        // 3. Add signed displacement to SP (allocate stack space)
        match self.instr_phase {
            InstrPhase::Initial => {
                // If the extension word isn't already prefetched (test harness),
                // fetch it now and come back to continue.
                if self.ext_count == 0 {
                    self.addr2 = u32::from(reg);
                    self.micro_ops.push(MicroOp::FetchExtWord);
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }
            InstrPhase::SrcEACalc => {
                // Extension word fetched, continue to push An.
            }
            InstrPhase::SrcRead => {
                self.exec_link_continuation();
                return;
            }
            _ => {
                self.instr_phase = InstrPhase::Initial;
                return;
            }
        }

        // Get displacement from extension word (prefetched or just fetched)
        let disp = self.next_ext_word() as i16 as i32;

        // Push An
        let an_value = self.regs.a(reg as usize);
        self.data = an_value;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // Store reg and displacement for continuation
        self.addr2 = u32::from(reg);
        self.data2 = disp as u32;
        self.instr_phase = InstrPhase::SrcRead;
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_link_continuation(&mut self) {
        // After push completes:
        // 1. Copy SP to An (frame pointer)
        // 2. Add displacement to SP

        let reg = self.addr2 as usize;
        let disp = self.data2 as i32;

        // SP now points to the pushed value
        let sp = self.regs.active_sp();
        self.regs.set_a(reg, sp);

        // Add displacement to SP
        let new_sp = (sp as i32).wrapping_add(disp) as u32;
        self.regs.set_active_sp(new_sp);

        self.instr_phase = InstrPhase::Complete;
        // 8 internal cycles for the remaining operations
        self.queue_internal_no_pc(8);
    }

    fn exec_unlk(&mut self, reg: u8) {
        // UNLK An (12 cycles)
        // 1. Copy An to SP (restore stack to frame pointer)
        // 2. Pop An from stack (restore old frame pointer)

        // Get An value that will become the new SP
        let an_value = self.regs.a(reg as usize);

        // Check for address error BEFORE modifying SP
        // (the 68000 detects odd addresses before committing SP change)
        if an_value & 1 != 0 {
            self.fault_addr = an_value;
            self.fault_fc = if self.regs.is_supervisor() { 5 } else { 1 }; // Data access
            self.fault_read = true;
            self.fault_in_instruction = false;
            // For UNLINK address errors, the saved PC in the exception frame
            // should be the current PC (regs.pc), not regs.pc - 2.
            self.exception_pc_override = Some(self.regs.pc);
            self.exception(3); // Address error
            return;
        }

        // Copy An to SP
        self.regs.set_active_sp(an_value);

        // Pop An from stack
        self.micro_ops.push(MicroOp::PopLongHi);
        self.micro_ops.push(MicroOp::PopLongLo);

        // Store which register to restore, set up continuation
        self.addr2 = u32::from(reg);
        self.instr_phase = InstrPhase::SrcRead;
        self.micro_ops.push(MicroOp::Execute);
        // Don't advance PC here - the completion code will handle it
    }

    fn exec_unlk_continuation(&mut self) {
        // After pop completes: An = popped value (in self.data)
        let reg = self.addr2 as usize;
        self.regs.set_a(reg, self.data);

        self.instr_phase = InstrPhase::Complete;
        // 4 internal cycles for remaining operations
        self.queue_internal_no_pc(4);
    }

    fn exec_move_usp(&mut self, op: u16) {
        // MOVE USP - requires supervisor mode
        if !self.regs.is_supervisor() {
            self.exception(8); // Privilege violation
            return;
        }

        let reg = (op & 7) as usize;
        if op & 0x0008 != 0 {
            // USP -> An (bit 3 = 1)
            let usp = self.regs.usp;
            self.regs.set_a(reg, usp);
        } else {
            // An -> USP (bit 3 = 0)
            self.regs.usp = self.regs.a(reg);
        }
        self.queue_internal(4);
    }

    fn exec_reset(&mut self) {
        // RESET asserts RESET signal for 124 clock periods
        // Requires supervisor mode
        if !self.regs.is_supervisor() {
            self.exception(8); // Privilege violation
        } else {
            if std::env::var("EMU68000_TRACE_RESET").is_ok() {
                let op_pc = self.instr_start_pc.wrapping_sub(2);
                eprintln!(
                    "[CPU] RESET instruction at pc=${:08X} SR=${:04X} A7=${:08X}",
                    op_pc,
                    self.regs.sr,
                    self.regs.a(7)
                );
            }
            self.micro_ops.push(MicroOp::ResetBus);
            self.queue_internal(132); // RESET timing
        }
    }

    fn exec_stop(&mut self) {
        // STOP #imm - requires supervisor mode
        // Loads immediate value into SR (masked), then halts CPU
        if !self.regs.is_supervisor() {
            self.exception(8);
        } else {
            // Get the immediate value from extension word
            let imm = self.next_ext_word();

            // 68000 masks reserved bits when writing to SR
            self.regs.sr = imm & crate::flags::SR_MASK;

            // STOP halts the CPU waiting for interrupt
            self.state = crate::cpu::State::Stopped;

        }
    }

    fn exec_rte(&mut self) {
        // Return from exception - requires supervisor mode
        if !self.regs.is_supervisor() {
            self.exception(8);
        } else {
            // Pop SR first, then use continuation to save it before popping PC
            self.micro_ops.push(MicroOp::PopWord);
            self.instr_phase = InstrPhase::SrcRead;
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    fn exec_rte_continuation(&mut self) {
        match self.instr_phase {
            InstrPhase::SrcRead => {
                // After PopWord: data contains SR
                // Save SR to data2 before it's overwritten by PC pop
                self.data2 = self.data;
                // Now pop PC
                self.micro_ops.push(MicroOp::PopLongHi);
                self.micro_ops.push(MicroOp::PopLongLo);
                self.instr_phase = InstrPhase::DstWrite;
                self.micro_ops.push(MicroOp::Execute);
            }
            InstrPhase::DstWrite => {
                // After PopLong: data contains return PC, data2 contains SR
                // Apply the popped SR first (the real 68000 restores SR before
                // detecting the address error on the return PC, so the address
                // error frame saves the popped SR, not the original).
                let new_sr = (self.data2 as u16) & crate::flags::SR_MASK;
                self.trace_sr_update("RTE", new_sr);
                self.regs.sr = new_sr;
                // Check for odd return address (triggers address error)
                if self.data & 1 != 0 {
                    self.trigger_rte_address_error(self.data);
                    return;
                }
                if std::env::var("EMU68000_TRACE_SR").is_ok() {
                    eprintln!(
                        "[CPU] RTE return pc=${:08X} sr=${:04X}",
                        self.data,
                        self.regs.sr
                    );
                }
                self.trace_jump_to("RTE", self.data);
                // PC should return to the exact address popped.
                self.regs.pc = self.data;
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal_no_pc(8);
            }
            _ => {
                self.instr_phase = InstrPhase::Initial;
            }
        }
    }

    pub(super) fn trigger_rte_address_error(&mut self, addr: u32) {
        // RTE to odd address triggers address error.
        // Error detected during address calculation, before any bus cycle,
        // so I/N (fault_in_instruction) is 0.
        self.fault_fc = if self.regs.sr & crate::flags::S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    fn exec_trapv(&mut self) {
        if self.regs.sr & crate::flags::V != 0 {
            // TRAPV is a 1-word instruction.
            // PC already points past the instruction (correct return address).
            self.exception(7); // TRAPV exception
        } else {
            self.queue_internal(4);
        }
    }

    fn exec_rtr(&mut self) {
        // RTR = 20 cycles: 4 (pop CCR) + 8 (pop PC) + 8 (prefetch)
        // Pop CCR first, then save it before popping PC
        self.micro_ops.push(MicroOp::PopWord);
        self.instr_phase = InstrPhase::SrcRead; // Signal continuation
        self.micro_ops.push(MicroOp::Execute);
    }

    fn exec_rtr_continuation(&mut self) {
        match self.instr_phase {
            InstrPhase::SrcRead => {
                // After PopWord: data contains CCR word
                // Save CCR to data2 before it's overwritten by PC pop
                self.data2 = self.data;
                // Now pop PC
                self.micro_ops.push(MicroOp::PopLongHi);
                self.micro_ops.push(MicroOp::PopLongLo);
                self.instr_phase = InstrPhase::DstWrite;
                self.micro_ops.push(MicroOp::Execute);
            }
            InstrPhase::DstWrite => {
                // After PopLong: data contains return PC, data2 contains CCR
                // Restore CCR first — the 68000 applies the popped CCR before
                // detecting the address error on the return PC.
                let ccr = (self.data2 & 0x1F) as u8;
                self.regs.set_ccr(ccr);
                // Check for odd return address (triggers address error)
                if self.data & 1 != 0 {
                    self.trigger_rtr_address_error(self.data);
                    return;
                }
                self.trace_jump_to("RTR", self.data);
                // PC should return to the exact address popped.
                self.regs.pc = self.data;
                self.instr_phase = InstrPhase::Complete;
                self.queue_internal_no_pc(8); // Prefetch cycles
            }
            _ => {
                self.instr_phase = InstrPhase::Initial;
            }
        }
    }

    pub(super) fn trigger_rtr_address_error(&mut self, addr: u32) {
        // RTR to odd address triggers address error.
        // Error detected during address calculation, before any bus cycle,
        // so I/N (fault_in_instruction) is 0.
        self.fault_fc = if self.regs.sr & crate::flags::S != 0 { 6 } else { 2 };
        self.fault_addr = addr;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.exception(3);
    }

    fn exec_jsr(&mut self, mode: u8, ea_reg: u8) {
        // JSR <ea> - Jump to Subroutine (push return address, then jump)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let trace_jsr = |cpu: &Self, target: u32| {
                if std::env::var("EMU68000_TRACE_JSR").is_ok() {
                    let op_pc = cpu.instr_start_pc.wrapping_sub(2);
                    let sp = cpu.regs.a(7);
                    eprintln!(
                        "[CPU] JSR @ ${op_pc:08X} -> ${target:08X} sp=${sp:08X} ret=${:08X}",
                        cpu.regs.pc
                    );
                }
            };
            let ext_count = self.ext_words_for_mode(addr_mode);
            if ext_count > 0 {
                for _ in 0..ext_count {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                }
                self.instr_phase = InstrPhase::SrcEACalc;
                self.src_mode = Some(addr_mode);
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // JSR (An) = 16 cycles: 8 (push) + 8 (prefetch)
                    let target = self.regs.a(r as usize);
                    trace_jsr(self, target);
                    if std::env::var("EMU68000_TRACE_JSR_IND").is_ok() {
                        eprintln!(
                            "[CPU] JSR (A{}): pc=${:08X} target=${:08X}",
                            r,
                            self.regs.pc,
                            target
                        );
                    }
                    self.trace_jump_to("JSR", target);
                    // Check for odd target address - triggers address error (before push)
                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = 8; // prefetch
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AddrIndDisp(r) => {
                    // JSR d16(An) = 18 cycles: 2 (EA calc) + 8 (push) + 8 (prefetch)
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                    trace_jsr(self, target);
                    self.trace_jump_to("JSR", target);
                    // Check for odd target address - triggers address error (before push)
                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = 10; // 2 (EA) + 8 (prefetch)
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AddrIndIndex(r) => {
                    // JSR d8(An,Xn) = 22 cycles: 6 (EA calc) + 8 (push) + 8 (prefetch)
                    let target = self.calc_index_ea(self.regs.a(r as usize));
                    trace_jsr(self, target);
                    self.trace_jump_to("JSR", target);
                    // Check for odd target address - triggers address error (before push)
                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = 14; // 6 (EA) + 8 (prefetch)
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AbsShort => {
                    // JSR addr.W = 18 cycles: 2 (EA) + 8 (push) + 8 (prefetch)
                    let target = self.next_ext_word() as i16 as i32 as u32;
                    trace_jsr(self, target);
                    self.trace_jump_to("JSR", target);
                    // Check for odd target address - triggers address error (before push)
                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = 10; // 2 (EA) + 8 (prefetch)
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AbsLong => {
                    // JSR addr.L = 20 cycles: 4 (fetch 2nd word) + 8 (push) + 8 (prefetch)
                    let hi = u32::from(self.next_ext_word());
                    let lo = u32::from(self.next_ext_word());
                    let target = (hi << 16) | lo;
                    trace_jsr(self, target);
                    self.trace_jump_to("JSR", target);
                    // Check for odd target address - triggers address error (before push)
                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = 12; // 4 (EA) + 8 (prefetch)
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::PcDisp => {
                    // JSR d16(PC) = 18 cycles: 2 (EA calc) + 8 (push) + 8 (prefetch)
                    // PC-relative: base is address of extension word.
                    let pc_at_ext = self.regs.pc;
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (pc_at_ext as i32).wrapping_add(disp) as u32;
                    trace_jsr(self, target);
                    self.trace_jump_to("JSR", target);
                    // Check for odd target address - triggers address error (before push)
                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = 10; // 2 (EA) + 8 (prefetch)
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::PcIndex => {
                    // JSR d8(PC,Xn) = 22 cycles: 6 (EA calc) + 8 (push) + 8 (prefetch)
                    // PC-relative: base is address of extension word.
                    let pc_at_ext = self.regs.pc;
                    let target = self.calc_index_ea(pc_at_ext);
                    trace_jsr(self, target);
                    self.trace_jump_to("JSR", target);
                    // Check for odd target address - triggers address error (before push)
                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }
                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = 14; // 6 (EA) + 8 (prefetch)
                    self.micro_ops.push(MicroOp::Internal);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_jsr_continuation(&mut self) {
        let Some(addr_mode) = self.src_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let pc_at_ext =
            self.regs.pc.wrapping_sub(2 * u32::from(self.ext_count));

        let (target, cycles) = match addr_mode {
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                (
                    (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32,
                    10,
                )
            }
            AddrMode::AddrIndIndex(r) => (self.calc_index_ea(self.regs.a(r as usize)), 14),
            AddrMode::AbsShort => (self.next_ext_word() as i16 as i32 as u32, 10),
            AddrMode::AbsLong => {
                let hi = u32::from(self.next_ext_word());
                let lo = u32::from(self.next_ext_word());
                ((hi << 16) | lo, 12)
            }
            AddrMode::PcDisp => {
                let disp = self.next_ext_word() as i16 as i32;
                ((pc_at_ext as i32).wrapping_add(disp) as u32, 10)
            }
            AddrMode::PcIndex => (self.calc_index_ea(pc_at_ext), 14),
            _ => {
                self.illegal_instruction();
                return;
            }
        };

        if std::env::var("EMU68000_TRACE_JSR").is_ok() {
            let op_pc = self.instr_start_pc.wrapping_sub(2);
            let sp = self.regs.a(7);
            eprintln!(
                "[CPU] JSR @ ${op_pc:08X} -> ${target:08X} sp=${sp:08X} ret=${:08X}",
                self.regs.pc
            );
        }
        self.trace_jump_to("JSR", target);
        if target & 1 != 0 {
            self.trigger_jsr_address_error(target);
            return;
        }

        // Return address is current PC (past extension words)
        self.data = self.regs.pc;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
        self.set_jump_pc(target);
        self.instr_phase = InstrPhase::Complete;
        self.internal_cycles = cycles;
        self.micro_ops.push(MicroOp::Internal);
    }

    fn exec_jmp(&mut self, mode: u8, ea_reg: u8) {
        // JMP <ea> - Jump to address
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            let ext_count = self.ext_words_for_mode(addr_mode);
            if ext_count > 0 {
                for _ in 0..ext_count {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                }
                self.instr_phase = InstrPhase::SrcEACalc;
                self.src_mode = Some(addr_mode);
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            match addr_mode {
                AddrMode::AddrInd(r) => {
                    // JMP (An) = 8 cycles (prefetch only)
                    let target = self.regs.a(r as usize);
                    self.trace_jump_to("JMP", target);
                    // Check for odd target address (instruction fetch must be even)
                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }
                    self.set_jump_pc(target);
                    self.internal_cycles = 8;
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AddrIndDisp(r) => {
                    // JMP d16(An) = 10 cycles: 2 (EA) + 8 (prefetch)
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                    self.trace_jump_to("JMP", target);
                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }
                    self.set_jump_pc(target);
                    self.internal_cycles = 10;
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AddrIndIndex(r) => {
                    // JMP d8(An,Xn) = 14 cycles: 6 (EA) + 8 (prefetch)
                    let target = self.calc_index_ea(self.regs.a(r as usize));
                    self.trace_jump_to("JMP", target);
                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }
                    self.set_jump_pc(target);
                    self.internal_cycles = 14;
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AbsShort => {
                    // JMP addr.W = 10 cycles: 2 (EA) + 8 (prefetch)
                    let target = self.next_ext_word() as i16 as i32 as u32;
                    self.trace_jump_to("JMP", target);
                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }
                    self.set_jump_pc(target);
                    self.internal_cycles = 10;
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::AbsLong => {
                    // JMP addr.L = 12 cycles: 4 (fetch 2nd word) + 8 (prefetch)
                    let hi = u32::from(self.next_ext_word());
                    let lo = u32::from(self.next_ext_word());
                    let target = (hi << 16) | lo;
                    self.trace_jump_to("JMP", target);
                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }
                    self.set_jump_pc(target);
                    self.internal_cycles = 12;
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::PcDisp => {
                    // JMP d16(PC) = 10 cycles: 2 (EA) + 8 (prefetch)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let disp = self.next_ext_word() as i16 as i32;
                    let target = (pc_at_ext as i32).wrapping_add(disp) as u32;
                    self.trace_jump_to("JMP", target);
                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }
                    self.set_jump_pc(target);
                    self.internal_cycles = 10;
                    self.micro_ops.push(MicroOp::Internal);
                }
                AddrMode::PcIndex => {
                    // JMP d8(PC,Xn) = 14 cycles: 6 (EA) + 8 (prefetch)
                    let pc_at_ext = self.regs.pc.wrapping_sub(2);
                    let target = self.calc_index_ea(pc_at_ext);
                    self.trace_jump_to("JMP", target);
                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }
                    self.set_jump_pc(target);
                    self.internal_cycles = 14;
                    self.micro_ops.push(MicroOp::Internal);
                }
                _ => self.illegal_instruction(),
            }
        } else {
            self.illegal_instruction();
        }
    }

    /// Trigger address error for JMP to odd address.
    /// Although logically this is an instruction fetch, the 68000's I/N bit reflects
    /// that the error was detected during data processing (before actual prefetch).
    pub(super) fn trigger_jmp_address_error(&mut self, target: u32) {
        self.fault_addr = target;
        self.fault_fc = if self.regs.is_supervisor() { 6 } else { 2 }; // Program fetch
        self.fault_read = true;
        self.fault_in_instruction = false; // I/N=0 for address errors detected during EA calc
        // begin_exception subtracts 2 from PC, giving us PC-2 which is the return address
        self.exception(3); // Address error
    }

    pub(super) fn trigger_jsr_address_error(&mut self, target: u32) {
        // JSR to odd address triggers address error.
        // Exception PC should be the return address (instruction after JSR).
        self.fault_addr = target;
        self.fault_fc = if self.regs.is_supervisor() { 6 } else { 2 }; // Program fetch
        self.fault_read = true;
        self.fault_in_instruction = false; // I/N=0 for address errors detected during EA calc
        self.exception(3); // Address error
    }

    fn exec_jmp_continuation(&mut self) {
        let Some(addr_mode) = self.src_mode else {
            self.instr_phase = InstrPhase::Initial;
            return;
        };

        let pc_at_ext =
            self.regs.pc.wrapping_sub(2 * u32::from(self.ext_count));

        let (target, cycles) = match addr_mode {
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                (
                    (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32,
                    10,
                )
            }
            AddrMode::AddrIndIndex(r) => (self.calc_index_ea(self.regs.a(r as usize)), 14),
            AddrMode::AbsShort => (self.next_ext_word() as i16 as i32 as u32, 10),
            AddrMode::AbsLong => {
                let hi = u32::from(self.next_ext_word());
                let lo = u32::from(self.next_ext_word());
                ((hi << 16) | lo, 12)
            }
            AddrMode::PcDisp => {
                let disp = self.next_ext_word() as i16 as i32;
                ((pc_at_ext as i32).wrapping_add(disp) as u32, 10)
            }
            AddrMode::PcIndex => (self.calc_index_ea(pc_at_ext), 14),
            _ => {
                self.illegal_instruction();
                return;
            }
        };

        self.trace_jump_to("JMP", target);
        if target & 1 != 0 {
            self.trigger_jmp_address_error(target);
            return;
        }

        self.set_jump_pc(target);
        self.instr_phase = InstrPhase::Complete;
        self.internal_cycles = cycles;
        self.micro_ops.push(MicroOp::Internal);
    }

    fn exec_chk(&mut self, op: u16) {
        // CHK <ea>,Dn - Check register against bounds
        // If Dn < 0 or Dn > <ea>, trigger CHK exception
        let reg = ((op >> 9) & 7) as u8;
        let mode = ((op >> 3) & 7) as u8;
        let ea_reg = (op & 7) as u8;

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Get the data register value (word operation)
                    let dn = self.regs.d[reg as usize] as i16;
                    let upper_bound = self.regs.d[r as usize] as i16;

                    // Check bounds
                    if dn < 0 {
                        // N flag set for negative, clear N/Z/V/C then set N
                        // X is not affected by CHK
                        self.regs.sr &= !(N | Z | V | C);
                        self.regs.sr |= N;
                        self.exception(6); // CHK exception
                    } else if dn > upper_bound {
                        // N flag clear for upper bound violation, clear N/Z/V/C
                        // X is not affected by CHK
                        self.regs.sr &= !(N | Z | V | C);
                        self.exception(6); // CHK exception
                    } else {
                        // Value is within bounds — real 68000 clears NZVC
                        self.regs.sr &= !(N | Z | V | C);
                        self.queue_internal(10);
                    }
                }
                AddrMode::Immediate => {
                    let upper_bound = self.next_ext_word() as i16;
                    let dn = self.regs.d[reg as usize] as i16;

                    if dn < 0 {
                        self.regs.sr &= !(N | Z | V | C);
                        self.regs.sr |= N;
                        self.exception(6);
                    } else if dn > upper_bound {
                        self.regs.sr &= !(N | Z | V | C);
                        self.exception(6);
                    } else {
                        // Value is within bounds — real 68000 clears NZVC
                        self.regs.sr &= !(N | Z | V | C);
                        self.queue_internal(10);
                    }
                }
                _ => {
                    // Memory source - use AluMemSrc with CHK operation
                    self.size = Size::Word; // CHK always operates on word
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = u32::from(reg); // Data register to check
                    self.data2 = 9; // 9 = CHK
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_addq(&mut self, size: Option<Size>, data: u8, mode: u8, ea_reg: u8) {
        // ADDQ #data,<ea> - quick add (data = 1-8, where 0 encodes 8)
        let imm = if data == 0 { 8u32 } else { u32::from(data) };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrReg(r) => {
                    // Address register - always long operation, no flags affected
                    let val = self.regs.a(r as usize);
                    let result = val.wrapping_add(imm);
                    self.regs.set_a(r as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::DataReg(r) => {
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    let dst = self.read_data_reg(r, size);
                    let result = dst.wrapping_add(imm);
                    self.write_data_reg(r, result, size);
                    self.set_flags_add(imm, dst, result, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory destination - requires read-modify-write
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    let continued = self.instr_phase != InstrPhase::Initial;
                    if !continued {
                        let ext_count = self.ext_words_for_mode(addr_mode);
                        if ext_count > 0 {
                            for _ in 0..ext_count {
                                self.micro_ops.push(MicroOp::FetchExtWord);
                            }
                            self.size = size;
                            self.src_mode = Some(addr_mode);
                            self.instr_phase = InstrPhase::SrcEACalc;
                            self.micro_ops.push(MicroOp::Execute);
                            return;
                        }
                    }

                    let mode = if continued {
                        self.src_mode.unwrap_or(addr_mode)
                    } else {
                        addr_mode
                    };

                    self.size = size;
                    let pc_at_ext = self
                        .regs
                        .pc
                        .wrapping_sub(2 * u32::from(self.ext_count));
                    let (addr, _is_reg) = self.calc_ea(mode, pc_at_ext);
                    self.addr = addr;
                    self.src_mode = Some(mode);
                    self.data = imm; // Immediate value as source
                    self.data2 = 0; // 0 = ADD
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                    if continued {
                        self.instr_phase = InstrPhase::Complete;
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_subq(&mut self, size: Option<Size>, data: u8, mode: u8, ea_reg: u8) {
        // SUBQ #data,<ea> - quick subtract (data = 1-8, where 0 encodes 8)
        let imm = if data == 0 { 8u32 } else { u32::from(data) };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::AddrReg(r) => {
                    // Address register - always long operation, no flags affected
                    let val = self.regs.a(r as usize);
                    let result = val.wrapping_sub(imm);
                    self.regs.set_a(r as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::DataReg(r) => {
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    let dst = self.read_data_reg(r, size);
                    let result = dst.wrapping_sub(imm);
                    self.write_data_reg(r, result, size);
                    self.set_flags_sub(imm, dst, result, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory destination - requires read-modify-write
                    let Some(size) = size else {
                        self.illegal_instruction();
                        return;
                    };
                    let continued = self.instr_phase != InstrPhase::Initial;
                    if !continued {
                        let ext_count = self.ext_words_for_mode(addr_mode);
                        if ext_count > 0 {
                            for _ in 0..ext_count {
                                self.micro_ops.push(MicroOp::FetchExtWord);
                            }
                            self.size = size;
                            self.src_mode = Some(addr_mode);
                            self.instr_phase = InstrPhase::SrcEACalc;
                            self.micro_ops.push(MicroOp::Execute);
                            return;
                        }
                    }

                    let mode = if continued {
                        self.src_mode.unwrap_or(addr_mode)
                    } else {
                        addr_mode
                    };

                    self.size = size;
                    let pc_at_ext = self
                        .regs
                        .pc
                        .wrapping_sub(2 * u32::from(self.ext_count));
                    let (addr, _is_reg) = self.calc_ea(mode, pc_at_ext);
                    self.addr = addr;
                    self.src_mode = Some(mode);
                    self.data = imm; // Immediate value as source
                    self.data2 = 1; // 1 = SUB
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                    if continued {
                        self.instr_phase = InstrPhase::Complete;
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_scc(&mut self, condition: u8, mode: u8, ea_reg: u8) {
        // Scc <ea> - Set byte to $FF if condition true, $00 if false
        let value: u8 = if Status::condition(self.regs.sr, condition) {
            0xFF
        } else {
            0x00
        };

        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Set low byte of register
                    self.regs.d[r as usize] =
                        (self.regs.d[r as usize] & 0xFFFF_FF00) | u32::from(value);
                    // 4 cycles if false, 6 if true
                    self.queue_internal(if value == 0xFF { 6 } else { 4 });
                }
                _ => {
                    // Memory destination - write byte
                    self.size = Size::Byte;
                    let continued = self.instr_phase != InstrPhase::Initial;
                    if !continued {
                        let ext_count = self.ext_words_for_mode(addr_mode);
                        if ext_count > 0 {
                            for _ in 0..ext_count {
                                self.micro_ops.push(MicroOp::FetchExtWord);
                            }
                            self.src_mode = Some(addr_mode);
                            self.instr_phase = InstrPhase::SrcEACalc;
                            self.micro_ops.push(MicroOp::Execute);
                            return;
                        }
                    }

                    let mode = if continued {
                        self.src_mode.unwrap_or(addr_mode)
                    } else {
                        addr_mode
                    };

                    let pc_at_ext = self
                        .regs
                        .pc
                        .wrapping_sub(2 * u32::from(self.ext_count));
                    let (addr, _is_reg) = self.calc_ea(mode, pc_at_ext);
                    self.addr = addr;
                    self.src_mode = Some(mode);
                    self.data = u32::from(value);
                    self.micro_ops.push(MicroOp::WriteByte);
                    if continued {
                        self.instr_phase = InstrPhase::Complete;
                    }
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_dbcc(&mut self, condition: u8, reg: u8) {
        // DBcc Dn, label - Decrement and branch
        // If condition is true, no branch (fall through past displacement word)
        // If condition is false, decrement Dn.W and branch if Dn != -1
        // Note: DBcc ALWAYS has word displacement following the opcode

        // Store condition and register for continuation
        // data2: bits 0-3 = reg, bits 4-7 = condition
        self.data2 = u32::from(reg) | (u32::from(condition) << 4);
        self.instr_phase = InstrPhase::SrcRead;
        self.micro_ops.push(MicroOp::FetchExtWord);
        self.micro_ops.push(MicroOp::Execute);
    }

    /// DBcc continuation after fetching displacement word.
    fn dbcc_continuation(&mut self) {
        use crate::cpu::InstrPhase;

        if self.instr_phase != InstrPhase::SrcRead {
            self.instr_phase = InstrPhase::Complete;
            return;
        }

        let reg = (self.data2 & 0xF) as usize;
        let condition = ((self.data2 >> 4) & 0xF) as u8;
        let disp = self.ext_words[0] as i16 as i32;

        self.instr_phase = InstrPhase::Complete;

        if std::env::var("EMU68000_TRACE_DBCC").is_ok() {
            use std::sync::atomic::{AtomicBool, Ordering};
            static LOGGED: AtomicBool = AtomicBool::new(false);
            if !LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "[CPU] DBcc cont: op=${:04X} pc=${:08X} reg=D{} cond={} disp={:+} sr=${:04X} d{}=${:08X}",
                    self.opcode,
                    self.regs.pc,
                    reg,
                    condition,
                    disp,
                    self.regs.sr,
                    reg,
                    self.regs.d[reg]
                );
            }
        }

        if Status::condition(self.regs.sr, condition) {
            // Condition true - no branch, PC is already past the displacement word
            self.queue_internal_no_pc(12); // Condition true, no loop
        } else {
            // Condition false - check if we would branch
            let val = (self.regs.d[reg] & 0xFFFF) as i16;
            let new_val = val.wrapping_sub(1);

            if new_val == -1 {
                // Counter will be exhausted - no branch, fall through
                self.regs.d[reg] =
                    (self.regs.d[reg] & 0xFFFF_0000) | (new_val as u16 as u32);
                if condition == 1 && std::env::var("EMU68000_TRACE_DBF_TERM").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    eprintln!(
                        "[CPU] DBF term pc=${op_pc:08X} disp={:+} d{}=${:04X} sr=${:04X} d0=${:08X} d1=${:08X}",
                        disp,
                        reg,
                        new_val as u16,
                        self.regs.sr,
                        self.regs.d[0],
                        self.regs.d[1]
                    );
                }
                self.queue_internal_no_pc(14); // Loop terminated
            } else {
                // Counter not exhausted - would branch
                // Calculate branch target: displacement is relative to PC after opcode
                // PC is now at opcode+4 (past displacement), so target = PC - 2 + disp
                let target = ((self.regs.pc as i32) - 2 + disp) as u32;

                // Check for odd branch target - this causes address error
                if target & 1 != 0 {
                    // DBcc to odd address triggers address error.
                    // Exception PC should be instruction after DBcc (= current PC).
                    // I/N = 0 because error detected during address calculation.
                    self.exception_pc_override = Some(self.regs.pc);
                    self.fault_fc = if self.regs.sr & crate::flags::S != 0 { 6 } else { 2 };
                    self.fault_addr = target;
                    self.fault_read = true;
                    self.fault_in_instruction = false;
                    self.exception(3);
                    return;
                }

                // Target is valid - now we can decrement and branch
                if std::env::var("EMU68000_TRACE_DBF").is_ok()
                    && condition == 1
                {
                    let new_val_u16 = new_val as u16;
                    if new_val_u16 == 0xFFFF || (new_val_u16 & 0x1FFF) == 0 {
                        let new_d = (self.regs.d[reg] & 0xFFFF_0000) | u32::from(new_val_u16);
                        eprintln!(
                            "[CPU] DBF D{} new=${:04X} pc=${:08X} d0=${:08X} d1=${:08X}",
                            reg,
                            new_val_u16,
                            self.regs.pc,
                            new_d,
                            self.regs.d[1]
                        );
                    }
                }
                self.regs.d[reg] =
                    (self.regs.d[reg] & 0xFFFF_0000) | (new_val as u16 as u32);
                // In normal mode, FetchOpcode will read from PC and advance by +2.
                // So set PC to the actual branch target.
                self.regs.pc = target;
                self.queue_internal_no_pc(10); // Loop continues
            }
        }
    }

    /// Compute exact DIVU cycle timing based on Jorge Cwik's algorithm.
    /// Returns total clock cycles for the division computation.
    /// The 68000 uses a restoring division algorithm where timing depends
    /// on intermediate values during the shift-and-subtract process.
    pub(crate) fn divu_cycles(dividend: u32, divisor: u16) -> u8 {
        // Overflow case
        if (dividend >> 16) >= u32::from(divisor) {
            return 10;
        }

        let mut mcycles: u32 = 38;
        let hdivisor = u32::from(divisor) << 16;
        let mut dvd = dividend;

        for _ in 0..15 {
            let temp = dvd;
            dvd <<= 1;

            if temp & 0x8000_0000 != 0 {
                // Carry from shift — subtract divisor (no extra cycles)
                dvd = dvd.wrapping_sub(hdivisor);
            } else {
                // No carry — 2 extra cycles for the comparison step
                mcycles += 2;
                if dvd >= hdivisor {
                    // Subtraction succeeds — save 1 cycle
                    dvd = dvd.wrapping_sub(hdivisor);
                    mcycles -= 1;
                }
            }
        }
        (mcycles * 2) as u8
    }

    /// Compute exact DIVS cycle timing based on Jorge Cwik's algorithm.
    /// Returns total clock cycles for the division computation.
    pub(crate) fn divs_cycles(dividend: i32, divisor: i16) -> u8 {
        let mut mcycles: u32 = 6;
        if dividend < 0 {
            mcycles += 1;
        }

        // Overflow check using absolute values
        let abs_dividend = (dividend as i64).unsigned_abs() as u32;
        let abs_divisor = (divisor as i32).unsigned_abs() as u16;

        if (abs_dividend >> 16) >= u32::from(abs_divisor) {
            return ((mcycles + 2) * 2) as u8;
        }

        // Compute absolute quotient for bit-counting
        let mut aquot = abs_dividend / u32::from(abs_divisor);

        mcycles += 55;

        if divisor >= 0 {
            if dividend >= 0 {
                mcycles -= 1;
            } else {
                mcycles += 1;
            }
        }

        // Count 15 MSBs of absolute quotient — each 0-bit adds 1 mcycle
        for _ in 0..15 {
            if (aquot as i16) >= 0 {
                mcycles += 1;
            }
            aquot <<= 1;
        }
        (mcycles * 2) as u8
    }

    fn exec_divu(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // DIVU <ea>,Dn - unsigned 32/16 -> 16r:16q division
        // Result: Dn = remainder(high word) : quotient(low word)
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            self.size = Size::Word;
            let ext_count = self.ext_words_for_mode(addr_mode);
            if ext_count > 0 && self.instr_phase == InstrPhase::Initial {
                for _ in 0..ext_count {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                }
                self.instr_phase = InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            if self.instr_phase != InstrPhase::Initial && self.instr_phase != InstrPhase::SrcRead {
                self.instr_phase = InstrPhase::Complete;
                return;
            }
            let continued = self.instr_phase == InstrPhase::SrcRead;
            let pc_at_ext = self
                .regs
                .pc
                .wrapping_sub(2 * u32::from(self.ext_count));
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let divisor = self.regs.d[r as usize] & 0xFFFF;
                    let dividend = self.regs.d[reg as usize];

                    if divisor == 0 {
                        // Division by zero - trap
                        self.exception(5);
                        return;
                    }

                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;

                    // Compute timing from actual division algorithm
                    let timing = Self::divu_cycles(dividend, divisor as u16);

                    // Check for overflow (quotient > 16 bits)
                    if quotient > 0xFFFF {
                        // Overflow - set V flag, don't store result
                        self.regs.sr |= crate::flags::V;
                        self.regs.sr &= !crate::flags::C; // C always cleared
                        // On the real 68000, N is always set on DIVU overflow
                        self.regs.sr |= crate::flags::N;
                        // Z is cleared on overflow
                        self.regs.sr &= !crate::flags::Z;
                    } else {
                        // Store result: remainder:quotient
                        self.regs.d[reg as usize] = (remainder << 16) | quotient;

                        // Set flags
                        self.regs.sr &= !(crate::flags::V | crate::flags::C);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::N, quotient & 0x8000 != 0);
                    }
                    self.queue_internal(timing);
                }
                AddrMode::Immediate => {
                    // DIVU #imm,Dn - immediate source
                    let divisor = u32::from(self.next_ext_word());
                    let dividend = self.regs.d[reg as usize];

                    if divisor == 0 {
                        self.exception(5);
                        return;
                    }

                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;

                    let timing = Self::divu_cycles(dividend, divisor as u16);

                    if quotient > 0xFFFF {
                        self.regs.sr |= crate::flags::V;
                        self.regs.sr &= !crate::flags::C;
                        // On the real 68000, N is always set on DIVU overflow
                        self.regs.sr |= crate::flags::N;
                        self.regs.sr &= !crate::flags::Z;
                    } else {
                        self.regs.d[reg as usize] = (remainder << 16) | quotient;
                        self.regs.sr &= !(crate::flags::V | crate::flags::C);
                        self.regs.sr =
                            Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                        self.regs.sr = Status::set_if(
                            self.regs.sr,
                            crate::flags::N,
                            quotient & 0x8000 != 0,
                        );
                    }
                    self.queue_internal(timing);
                }
                _ => {
                    // Memory source - use AluMemSrc
                    self.src_mode = Some(addr_mode);
                    self.size = Size::Word; // DIVU reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, pc_at_ext);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 12; // 12 = DIVU
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_divs(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // DIVS <ea>,Dn - signed 32/16 -> 16r:16q division
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            self.size = Size::Word;
            let ext_count = self.ext_words_for_mode(addr_mode);
            if ext_count > 0 && self.instr_phase == InstrPhase::Initial {
                for _ in 0..ext_count {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                }
                self.instr_phase = InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            if self.instr_phase != InstrPhase::Initial && self.instr_phase != InstrPhase::SrcRead {
                self.instr_phase = InstrPhase::Complete;
                return;
            }
            let continued = self.instr_phase == InstrPhase::SrcRead;
            let pc_at_ext = self
                .regs
                .pc
                .wrapping_sub(2 * u32::from(self.ext_count));
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let divisor = (self.regs.d[r as usize] as i16) as i32;
                    let dividend = self.regs.d[reg as usize] as i32;

                    if divisor == 0 {
                        // Division by zero - trap
                        self.exception(5);
                        return;
                    }

                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;

                    // Compute timing from actual division algorithm
                    let timing = Self::divs_cycles(dividend, divisor as i16);

                    // Check for overflow (quotient doesn't fit in signed 16-bit)
                    if !(-32768..=32767).contains(&quotient) {
                        // Overflow
                        self.regs.sr |= crate::flags::V;
                        self.regs.sr &= !crate::flags::C;
                        // On the real 68000, N is always set on DIVS overflow
                        self.regs.sr |= crate::flags::N;
                        // Z is cleared on overflow
                        self.regs.sr &= !crate::flags::Z;
                    } else {
                        // Store result: remainder:quotient (both as 16-bit values)
                        let q = quotient as i16 as u16 as u32;
                        let r = remainder as i16 as u16 as u32;
                        self.regs.d[reg as usize] = (r << 16) | q;

                        // Set flags
                        self.regs.sr &= !(crate::flags::V | crate::flags::C);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                        self.regs.sr = Status::set_if(self.regs.sr, crate::flags::N, quotient < 0);
                    }
                    self.queue_internal(timing);
                }
                AddrMode::Immediate => {
                    // DIVS #imm,Dn - immediate source
                    let divisor = (self.next_ext_word() as i16) as i32;
                    let dividend = self.regs.d[reg as usize] as i32;

                    if divisor == 0 {
                        self.exception(5);
                        return;
                    }

                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;

                    let timing = Self::divs_cycles(dividend, divisor as i16);

                    if !(-32768..=32767).contains(&quotient) {
                        self.regs.sr |= crate::flags::V;
                        self.regs.sr &= !crate::flags::C;
                        // On the real 68000, N is always set on DIVS overflow
                        self.regs.sr |= crate::flags::N;
                        self.regs.sr &= !crate::flags::Z;
                    } else {
                        let q = quotient as i16 as u16 as u32;
                        let r = remainder as i16 as u16 as u32;
                        self.regs.d[reg as usize] = (r << 16) | q;
                        self.regs.sr &= !(crate::flags::V | crate::flags::C);
                        self.regs.sr =
                            Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                        self.regs.sr =
                            Status::set_if(self.regs.sr, crate::flags::N, quotient < 0);
                    }
                    self.queue_internal(timing);
                }
                _ => {
                    // Memory source - use AluMemSrc
                    self.src_mode = Some(addr_mode);
                    self.size = Size::Word; // DIVS reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, pc_at_ext);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 13; // 13 = DIVS
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_sbcd(&mut self, op: u16) {
        // SBCD - Subtract Decimal with Extend (packed BCD subtraction)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        // Format: 1000 Ry 10000 R Rx (R=0 register, R=1 memory)
        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0;

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = Size::Byte;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 1; // 1 = SBCD
            self.movem_long_phase = 0;
            self.extend_predec_done = false;
            // Set addressing modes so exception handling can detect predec mode
            self.src_mode = Some(AddrMode::AddrIndPreDec(rx as u8));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(ry as u8));
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dy - Dx - X -> Dy
            let src = self.regs.d[rx] as u8;
            let dst = self.regs.d[ry] as u8;
            let x = u8::from(self.regs.sr & X != 0);

            let (result, borrow, overflow) = self.bcd_sub(dst, src, x);

            // Write result to low byte of Dy
            self.regs.d[ry] = (self.regs.d[ry] & 0xFFFF_FF00) | u32::from(result);

            // Set flags
            let mut sr = self.regs.sr;
            // Z: cleared if non-zero, unchanged otherwise
            if result != 0 {
                sr &= !Z;
            }
            // C and X: set if decimal borrow
            sr = Status::set_if(sr, C, borrow);
            sr = Status::set_if(sr, X, borrow);
            // N: set based on MSB of result
            sr = Status::set_if(sr, N, result & 0x80 != 0);
            // V: set when BCD correction flips bit 7
            sr = Status::set_if(sr, V, overflow);
            self.regs.sr = sr;

            self.queue_internal(6);
        }
    }

    /// Perform packed BCD subtraction: dst - src - extend.
    /// Returns (result, borrow, overflow).
    pub(crate) fn bcd_sub(&self, dst: u8, src: u8, extend: u8) -> (u8, bool, bool) {
        // Binary subtraction first
        let uncorrected = dst.wrapping_sub(src).wrapping_sub(extend);

        let mut result = uncorrected;

        // Low nibble correction: if low nibble would have underflowed
        let low_borrowed = (dst & 0x0F) < (src & 0x0F).saturating_add(extend);
        if low_borrowed {
            result = result.wrapping_sub(6);
        }

        // High nibble correction: only if the original high nibble underflowed
        let high_borrowed = (dst >> 4) < (src >> 4) + u8::from(low_borrowed);
        if high_borrowed {
            result = result.wrapping_sub(0x60);
        }

        // Borrow: set if either the original high nibble underflowed, OR
        // the low nibble correction (-6) caused the whole byte to wrap.
        // The latter happens when the uncorrected result is < 6 and low
        // correction is needed (with invalid BCD inputs).
        let low_correction_wraps = low_borrowed && uncorrected < 6;
        let borrow = high_borrowed || low_correction_wraps;

        // V: set when BCD correction flips bit 7 from 1 to 0
        let overflow = (uncorrected & !result & 0x80) != 0;

        (result, borrow, overflow)
    }

    /// Perform NBCD: negate BCD (0 - src - X).
    /// The 68000 uses a specific algorithm different from regular BCD subtraction.
    /// Returns (result, borrow, overflow).
    pub(crate) fn nbcd(&self, src: u8, extend: u8) -> (u8, bool, bool) {
        // NBCD: compute as 0 - src - X using BCD subtraction
        let (result, borrow, overflow) = self.bcd_sub(0, src, extend);

        (result, borrow, overflow)
    }

    fn exec_or(&mut self, size: Option<Size>, reg: u8, mode: u8, ea_reg: u8, to_ea: bool) {
        // OR Dn,<ea> or OR <ea>,Dn
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if ext_count > 0 {
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = if to_ea {
                        InstrPhase::DstEACalc
                    } else {
                        InstrPhase::SrcEACalc
                    };
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }
            if to_ea {
                // OR Dn,<ea> - destination is EA
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst | src;
                        self.write_data_reg(r, result, size);
                        self.set_flags_move(result, size); // OR sets N,Z, clears V,C
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 3; // 3 = OR
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
                }
            } else {
                // OR <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst | src;
                        self.write_data_reg(reg, result, size);
                        self.set_flags_move(result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::Immediate => {
                        // OR #imm,Dn (same behaviour as ORI #imm,Dn)
                        self.size = size;
                        let src = self.read_immediate();
                        let dst = self.read_data_reg(reg, size);
                        let result = dst | src;
                        self.write_data_reg(reg, result, size);
                        self.set_flags_move(result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory source
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 3; // 3 = OR
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
                    }
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_sub(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        to_ea: bool,
    ) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                    if log_this {
                        eprintln!(
                            "[CPU] exec_add ext_words={ext_count} addr_mode={addr_mode:?}"
                        );
                    }
                }
                if ext_count > 0 {
                    if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                        let op_pc = self.instr_start_pc.wrapping_sub(2);
                        let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                        if log_this {
                            eprintln!(
                                "[CPU] exec_add queue ext_words={ext_count} phase={:?}",
                                self.instr_phase
                            );
                        }
                    }
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = if to_ea {
                        InstrPhase::DstEACalc
                    } else {
                        InstrPhase::SrcEACalc
                    };
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }
            if continued && std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                let op_pc = self.instr_start_pc.wrapping_sub(2);
                let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                if log_this {
                    eprintln!("[CPU] exec_add continued phase={:?}", self.instr_phase);
                }
            }
            if to_ea {
                // SUB Dn,<ea> - destination is EA
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(r, result, size);
                        self.set_flags_sub(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 1; // 1 = SUB
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
                }
            } else {
                // SUB <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_sub(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::AddrReg(r) => {
                        // SUB.W/L An,Dn
                        let src = self.regs.a(r as usize);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_sub(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::Immediate => {
                        // SUB #imm,Dn (same behaviour as SUBI #imm,Dn)
                        self.size = size;
                        let src = self.read_immediate();
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_sub(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory source
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 1; // 1 = SUB
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
                    }
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_suba(&mut self, size: Size, reg: u8, mode: u8, ea_reg: u8) {
        // SUBA <ea>,An - subtract from address register (no flags affected)
        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                    if log_this {
                        eprintln!(
                            "[CPU] exec_add ext_count={ext_count} addr_mode={addr_mode:?} continued={continued} phase={:?} queue_len={}",
                            self.instr_phase,
                            self.micro_ops.len()
                        );
                    }
                }
                if ext_count > 0 {
                    if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                        let op_pc = self.instr_start_pc.wrapping_sub(2);
                        let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                        if log_this {
                            eprintln!(
                                "[CPU] exec_add queue ext_words={ext_count} phase={:?} queue_pos={}",
                                self.instr_phase,
                                self.micro_ops.pos()
                            );
                        }
                    }
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }

            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.d[r as usize] as i16 as i32 as u32
                    } else {
                        self.regs.d[r as usize]
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.a(r as usize) as i16 as i32 as u32
                    } else {
                        self.regs.a(r as usize)
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::Immediate => {
                    self.size = size;
                    let src = if size == Size::Word {
                        self.next_ext_word() as i16 as i32 as u32
                    } else {
                        let hi = u32::from(self.next_ext_word());
                        let lo = u32::from(self.next_ext_word());
                        (hi << 16) | lo
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = u32::from(reg);
                    self.data2 = 6; // 6 = SUBA
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }

            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_subx(&mut self, op: u16) {
        // SUBX - Subtract with extend (for multi-precision arithmetic)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0; // Register/Memory flag

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = size;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 3; // 3 = SUBX
            self.movem_long_phase = 0;
            self.extend_predec_done = false;
            // Set addressing modes so exception handling can detect predec mode
            self.src_mode = Some(AddrMode::AddrIndPreDec(rx as u8));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(ry as u8));
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dx,Dy
            let src = self.read_data_reg(rx as u8, size);
            let dst = self.read_data_reg(ry as u8, size);
            let x = u32::from(self.regs.sr & X != 0);

            let result = dst.wrapping_sub(src).wrapping_sub(x);
            self.write_data_reg(ry as u8, result, size);

            // SUBX flags (like SUB but Z is only cleared, never set)
            let (src_masked, dst_masked, result_masked, msb) = match size {
                Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80u32),
                Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
                Size::Long => (src, dst, result, 0x8000_0000),
            };

            let mut sr = self.regs.sr;
            sr = Status::set_if(sr, N, result_masked & msb != 0);
            // Z: cleared if non-zero, unchanged if zero
            if result_masked != 0 {
                sr &= !Z;
            }
            // V: overflow
            let overflow = ((dst_masked ^ src_masked) & (dst_masked ^ result_masked) & msb) != 0;
            sr = Status::set_if(sr, V, overflow);
            // C: borrow
            let carry = src_masked.wrapping_add(x) > dst_masked
                || (src_masked == dst_masked && x != 0);
            sr = Status::set_if(sr, C, carry);
            sr = Status::set_if(sr, X, carry);

            self.regs.sr = sr;
            self.queue_internal(if size == Size::Long { 8 } else { 4 });
        }
    }

    fn exec_cmp(&mut self, size: Option<Size>, reg: u8, mode: u8, ea_reg: u8) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        // CMP <ea>,Dn - compare (dst - src, set flags only)
        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                    if log_this {
                        eprintln!(
                            "[CPU] exec_add ext_count={ext_count} addr_mode={addr_mode:?} continued={continued} phase={:?} queue_len={}",
                            self.instr_phase,
                            self.micro_ops.len()
                        );
                    }
                }
                if ext_count > 0 {
                    if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                        let op_pc = self.instr_start_pc.wrapping_sub(2);
                        let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                        if log_this {
                            eprintln!(
                                "[CPU] exec_add queue ext_words={ext_count} phase={:?} queue_pos={}",
                                self.instr_phase,
                                self.micro_ops.pos()
                            );
                        }
                    }
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }

            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.read_data_reg(r, size);
                    let dst = self.read_data_reg(reg, size);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, size);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = self.regs.a(r as usize);
                    let dst = self.read_data_reg(reg, size);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, size);
                    self.queue_internal(4);
                }
                AddrMode::Immediate => {
                    // CMP #imm,Dn (same behaviour as CMPI #imm,Dn)
                    self.size = size;
                    let src = self.read_immediate();
                    let dst = self.read_data_reg(reg, size);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, size);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = u32::from(reg);
                    self.data2 = 4; // 4 = CMP
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_cmpa(&mut self, size: Size, reg: u8, mode: u8, ea_reg: u8) {
        // CMPA <ea>,An - compare to address register
        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if ext_count > 0 {
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }

            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.d[r as usize] as i16 as i32 as u32
                    } else {
                        self.regs.d[r as usize]
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, Size::Long);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.a(r as usize) as i16 as i32 as u32
                    } else {
                        self.regs.a(r as usize)
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, Size::Long);
                    self.queue_internal(4);
                }
                AddrMode::Immediate => {
                    self.size = size;
                    let src = if size == Size::Word {
                        self.next_ext_word() as i16 as i32 as u32
                    } else {
                        let hi = u32::from(self.next_ext_word());
                        let lo = u32::from(self.next_ext_word());
                        (hi << 16) | lo
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, Size::Long);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = u32::from(reg);
                    self.data2 = 7; // 7 = CMPA
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }

            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_cmpm(&mut self, op: u16) {
        // CMPM (Ay)+,(Ax)+ - Compare memory with postincrement
        // Format: 1011 Ax 1 ss 001 Ay
        let ay = (op & 7) as usize;
        let ax = ((op >> 9) & 7) as usize;
        let size = Size::from_bits(((op >> 6) & 3) as u8);

        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        // Set up state for tick_cmpm_execute
        self.size = size;
        self.addr = self.regs.a(ay);   // Source address
        self.addr2 = self.regs.a(ax);  // Destination address
        self.data = ay as u32;         // Source register number
        self.data2 = ax as u32;        // Destination register number
        self.movem_long_phase = 0;     // Start at phase 0

        // Queue the CMPM micro-op (two phases: read src, read dst + compare)
        // followed by internal cycles for the final prefetch which advances PC.
        self.micro_ops.push(MicroOp::CmpmExecute);
        self.queue_internal(4);
    }

    fn exec_eor(&mut self, size: Option<Size>, reg: u8, mode: u8, ea_reg: u8) {
        // EOR Dn,<ea> - exclusive OR (source is always data register)
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if ext_count > 0 {
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::DstEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.read_data_reg(reg, size);
                    let dst = self.read_data_reg(r, size);
                    let result = dst ^ src;
                    self.write_data_reg(r, result, size);
                    self.set_flags_move(result, size); // EOR sets N,Z, clears V,C
                    self.queue_internal(4);
                }
                _ => {
                    // Memory destination: read-modify-write
                    self.src_mode = Some(addr_mode);
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = self.read_data_reg(reg, size);
                    self.data2 = 4; // 4 = EOR
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_mulu(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // MULU <ea>,Dn - unsigned 16x16->32 multiply
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            self.size = Size::Word;
            let ext_count = self.ext_words_for_mode(addr_mode);
            if ext_count > 0 && self.instr_phase == InstrPhase::Initial {
                for _ in 0..ext_count {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                }
                self.instr_phase = InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            if self.instr_phase != InstrPhase::Initial && self.instr_phase != InstrPhase::SrcRead {
                self.instr_phase = InstrPhase::Complete;
                return;
            }
            let continued = self.instr_phase == InstrPhase::SrcRead;
            let pc_at_ext = self
                .regs
                .pc
                .wrapping_sub(2 * u32::from(self.ext_count));
            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = self.regs.d[r as usize] & 0xFFFF;
                    let dst = self.regs.d[reg as usize] & 0xFFFF;
                    let result = src * dst;
                    self.regs.d[reg as usize] = result;

                    // Set flags: N based on bit 31, Z if result is 0, V=0, C=0
                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                    // Timing: 38 + 2*number of 1-bits in source operand
                    let ones = (src as u16).count_ones() as u8;
                    self.queue_internal(38 + 2 * ones);
                }
                AddrMode::Immediate => {
                    // MULU #imm,Dn - immediate source
                    let src = u32::from(self.next_ext_word());
                    let dst = self.regs.d[reg as usize] & 0xFFFF;
                    let result = src * dst;
                    self.regs.d[reg as usize] = result;

                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                    let ones = (src as u16).count_ones() as u8;
                    self.queue_internal(38 + 2 * ones);
                }
                _ => {
                    // Memory source - use AluMemSrc
                    self.src_mode = Some(addr_mode);
                    self.size = Size::Word; // MULU reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, pc_at_ext);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 10; // 10 = MULU
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_muls(&mut self, reg: u8, mode: u8, ea_reg: u8) {
        // MULS <ea>,Dn - signed 16x16->32 multiply
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            self.size = Size::Word;
            let ext_count = self.ext_words_for_mode(addr_mode);
            if ext_count > 0 && self.instr_phase == InstrPhase::Initial {
                for _ in 0..ext_count {
                    self.micro_ops.push(MicroOp::FetchExtWord);
                }
                self.instr_phase = InstrPhase::SrcRead;
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            if self.instr_phase != InstrPhase::Initial && self.instr_phase != InstrPhase::SrcRead {
                self.instr_phase = InstrPhase::Complete;
                return;
            }
            let continued = self.instr_phase == InstrPhase::SrcRead;
            let pc_at_ext = self
                .regs
                .pc
                .wrapping_sub(2 * u32::from(self.ext_count));
            match addr_mode {
                AddrMode::DataReg(r) => {
                    // Save source word for timing BEFORE writing result
                    // (when r == reg, the register is overwritten by the result)
                    let src16 = self.regs.d[r as usize] as u16;
                    let src = (self.regs.d[r as usize] as i16) as i32;
                    let dst = (self.regs.d[reg as usize] as i16) as i32;
                    let result = (src * dst) as u32;
                    self.regs.d[reg as usize] = result;

                    // Set flags: N based on bit 31, Z if result is 0, V=0, C=0
                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                    // Timing: 38 + 2*number of bit transitions in source word
                    let pattern = src16 ^ (src16 << 1);
                    let ones = pattern.count_ones() as u8;
                    self.queue_internal(38 + 2 * ones);
                }
                AddrMode::Immediate => {
                    // MULS #imm,Dn - immediate source
                    let src = self.next_ext_word() as i16 as i32;
                    let dst = (self.regs.d[reg as usize] as i16) as i32;
                    let result = (src * dst) as u32;
                    self.regs.d[reg as usize] = result;

                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                    let src16 = src as u16;
                    let pattern = src16 ^ (src16 << 1);
                    let ones = pattern.count_ones() as u8;
                    self.queue_internal(38 + 2 * ones);
                }
                _ => {
                    // Memory source - use AluMemSrc
                    self.src_mode = Some(addr_mode);
                    self.size = Size::Word; // MULS reads word operand
                    let (addr, _is_reg) = self.calc_ea(addr_mode, pc_at_ext);
                    self.addr = addr;
                    self.data = u32::from(reg);
                    self.data2 = 11; // 11 = MULS
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_abcd(&mut self, op: u16) {
        // ABCD - Add Decimal with Extend (packed BCD addition)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        // Format: 1100 Ry 10000 R Rx (R=0 register, R=1 memory)
        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0;

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = Size::Byte;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 0; // 0 = ABCD
            self.movem_long_phase = 0;
            self.extend_predec_done = false;
            // Set addressing modes so exception handling can detect predec mode
            self.src_mode = Some(AddrMode::AddrIndPreDec(rx as u8));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(ry as u8));
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dx,Dy
            let src = self.regs.d[rx] as u8;
            let dst = self.regs.d[ry] as u8;
            let x = u8::from(self.regs.sr & X != 0);

            let (result, carry, overflow) = self.bcd_add(src, dst, x);

            // Write result to low byte of Dy
            self.regs.d[ry] = (self.regs.d[ry] & 0xFFFF_FF00) | u32::from(result);

            // Set flags
            let mut sr = self.regs.sr;
            // Z: cleared if non-zero, unchanged otherwise
            if result != 0 {
                sr &= !Z;
            }
            // C and X: set if decimal carry
            sr = Status::set_if(sr, C, carry);
            sr = Status::set_if(sr, X, carry);
            // N: set based on MSB of result
            sr = Status::set_if(sr, N, result & 0x80 != 0);
            // V: set when BCD correction flips bit 7
            sr = Status::set_if(sr, V, overflow);
            self.regs.sr = sr;

            self.queue_internal(6);
        }
    }

    /// Perform packed BCD addition: src + dst + extend.
    /// Returns (result, carry, overflow).
    /// The overflow flag matches real 68000 hardware: set when bit 7 flips from 0 to 1
    /// during BCD correction.
    pub(crate) fn bcd_add(&self, src: u8, dst: u8, extend: u8) -> (u8, bool, bool) {
        // Low nibble: binary add then correct
        let low_sum = (dst & 0x0F) + (src & 0x0F) + extend;
        let corf: u16 = if low_sum > 9 { 6 } else { 0 };

        // Full binary sum (before any correction)
        let uncorrected = u16::from(dst) + u16::from(src) + u16::from(extend);

        // Carry: compute from high digit sum including full carry from low
        // correction. With invalid BCD inputs the low correction can produce
        // more than one bit of carry (e.g. 0xD + 0xD + 1 + 6 = 33, carry = 2).
        let low_corrected = low_sum + if low_sum > 9 { 6 } else { 0 };
        let low_carry = low_corrected >> 4;
        let high_sum = (dst >> 4) + (src >> 4) + low_carry;
        let carry = high_sum > 9;

        // Result: apply low correction, then high correction if carry
        let result = if carry {
            uncorrected + corf + 0x60
        } else {
            uncorrected + corf
        };

        // V: set when uncorrected bit 7 was 0 but corrected bit 7 is 1
        let overflow = (!uncorrected & result & 0x80) != 0;

        (result as u8, carry, overflow)
    }

    fn exec_exg(&mut self, op: u16) {
        let rx = ((op >> 9) & 7) as usize;
        let ry = (op & 7) as usize;
        let mode = (op >> 3) & 0x1F;

        match mode {
            0x08 => {
                // Exchange data registers
                let tmp = self.regs.d[rx];
                self.regs.d[rx] = self.regs.d[ry];
                self.regs.d[ry] = tmp;
            }
            0x09 => {
                // Exchange address registers
                let tmp = self.regs.a(rx);
                self.regs.set_a(rx, self.regs.a(ry));
                self.regs.set_a(ry, tmp);
            }
            0x11 => {
                // Exchange data and address registers
                let tmp = self.regs.d[rx];
                self.regs.d[rx] = self.regs.a(ry);
                self.regs.set_a(ry, tmp);
            }
            _ => self.illegal_instruction(),
        }
        self.queue_internal(6);
    }

    fn exec_and(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        to_ea: bool,
    ) {
        // AND Dn,<ea> or AND <ea>,Dn
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if ext_count > 0 {
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = if to_ea {
                        InstrPhase::DstEACalc
                    } else {
                        InstrPhase::SrcEACalc
                    };
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }
            if to_ea {
                // AND Dn,<ea> - destination is EA
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst & src;
                        self.write_data_reg(r, result, size);
                        self.set_flags_move(result, size); // AND sets N,Z, clears V,C
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 2; // 2 = AND
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
                }
            } else {
                // AND <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst & src;
                        self.write_data_reg(reg, result, size);
                        self.set_flags_move(result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::Immediate => {
                        // AND #imm,Dn (same behaviour as ANDI #imm,Dn)
                        self.size = size;
                        let src = self.read_immediate();
                        let dst = self.read_data_reg(reg, size);
                        let result = dst & src;
                        self.write_data_reg(reg, result, size);
                        self.set_flags_move(result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory source
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 2; // 2 = AND
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
                    }
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_add(
        &mut self,
        size: Option<Size>,
        reg: u8,
        mode: u8,
        ea_reg: u8,
        to_ea: bool,
    ) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
            let op_pc = self.instr_start_pc.wrapping_sub(2);
            let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
            if log_this {
                eprintln!(
                    "[CPU] exec_add op_pc=${op_pc:08X} size={size:?} mode={mode} ea_reg={ea_reg} to_ea={to_ea} phase={:?}",
                    self.instr_phase
                );
            }
        }
        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                    let op_pc = self.instr_start_pc.wrapping_sub(2);
                    let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                    if log_this {
                        eprintln!(
                            "[CPU] exec_add ext_count={ext_count} addr_mode={addr_mode:?} continued={continued} phase={:?} queue_len={}",
                            self.instr_phase,
                            self.micro_ops.len()
                        );
                    }
                }
                if ext_count > 0 {
                    if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                        let op_pc = self.instr_start_pc.wrapping_sub(2);
                        let log_this = trace_add_pc_target().map_or(true, |target| target == op_pc);
                        if log_this {
                            eprintln!(
                                "[CPU] exec_add queue ext_words={ext_count} phase={:?} queue_pos={}",
                                self.instr_phase,
                                self.micro_ops.pos()
                            );
                        }
                    }
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = if to_ea {
                        InstrPhase::DstEACalc
                    } else {
                        InstrPhase::SrcEACalc
                    };
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }
            if to_ea {
                // ADD Dn,<ea> - destination is EA
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(reg, size);
                        let dst = self.read_data_reg(r, size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(r, result, size);
                        self.set_flags_add(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory destination: read-modify-write
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                            let op_pc = self.instr_start_pc.wrapping_sub(2);
                            let log_this =
                                trace_add_pc_target().map_or(true, |target| target == op_pc);
                            if log_this {
                                eprintln!(
                                    "[CPU] ADD->EA op_pc=${op_pc:08X} mode={addr_mode:?} addr=${addr:08X}"
                                );
                            }
                        }
                        self.addr = addr;
                        self.data = self.read_data_reg(reg, size);
                        self.data2 = 0; // 0 = ADD
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemRmw);
                    }
                }
            } else {
                // ADD <ea>,Dn - destination is register
                match addr_mode {
                    AddrMode::DataReg(r) => {
                        let src = self.read_data_reg(r, size);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_add(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::AddrReg(r) => {
                        // ADD.W/L An,Dn
                        let src = self.regs.a(r as usize);
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_add(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    AddrMode::Immediate => {
                        // ADD #imm,Dn (same behaviour as ADDI #imm,Dn)
                        self.size = size;
                        let src = self.read_immediate();
                        let dst = self.read_data_reg(reg, size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(reg, result, size);
                        self.set_flags_add(src, dst, result, size);
                        self.queue_internal(4);
                    }
                    _ => {
                        // Memory source: read from memory, add to register
                        self.src_mode = Some(addr_mode);
                        self.size = size;
                        let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                        if std::env::var("EMU68000_TRACE_ADD_EA").is_ok() {
                            let op_pc = self.instr_start_pc.wrapping_sub(2);
                            let log_this =
                                trace_add_pc_target().map_or(true, |target| target == op_pc);
                            if log_this {
                                eprintln!(
                                    "[CPU] ADD<-EA op_pc=${op_pc:08X} mode={addr_mode:?} addr=${addr:08X}"
                                );
                            }
                        }
                        self.addr = addr;
                        self.data = u32::from(reg);
                        self.data2 = 0; // 0 = ADD
                        self.movem_long_phase = 0;
                        self.micro_ops.push(MicroOp::AluMemSrc);
                    }
                }
            }
            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_adda(&mut self, size: Size, reg: u8, mode: u8, ea_reg: u8) {
        // ADDA <ea>,An - add to address register (no flags affected)
        let continued = self.instr_phase != InstrPhase::Initial;
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            if !continued {
                self.size = size;
                let ext_count = self.ext_words_for_mode(addr_mode);
                if ext_count > 0 {
                    for _ in 0..ext_count {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    self.instr_phase = InstrPhase::SrcEACalc;
                    self.micro_ops.push(MicroOp::Execute);
                    return;
                }
            }

            match addr_mode {
                AddrMode::DataReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.d[r as usize] as i16 as i32 as u32
                    } else {
                        self.regs.d[r as usize]
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_add(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::AddrReg(r) => {
                    let src = if size == Size::Word {
                        self.regs.a(r as usize) as i16 as i32 as u32
                    } else {
                        self.regs.a(r as usize)
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_add(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                AddrMode::Immediate => {
                    self.size = size;
                    let src = if size == Size::Word {
                        self.next_ext_word() as i16 as i32 as u32
                    } else {
                        let hi = u32::from(self.next_ext_word());
                        let lo = u32::from(self.next_ext_word());
                        (hi << 16) | lo
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = dst.wrapping_add(src);
                    self.regs.set_a(reg as usize, result);
                    self.queue_internal(4);
                }
                _ => {
                    // Memory source
                    self.size = size;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.src_mode = Some(addr_mode);
                    self.data = u32::from(reg);
                    self.data2 = 5; // 5 = ADDA
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemSrc);
                }
            }

            if continued {
                self.instr_phase = InstrPhase::Complete;
            }
        } else {
            self.illegal_instruction();
        }
    }

    fn exec_addx(&mut self, op: u16) {
        // ADDX - Add with extend (for multi-precision arithmetic)
        // Two forms: Dx,Dy or -(Ax),-(Ay)
        let size = Size::from_bits(((op >> 6) & 3) as u8);
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let rx = (op & 7) as usize;
        let ry = ((op >> 9) & 7) as usize;
        let rm = op & 0x0008 != 0; // Register/Memory flag

        if rm {
            // Memory to memory: -(Ax),-(Ay)
            // DON'T pre-decrement here - let tick_extend_mem_op handle it
            // so address errors don't modify registers
            self.size = size;
            // Pack register numbers: data = rx | (ry << 8)
            self.data = (rx as u32) | ((ry as u32) << 8);
            self.data2 = 2; // 2 = ADDX
            self.movem_long_phase = 0;
            // Mark that registers haven't been pre-decremented yet
            self.extend_predec_done = false;
            // Set addressing modes so exception handling can detect predec mode
            self.src_mode = Some(AddrMode::AddrIndPreDec(rx as u8));
            self.dst_mode = Some(AddrMode::AddrIndPreDec(ry as u8));
            self.micro_ops.push(MicroOp::ExtendMemOp);
        } else {
            // Register to register: Dx,Dy
            let src = self.read_data_reg(rx as u8, size);
            let dst = self.read_data_reg(ry as u8, size);
            let x = u32::from(self.regs.sr & X != 0);

            let result = dst.wrapping_add(src).wrapping_add(x);
            self.write_data_reg(ry as u8, result, size);

            // ADDX flags (like ADD but Z is only cleared, never set)
            let (src_masked, dst_masked, result_masked, msb) = match size {
                Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80u32),
                Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
                Size::Long => (src, dst, result, 0x8000_0000),
            };

            let mut sr = self.regs.sr;
            sr = Status::set_if(sr, N, result_masked & msb != 0);
            // Z: cleared if non-zero, unchanged if zero
            if result_masked != 0 {
                sr &= !Z;
            }
            // V: overflow (same signs in, different sign out)
            let overflow = (!(src_masked ^ dst_masked) & (src_masked ^ result_masked) & msb) != 0;
            sr = Status::set_if(sr, V, overflow);
            // C: carry out
            let carry = match size {
                Size::Byte => {
                    (u16::from(src as u8) + u16::from(dst as u8) + u16::from(x as u8)) > 0xFF
                }
                Size::Word => {
                    (u32::from(src as u16) + u32::from(dst as u16) + x) > 0xFFFF
                }
                Size::Long => src.checked_add(dst).and_then(|v| v.checked_add(x)).is_none(),
            };
            sr = Status::set_if(sr, C, carry);
            sr = Status::set_if(sr, X, carry);

            self.regs.sr = sr;
            self.queue_internal(if size == Size::Long { 8 } else { 4 });
        }
    }

    fn exec_shift_mem(&mut self, kind: u8, direction: bool, mode: u8, ea_reg: u8) {
        // Memory shift/rotate - always word size, always shift by 1
        // Format: 1110 kind dr 11 mode reg
        // kind: 00=AS, 01=LS, 10=ROX, 11=RO
        // dr: 0=right, 1=left
        if let Some(addr_mode) = AddrMode::decode(mode, ea_reg) {
            match addr_mode {
                AddrMode::DataReg(_) | AddrMode::AddrReg(_) => {
                    // Memory only - registers use exec_shift_reg
                    self.illegal_instruction();
                }
                _ => {
                    // Memory operand - calculate EA then do read-modify-write
                    // Memory shifts are always word size - set for correct (An)+/-(An) increment
                    self.src_mode = Some(addr_mode);
                    self.size = Size::Word;
                    let (addr, _is_reg) = self.calc_ea(addr_mode, self.regs.pc);
                    self.addr = addr;
                    self.data = u32::from(kind);
                    self.data2 = u32::from(direction);
                    self.movem_long_phase = 0;
                    // Single micro-op handles both read and write phases
                    self.micro_ops.push(MicroOp::ShiftMemExecute);
                }
            }
        } else {
            self.illegal_instruction();
        }
    }

    pub(super) fn exec_shift_reg(
        &mut self,
        kind: u8,
        direction: bool, // false=right, true=left
        count_or_reg: u8,
        reg: u8,
        size: Option<Size>,
        immediate: bool, // true=count in opcode, false=count in register
    ) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        // Get shift count (1-8 for immediate, or from register mod 64)
        let count = if immediate {
            if count_or_reg == 0 { 8 } else { count_or_reg as u32 }
        } else {
            self.regs.d[count_or_reg as usize] % 64
        };

        let value = self.read_data_reg(reg, size);
        let (mask, msb_bit) = match size {
            Size::Byte => (0xFF_u32, 0x80_u32),
            Size::Word => (0xFFFF, 0x8000),
            Size::Long => (0xFFFF_FFFF, 0x8000_0000),
        };

        let (result, carry) = match (kind, direction) {
            // ASL - Arithmetic shift left
            (0, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        let c = if count == bits {
                            value & 1 != 0
                        } else {
                            false
                        };
                        (0, c)
                    } else {
                        let shifted = (value << count) & mask;
                        let c = (value >> (bits - count)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // ASR - Arithmetic shift right (sign extends)
            (0, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let sign_bit = value & msb_bit != 0;
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    // For large counts, result is all sign bits
                    if count >= bits {
                        let result = if sign_bit { mask } else { 0 };
                        (result, sign_bit)
                    } else {
                        let mut result = value;
                        for _ in 0..count {
                            result = (result >> 1) | if sign_bit { msb_bit } else { 0 };
                        }
                        let c = (value >> (count - 1)) & 1 != 0;
                        (result & mask, c)
                    }
                }
            }
            // LSL - Logical shift left
            (1, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        // All bits shifted out, carry is last bit shifted out
                        let c = if count == bits {
                            value & 1 != 0
                        } else {
                            false
                        };
                        (0, c)
                    } else {
                        let shifted = (value << count) & mask;
                        let c = (value >> (bits - count)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // LSR - Logical shift right
            (1, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        // All bits shifted out
                        let c = if count == bits {
                            (value >> (bits - 1)) & 1 != 0
                        } else {
                            false
                        };
                        (0, c)
                    } else {
                        let shifted = (value >> count) & mask;
                        let c = (value >> (count - 1)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // ROXL - Rotate through X left
            // X flag is included as bit in the rotation chain
            (2, true) => {
                if count == 0 {
                    // Count 0: result unchanged, C = X
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    // Rotation is through (bits + 1) positions (including X)
                    let total_bits = bits + 1;
                    let count = count % total_bits;
                    if count == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        // Build extended value: X in position 'bits', data in lower bits
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        // Rotate left through the (bits+1) wide value
                        let rotated = ((extended << count) | (extended >> (total_bits - count)))
                            & ((1u64 << total_bits) - 1);
                        // Extract result and new X
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
            }
            // ROXR - Rotate through X right
            // X flag is included as bit in the rotation chain
            (2, false) => {
                if count == 0 {
                    // Count 0: result unchanged, C = X
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    // Rotation is through (bits + 1) positions (including X)
                    let total_bits = bits + 1;
                    let count = count % total_bits;
                    if count == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        // Build extended value: X in position 'bits', data in lower bits
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        // Rotate right through the (bits+1) wide value
                        let rotated = ((extended >> count) | (extended << (total_bits - count)))
                            & ((1u64 << total_bits) - 1);
                        // Extract result and new X
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
            }
            // ROL - Rotate left
            (3, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let eff_count = count % bits;
                    if eff_count == 0 {
                        // Full rotation, value unchanged, carry is LSB
                        (value, value & 1 != 0)
                    } else {
                        let rotated = ((value << eff_count) | (value >> (bits - eff_count))) & mask;
                        let c = rotated & 1 != 0;
                        (rotated, c)
                    }
                }
            }
            // ROR - Rotate right
            (3, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let eff_count = count % bits;
                    if eff_count == 0 {
                        // Full rotation, value unchanged, carry is MSB
                        (value, value & msb_bit != 0)
                    } else {
                        let rotated = ((value >> eff_count) | (value << (bits - eff_count))) & mask;
                        let c = (value >> (eff_count - 1)) & 1 != 0;
                        (rotated, c)
                    }
                }
            }
            _ => (value, false),
        };

        self.write_data_reg(reg, result, size);

        // Set flags
        // N and Z based on result
        self.set_flags_move(result, size);

        // C flag is last bit shifted out (or cleared if count=0 for non-ROX)
        // X flag is set same as C for shifts and ROXL/ROXR, unchanged for ROL/ROR
        if count > 0 {
            self.regs.sr = Status::set_if(self.regs.sr, C, carry);
            // X is set for shifts (kind 0,1) and ROXL/ROXR (kind 2), not ROL/ROR (kind 3)
            if kind < 3 {
                self.regs.sr = Status::set_if(self.regs.sr, X, carry);
            }
        } else {
            // Count=0: For ROXL/ROXR, C = X; for others, C is cleared
            if kind == 2 {
                let x = self.regs.sr & X != 0;
                self.regs.sr = Status::set_if(self.regs.sr, C, x);
            } else {
                self.regs.sr &= !C;
            }
        }

        // V flag handling:
        // - For ASL: V is set if the MSB changed at ANY point during the shift
        // - For all other shifts/rotates: V is always cleared
        if kind == 0 && direction {
            // ASL: Check if any of the bits that pass through the MSB position differ
            if count == 0 {
                self.regs.sr &= !crate::flags::V;
            } else {
                let bits = match size {
                    Size::Byte => 8u32,
                    Size::Word => 16,
                    Size::Long => 32,
                };

                let v = if count >= bits {
                    // When shifting by >= operand size, ALL bits shift out and zeros fill in.
                    // V is set if value is non-zero (at some point MSB will change)
                    // - If all 0s: MSB never changes (0→0), V=0
                    // - If any 1s: eventually MSB becomes 1 then shifts out to 0, V=1
                    (value & mask) != 0
                } else {
                    // Build a mask for the top (count+1) bits
                    // Note: when count+1 >= bits, we need to check all bits
                    let check_bits = count + 1;
                    let check_mask = if check_bits >= bits {
                        mask
                    } else {
                        ((1u32 << check_bits) - 1) << (bits - check_bits)
                    };
                    // Get the bits to check
                    let top_bits = value & check_mask;
                    // V is set if these bits are neither all 0s nor all 1s
                    top_bits != 0 && top_bits != check_mask
                };
                self.regs.sr = Status::set_if(self.regs.sr, crate::flags::V, v);
            }
        } else {
            // All other shifts/rotates: V is always cleared
            self.regs.sr &= !crate::flags::V;
        }

        // Timing: 6 + 2*count for byte/word, 8 + 2*count for long
        let base_cycles = if size == Size::Long { 8 } else { 6 };
        self.queue_internal(base_cycles + 2 * count as u8);
    }
}
