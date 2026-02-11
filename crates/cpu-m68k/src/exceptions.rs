//! Exception handling for the 68000.
//!
//! Exception groups:
//! - Group 0: Reset, bus error, address error (highest priority)
//! - Group 1: Trace, interrupt, illegal instruction, privilege violation
//! - Group 2: TRAP, TRAPV, CHK, zero divide
//!
//! Standard exception frame: 6 bytes (SR, PC)
//! Group 0 exception frame: 14 bytes (additional fault info)
//!
//! This is a stub — exception handling will be fleshed out in Phase 11.

use crate::cpu::Cpu68000;
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Trigger an exception by vector number.
    ///
    /// For now this is a minimal stub that queues the exception frame push
    /// and vector fetch. Full implementation comes in Phase 11.
    pub(crate) fn exception(&mut self, vector: u8) {
        // Save SR before entering supervisor mode
        let old_sr = self.regs.sr;

        // Enter supervisor mode, clear trace
        self.regs.sr |= 0x2000; // Set S
        self.regs.sr &= !0x8000; // Clear T

        // Push exception frame: PC (long), then SR (word)
        // 68000 pushes PC first (high word at lower address), then SR
        // The return PC should be the start of the faulting instruction
        let return_pc = self.exception_pc_override
            .take()
            .unwrap_or(self.instr_start_pc);

        // For now, set data to PC and push, then set data to SR and push
        // This is simplified — proper implementation will handle group 0 frames
        self.data = return_pc;
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::Internal(4)); // Exception processing overhead

        // Push PC (long: high word first since predecrement)
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // Push SR (word) — but we need to set data to SR first
        // Since PushWord uses self.data, we need a way to set it between pushes.
        // For now we'll store the SR and handle it specially.
        // TODO: This needs a more sophisticated approach — use a followup Execute
        // to load SR into data between the PC push and SR push.

        // Simplified: just store what we need
        self.data2 = u32::from(old_sr);
        self.addr2 = u32::from(vector) * 4; // Vector table address

        // We need a staged exception handler. For Phase 0, use a simple approach:
        // Store the values directly.
        // Queue exception processing as internal cycles + reads
        self.micro_ops.push(MicroOp::Internal(0)); // Followup to push SR
        self.micro_ops.push(MicroOp::Execute); // Will handle via in_followup

        // Set up followup to continue exception processing
        self.in_followup = true;
        self.followup_tag = 0xFE; // Exception continuation tag
    }

    /// Continue exception processing after PC has been pushed.
    /// Called from decode_and_execute when followup_tag == 0xFE.
    pub(crate) fn exception_continue(&mut self) {
        // Push old SR
        self.data = self.data2; // Old SR stored in data2
        self.micro_ops.push(MicroOp::PushWord);

        // Read exception vector
        self.addr = self.addr2; // Vector address stored in addr2
        self.micro_ops.push(MicroOp::ReadLongHi);
        self.micro_ops.push(MicroOp::ReadLongLo);

        // After vector is read, we need to jump there and refill prefetch
        self.followup_tag = 0xFF; // Vector jump tag
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Finish exception processing: jump to vector address.
    /// Called from decode_and_execute when followup_tag == 0xFF.
    pub(crate) fn exception_jump_vector(&mut self) {
        // self.data now contains the vector address (read by ReadLongHi + ReadLongLo)
        self.regs.pc = self.data;
        self.in_followup = false;
        self.followup_tag = 0;

        // Need to fill the prefetch pipeline at the new PC
        // Queue two FetchIRC ops to fill IR and IRC, then Execute
        self.micro_ops.push(MicroOp::FetchIRC); // Fills IRC from new PC
        self.micro_ops.push(MicroOp::Internal(0)); // Move IRC to IR
        self.micro_ops.push(MicroOp::Execute); // New followup to move IRC->IR and fetch again
        self.followup_tag = 0xFD; // Prefetch fill tag
        self.in_followup = true;
    }

    /// Fill prefetch after exception vector jump.
    /// Called from decode_and_execute when followup_tag == 0xFD.
    pub(crate) fn exception_fill_prefetch(&mut self) {
        // First FetchIRC has completed, IRC now contains first word at vector target.
        // Move it to IR and fetch the next word.
        self.ir = self.irc;
        self.instr_start_pc = self.irc_addr;
        self.in_followup = false;
        self.followup_tag = 0;
        // Queue FetchIRC for the next word, then Execute to decode the first instruction
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
    }
}
