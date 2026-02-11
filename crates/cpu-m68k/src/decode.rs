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
            // MOVE.b / MOVE.w / MOVE.l / MOVEA.w / MOVEA.l
            0x1 | 0x2 | 0x3 => self.exec_move(),
            0x7 => {
                // MOVEQ: 0111 RRR 0 DDDDDDDD
                if op & 0x0100 == 0 {
                    self.exec_moveq();
                } else {
                    self.illegal_instruction();
                }
            }
            _ => self.illegal_instruction(),
        }
    }

    /// Decode a 6-bit EA field (mode 3 bits + reg 3 bits).
    pub(crate) fn decode_ea(mode: u8, reg: u8) -> Option<AddrMode> {
        AddrMode::decode(mode & 7, reg & 7)
    }
}
