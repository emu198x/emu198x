//! Instruction decode for the 68000.
//!
//! Decodes IR (the opcode in the instruction register) and dispatches to
//! the appropriate execute handler. Multi-stage instructions use the
//! `in_followup` / `followup_tag` mechanism to resume decode after
//! consuming extension words from IRC.

use crate::addressing::AddrMode;
use crate::cpu::Cpu68000;

impl Cpu68000 {
    /// Decode the current IR and execute the instruction.
    ///
    /// Called as an instant (0-cycle) micro-op from the tick engine.
    /// For single-stage instructions, this queues all necessary micro-ops.
    /// For multi-stage instructions, this consumes IRC (queuing FetchIRC),
    /// then queues another Execute for the next stage.
    pub(crate) fn decode_and_execute(&mut self) {
        // Handle exception continuations first
        if self.in_followup {
            match self.followup_tag {
                0xE0 => {
                    // STOP completion: enter Stopped state after FetchIRC refilled IRC.
                    self.state = crate::cpu::State::Stopped;
                    self.in_followup = false;
                    self.followup_tag = 0;
                    return;
                }
                0xFA => { self.exception_group0_vector(); return; }
                0xFB => { self.exception_group0_finish(); return; }
                0xFC => { self.exception_group0_continue(); return; }
                0xFD => { self.exception_fill_prefetch(); return; }
                0xFE => { self.exception_continue(); return; }
                0xFF => { self.exception_jump_vector(); return; }
                _ => {
                    // Instruction-specific followup — fall through to opcode dispatch.
                    // The instruction handler (e.g. exec_move) checks in_followup
                    // and followup_tag to resume its multi-stage decode.
                }
            }
        }

        let op = self.ir;

        match op >> 12 {
            // Immediate ALU (ORI, ANDI, SUBI, ADDI, EORI, CMPI) + bit ops
            0x0 => self.exec_group0(),
            // MOVE.b / MOVE.w / MOVE.l / MOVEA.w / MOVEA.l
            0x1 | 0x2 | 0x3 => self.exec_move(),
            // Miscellaneous: LEA, CLR, NEG, NOT, TST, JMP, JSR, RTS, RTE, etc.
            0x4 => self.exec_group4(),
            // ADDQ/SUBQ/Scc/DBcc: 0101 DDD O SS MMMRRR
            0x5 => self.exec_addq_subq(),
            // Bcc/BRA/BSR: 0110 CCCC DDDDDDDD
            0x6 => self.exec_branch(),
            0x7 => {
                // MOVEQ: 0111 RRR 0 DDDDDDDD
                if op & 0x0100 == 0 {
                    self.exec_moveq();
                } else {
                    self.illegal_instruction();
                }
            }
            // OR/DIVU/SBCD: 1000 RRR OOO MMMRRR
            0x8 => self.exec_or(),
            // SUB/SUBA: 1001 RRR OOO MMMRRR
            0x9 => self.exec_add_sub(false),
            // CMP/CMPA/EOR: 1011 RRR OOO MMMRRR
            0xB => self.exec_cmp_eor(),
            // AND/MULU/ABCD/EXG: 1100 RRR OOO MMMRRR
            0xC => self.exec_and(),
            // ADD/ADDA: 1101 RRR OOO MMMRRR
            0xD => self.exec_add_sub(true),
            // Line A: $Axxx — unimplemented, vector 10
            0xA => self.exception(10, 0),
            // Line F: $Fxxx — unimplemented, vector 11
            0xF => self.exception(11, 0),
            // Shifts/rotates: 1110 CCC D SS I TT RRR
            0xE => self.exec_shift_rotate(),
            _ => self.illegal_instruction(),
        }
    }

    /// Decode a 6-bit EA field (mode 3 bits + reg 3 bits).
    pub(crate) fn decode_ea(mode: u8, reg: u8) -> Option<AddrMode> {
        AddrMode::decode(mode & 7, reg & 7)
    }
}
