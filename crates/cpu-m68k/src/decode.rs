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
                    // Instruction-specific followup — will be handled per-instruction
                    // in later phases. For now, treat as illegal.
                    self.in_followup = false;
                    self.followup_tag = 0;
                    self.illegal_instruction();
                    return;
                }
            }
        }

        let op = self.ir;

        // All opcodes are currently unimplemented — trigger illegal instruction
        // exception. As instruction groups are implemented in later phases,
        // this match will expand.
        match op >> 12 {
            // Phase 1+: instruction groups will be added here
            _ => self.illegal_instruction(),
        }
    }

    /// Decode a 6-bit EA field (mode 3 bits + reg 3 bits).
    pub(crate) fn decode_ea(mode: u8, reg: u8) -> Option<AddrMode> {
        AddrMode::decode(mode & 7, reg & 7)
    }
}
