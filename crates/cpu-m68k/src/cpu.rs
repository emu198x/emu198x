//! Motorola 68000 CPU core with cycle-accurate IR/IRC prefetch pipeline.
//!
//! The tick engine follows the Z80 crate's proven architecture — per-cycle
//! ticking, explicit micro-op queue, instant Execute — adapted for the
//! 68000's 4-cycle bus and 2-word prefetch pipeline.
//!
//! ## Prefetch pipeline: IR + IRC
//!
//! - **IR** (Instruction Register): the opcode being executed
//! - **IRC** (Instruction Register Cache): the next prefetched word
//!
//! At any point, the CPU has already fetched two words ahead. When an
//! instruction consumes IRC (as an extension word or as the next opcode),
//! a 4-cycle bus fetch replaces it from memory at PC.

use emu_core::{Observable, Ticks, Value};

use crate::alu::Size;
use crate::bus::{FunctionCode, M68kBus};
use crate::exceptions::ExceptionState;
use crate::flags::{Status, C, N, V, X, Z};
use crate::microcode::{MicroOp, MicroOpQueue};
use crate::registers::Registers;

/// CPU execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    /// Normal execution.
    Running,
    /// Halted (double bus fault).
    Halted,
    /// Stopped (STOP instruction, waiting for interrupt).
    Stopped,
}

/// Motorola 68000 CPU.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cpu68000 {
    // === Registers ===
    pub regs: Registers,

    // === Prefetch pipeline ===
    /// Instruction Register: the opcode being executed.
    pub(crate) ir: u16,
    /// Instruction Register Cache: the next prefetched word.
    pub(crate) irc: u16,
    /// Address where IRC was fetched from (for PC-relative addressing).
    pub(crate) irc_addr: u32,

    // === Execution state ===
    pub(crate) state: State,
    pub(crate) micro_ops: MicroOpQueue,
    /// Current cycle within a multi-cycle micro-op (0-3 for bus ops).
    pub(crate) cycle: u8,

    // === Instruction decode state ===
    /// Start PC of current instruction (PC before opcode fetch).
    pub(crate) instr_start_pc: u32,
    /// True when executing a followup stage of a multi-stage instruction.
    pub(crate) in_followup: bool,
    /// Followup tag for multi-stage decode (instruction-specific).
    pub(crate) followup_tag: u8,

    // === Temporary storage (used during instruction execution) ===
    pub(crate) addr: u32,
    pub(crate) data: u32,
    pub(crate) size: Size,
    pub(crate) addr2: u32,
    pub(crate) data2: u32,

    // === Exception state ===
    pub(crate) exc: ExceptionState,
    pub(crate) exception_pc_override: Option<u32>,
    pub(crate) program_space_access: bool,

    // === Address error tracking ===
    /// Number of IRC words consumed during current instruction.
    pub(crate) irc_consumed_count: u8,
    /// True if the MOVE source operand is from memory (not register/immediate).
    pub(crate) move_src_was_memory: bool,
    /// For source (An)+: register and increment to undo if read AE fires.
    /// The 68000 doesn't apply the post-increment when the read faults.
    pub(crate) src_postinc_undo: Option<(u8, u32)>,
    /// For source -(An): register and original value to restore if read AE fires.
    /// The 68000 undoes the predecrement when the read faults.
    pub(crate) src_predec_undo: Option<(u8, u32)>,
    /// For destination (An)+/-(An): register and original value to restore if
    /// write AE fires. The 68000 doesn't commit the register change when the
    /// destination write faults.
    pub(crate) dst_reg_undo: Option<(u8, u32)>,
    /// SR value saved before set_flags_move for write AE undo.
    /// When a write AE fires, the pushed SR should reflect the pre-instruction state.
    /// Used for (An)/(An)+/-(An) destinations where no flag evaluation occurs.
    pub(crate) pre_move_sr: Option<u16>,
    /// SR value saved before set_flags_move for partial V,C restoration on write AE.
    /// Used for d16(An)/d8(An,Xn)/abs destinations where N,Z are evaluated during
    /// the extension word fetch, but V,C clearing happens during the write cycle.
    /// On write AE, N,Z stay computed but V,C revert to pre-instruction values.
    pub(crate) pre_move_vc: Option<u16>,
    /// For UNLK: original SP value to restore if AE fires.
    /// UNLK does A7 ← An before reading from the new (potentially odd) A7.
    /// If the read faults, the real 68000 undoes the A7 modification.
    pub(crate) sp_undo: Option<(bool, u32)>, // (was_supervisor, original_sp)

    /// Deferred FetchIRC count for MOVE destination ext words.
    /// On the real 68000, destination ext word FetchIRCs happen AFTER the source
    /// read, not before. For read AE, they don't happen at all.
    pub(crate) deferred_fetch_count: u8,
    /// Deferred Internal(2) for destination index EA calculation.
    pub(crate) deferred_index: bool,
    /// True if AbsLong destination's second word still needs to be consumed.
    /// First word was consumed (deferred) during decode, second deferred to writeback.
    pub(crate) abslong_pending: bool,
    /// True when the current read is from a -(An) long predecrement.
    /// The real 68000 decrements by 2 first and reads the low word at An-2.
    /// If An-2 is odd, the AE reports fault address = An-2 (not An-4).
    /// Our code decrements by 4 at once, so we need to add 2 to the fault addr.
    pub(crate) predec_long_read: bool,
    /// True when the AE was triggered by a FetchIRC (instruction fetch at odd PC).
    /// Used by compute_ae_frame_pc_non_move to return self.regs.pc (the target)
    /// instead of computing ISP + offset. Set in check_address_error, consumed by
    /// address_error → compute_ae_frame_pc_non_move.
    pub(crate) ae_from_fetch_irc: bool,
    /// For JSR/BSR: original stack pointer before pushing the return address.
    /// When FetchIRC at an odd target triggers AE, the real 68000 undoes the
    /// push (restores SP to pre-push value). AE frame is then pushed relative
    /// to SSP (after switching to supervisor mode), so total SSP change is -14.
    /// Stores (was_supervisor, original_sp_value).
    pub(crate) jsr_push_undo: Option<(bool, u32)>,
    /// For DBcc: original Dn.w value before the decrement.
    /// When the branch to an odd target triggers AE, the real 68000 undoes
    /// the Dn decrement. Stores (register_index, original_word_value).
    pub(crate) dbcc_dn_undo: Option<(u8, u16)>,

    // === Group 0 exception state ===
    /// True while processing a group 0 exception (address/bus error).
    /// A second group 0 fault during this period causes a double bus fault → halt.
    pub(crate) processing_group0: bool,

    // === Trace state ===
    /// True if the instruction that just completed had T set in SR at its start.
    /// Checked in start_next_instruction; if set, fires a trace exception (vector 9).
    pub(crate) trace_pending: bool,
    /// Diagnostic: number of illegal-instruction logs emitted.
    #[cfg(debug_assertions)]
    illegal_trace_count: u32,

    // === Interrupt state ===
    int_pending: u8,

    // === Timing ===
    pub(crate) total_cycles: Ticks,
    wait_cycles: u8,
}

impl Cpu68000 {
    /// Create a new CPU in reset state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            ir: 0,
            irc: 0,
            irc_addr: 0,
            state: State::Running,
            micro_ops: MicroOpQueue::new(),
            cycle: 0,
            instr_start_pc: 0,
            in_followup: false,
            followup_tag: 0,
            addr: 0,
            data: 0,
            size: Size::Word,
            addr2: 0,
            data2: 0,
            exc: ExceptionState::default(),
            exception_pc_override: None,
            program_space_access: false,
            irc_consumed_count: 0,
            move_src_was_memory: false,
            src_postinc_undo: None,
            src_predec_undo: None,
            dst_reg_undo: None,
            pre_move_sr: None,
            pre_move_vc: None,
            deferred_fetch_count: 0,
            sp_undo: None,
            deferred_index: false,
            abslong_pending: false,
            predec_long_read: false,
            ae_from_fetch_irc: false,
            jsr_push_undo: None,
            dbcc_dn_undo: None,
            processing_group0: false,
            trace_pending: false,
            #[cfg(debug_assertions)]
            illegal_trace_count: 0,
            int_pending: 0,
            total_cycles: Ticks::ZERO,
            wait_cycles: 0,
        }
    }

    /// Get total elapsed cycles.
    #[must_use]
    pub const fn total_cycles(&self) -> Ticks {
        self.total_cycles
    }

    /// Get a reference to the register set.
    #[must_use]
    pub fn registers(&self) -> &Registers {
        &self.regs
    }

    /// Get a mutable reference to the register set.
    pub fn registers_mut(&mut self) -> &mut Registers {
        &mut self.regs
    }

    /// Check if the CPU is halted (double bus fault).
    #[must_use]
    pub fn is_halted(&self) -> bool {
        matches!(self.state, State::Halted)
    }

    /// Check if the CPU is stopped (STOP instruction, waiting for interrupt).
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, State::Stopped)
    }

    /// Get the current Instruction Register (opcode being executed).
    #[must_use]
    pub const fn ir(&self) -> u16 {
        self.ir
    }

    /// Get the current Instruction Register Cache (next prefetched word).
    #[must_use]
    pub const fn irc(&self) -> u16 {
        self.irc
    }

    /// Set the interrupt priority level (0-7). 0 means no interrupt.
    pub fn set_ipl(&mut self, level: u8) {
        self.int_pending = level & 7;
    }

    /// Set up prefetch state for single-step testing.
    ///
    /// In the DL test format, `state.pc` points past the opcode + IRC.
    /// `opcode` goes into IR, `irc` goes into IRC.
    /// PC is already set in `self.regs.pc` and points to where the next
    /// fetch would come from (after IRC).
    pub fn setup_prefetch(&mut self, opcode: u16, irc: u16) {
        self.ir = opcode;
        self.irc = irc;
        // IRC was fetched from PC-2 (the word before current PC)
        self.irc_addr = self.regs.pc.wrapping_sub(2);
        // Instruction started at PC-4 (before opcode and IRC fetches)
        self.instr_start_pc = self.regs.pc.wrapping_sub(4);
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::Execute);
        self.cycle = 0;
        self.in_followup = false;
        self.followup_tag = 0;
        self.processing_group0 = false;
        self.state = State::Running;
    }

    /// Consume IRC as an extension word and queue FetchIRC to refill.
    ///
    /// Returns the value that was in IRC. Queues a 4-cycle FetchIRC
    /// to read the next word from memory at PC into IRC.
    pub(crate) fn consume_irc(&mut self) -> u16 {
        let value = self.irc;
        self.irc_consumed_count += 1;
        // Queue the refill — will read from current PC
        self.micro_ops.push_front(MicroOp::FetchIRC);
        value
    }

    /// Consume IRC as an extension word WITHOUT queuing FetchIRC.
    ///
    /// Used for MOVE destination extension words when the source is memory.
    /// The FetchIRC is deferred to after the source read, because the real
    /// 68000 doesn't execute destination FetchIRCs before the source bus access.
    pub(crate) fn consume_irc_deferred(&mut self) -> u16 {
        let value = self.irc;
        self.irc_consumed_count += 1;
        self.deferred_fetch_count += 1;
        value
    }

    /// Read IRC without consuming it (peek).
    #[must_use]
    pub(crate) fn peek_irc(&self) -> u16 {
        self.irc
    }

    /// Read the current data register (temporary storage used during execution).
    /// Useful for testing to see what data was loaded by an instruction.
    #[must_use]
    pub fn current_data(&self) -> u32 {
        self.data
    }

    /// Return a debug snapshot of internal CPU state for diagnostics.
    ///
    /// Exposes `pub(crate)` fields that integration tests cannot read directly:
    /// IR, in_followup, followup_tag, and the current micro-op queue contents.
    #[must_use]
    pub fn debug_state(&self) -> String {
        format!(
            "ir=0x{:04X} in_followup={} followup_tag={} micro_ops={}",
            self.ir, self.in_followup, self.followup_tag,
            self.micro_ops.debug_contents()
        )
    }

    /// Start executing the next instruction (or handle a pending interrupt).
    ///
    /// Called when the micro-op queue is empty: IR <- IRC, queue FetchIRC + Execute.
    /// If an interrupt is pending and its priority exceeds the mask, the interrupt
    /// exception is queued instead of the next instruction.
    fn start_next_instruction<B: M68kBus>(&mut self, bus: &mut B) {
        // Promote IR ← IRC, start new instruction
        self.ir = self.irc;
        self.instr_start_pc = self.irc_addr;
        self.in_followup = false;
        self.followup_tag = 0;

        // Clear AE tracking state — the previous instruction completed normally
        self.irc_consumed_count = 0;
        self.deferred_fetch_count = 0;
        self.jsr_push_undo = None;
        self.dbcc_dn_undo = None;
        self.src_postinc_undo = None;
        self.src_predec_undo = None;

        // Check for pending interrupt before executing the next instruction.
        // On the real 68000, interrupts are sampled at instruction boundaries.
        if self.accept_interrupt(bus) {
            return;
        }

        // Queue: FetchIRC (to refill IRC from PC), then Execute (to decode new IR)
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Check for a pending interrupt and, if accepted, queue exception processing.
    ///
    /// Returns true if an interrupt was accepted and exception ops were queued.
    /// The interrupt acknowledge cycle calls `bus.interrupt_ack()` to get the
    /// vector number from the interrupting device.
    fn accept_interrupt<B: M68kBus>(&mut self, bus: &mut B) -> bool {
        let level = self.int_pending;
        if level == 0 {
            return false;
        }

        // Level 7 is NMI (always accepted), others must exceed the mask
        let mask = self.regs.interrupt_mask();
        if level < 7 && level <= mask {
            return false;
        }

        // Save old SR and return PC (address of next instruction)
        let old_sr = self.regs.sr;
        let return_pc = self.instr_start_pc;

        // Enter supervisor mode, clear trace, set interrupt mask to accepted level
        self.regs.sr |= 0x2000;
        self.regs.sr &= !0x8000;
        self.regs.set_interrupt_mask(level);

        // Acknowledge interrupt — get vector number from bus
        let vector = bus.interrupt_ack(level);

        self.exc = ExceptionState {
            old_sr,
            return_pc,
            vector_addr: u32::from(vector) * 4,
            is_group0: false,
            ..Default::default()
        };

        self.micro_ops.clear();

        // Interrupt processing: ~44 cycles total.
        // Internal(10) accounts for the acknowledge cycle + internal processing.
        self.micro_ops.push(MicroOp::Internal(10));

        // Push return PC
        self.data = return_pc;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);

        // Continue via standard exception followup (push SR, read vector, jump)
        self.in_followup = true;
        self.followup_tag = 0xFE;
        self.micro_ops.push(MicroOp::Execute);

        true
    }

    // === Tick engine ===

    /// Advance the CPU by one clock cycle.
    ///
    /// One tick = one CPU clock (7.09 MHz PAL Amiga). Each tick:
    /// 1. Burn wait cycles from bus contention (if any)
    /// 2. Process all leading instant ops (Execute, Internal(0))
    /// 3. If queue is empty, call start_next_instruction() and loop to step 2
    /// 4. Process one cycle of the current timed op
    /// 5. If the timed op completed, process trailing instant ops
    pub fn tick<B: M68kBus>(&mut self, bus: &mut B) {
        self.total_cycles += Ticks::new(1);

        // Halted or stopped — do nothing
        match self.state {
            State::Halted => return,
            State::Stopped => {
                // Check for interrupt to wake from STOP
                if self.int_pending > self.regs.interrupt_mask()
                    || self.int_pending == 7
                {
                    self.state = State::Running;
                    // Will start processing on next tick
                } else {
                    return;
                }
            }
            State::Running => {}
        }

        // Step 1: Burn wait cycles
        if self.wait_cycles > 0 {
            self.wait_cycles -= 1;
            return;
        }

        // Step 2: Process all leading instant ops
        self.process_instant_ops(bus);

        // Step 3: If queue is empty after instant ops, start next instruction
        // (or handle a pending interrupt at the instruction boundary)
        if self.micro_ops.is_empty() {
            self.start_next_instruction(bus);
            // Process the instant ops that follow (Execute at the end)
            self.process_instant_ops(bus);
        }

        // Step 4: Process one cycle of the current timed op
        if let Some(op) = self.micro_ops.front()
            && !op.is_instant()
        {
            // Check for address error on first cycle of word/long bus ops
            if self.cycle == 0 {
                if self.check_address_error(op, bus) {
                    return; // Address error triggered, exception queued
                }
                // Clear predec_long_read after successful AE check on ReadLongHi
                if matches!(op, MicroOp::ReadLongHi) {
                    self.predec_long_read = false;
                }
            }

            self.cycle += 1;
            let target_cycles = op.cycles();

            if self.cycle >= target_cycles {
                // Op completed — execute its side effects
                let completed_op = self.micro_ops.pop().expect("queue not empty");
                self.execute_bus_op(completed_op, bus);
                self.cycle = 0;

                // Step 5: Process trailing instant ops when in a multi-stage
                // instruction or exception. Execute runs within the final cycle
                // of a bus operation (matching real 68000 behavior). But NOT when
                // the queue contains the start-next-instruction Execute (in_followup
                // is false at instruction boundaries).
                if self.in_followup {
                    self.process_instant_ops(bus);
                }
            }
        }
    }

    /// Process all instant ops at the front of the queue.
    fn process_instant_ops<B: M68kBus>(&mut self, bus: &mut B) {
        // Safety limit to prevent infinite loops from buggy instruction handlers
        let mut limit = 16;
        while limit > 0 {
            match self.micro_ops.front() {
                Some(op) if op.is_instant() => {
                    let op = self.micro_ops.pop().expect("queue not empty");
                    self.execute_instant_op(op, bus);
                    limit -= 1;
                }
                _ => break,
            }
        }
    }

    /// Execute an instant (0-cycle) micro-op.
    fn execute_instant_op<B: M68kBus>(&mut self, op: MicroOp, bus: &mut B) {
        match op {
            MicroOp::AssertReset => {
                bus.reset();
            }
            MicroOp::Execute => {
                self.decode_and_execute();
            }
            MicroOp::Internal(0) => {
                // No-op, instant
            }
            _ => {
                // Should not reach here — non-instant ops handled in execute_bus_op
                debug_assert!(false, "Non-instant op {:?} in execute_instant_op", op);
            }
        }
    }

    /// Execute a completed bus/timed micro-op (called when all cycles are consumed).
    fn execute_bus_op<B: M68kBus>(&mut self, op: MicroOp, bus: &mut B) {
        match op {
            MicroOp::FetchIRC => {
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), true);
                let read_addr = self.regs.pc & 0x00FF_FFFE;
                let result = bus.read_word(read_addr, fc);
                if result.bus_error {
                    self.program_space_access = true;
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.irc = result.data;
                self.irc_addr = self.regs.pc;
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            MicroOp::ReadByte => {
                let fc = self.data_fc();
                let read_addr = self.addr & 0x00FF_FFFF;
                let result = bus.read_byte(read_addr, fc);
                if result.bus_error {
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data & 0xFF);
            }
            MicroOp::ReadWord => {
                let fc = self.data_fc();
                let read_addr = self.addr & 0x00FF_FFFE;
                let result = bus.read_word(read_addr, fc);
                if result.bus_error {
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data);
            }
            MicroOp::ReadLongHi => {
                let fc = self.data_fc();
                let read_addr = self.addr & 0x00FF_FFFE;
                let result = bus.read_word(read_addr, fc);
                if result.bus_error {
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data) << 16;
            }
            MicroOp::ReadLongLo => {
                let fc = self.data_fc();
                let addr = self.addr.wrapping_add(2);
                let read_addr = addr & 0x00FF_FFFE;
                let result = bus.read_word(read_addr, fc);
                if result.bus_error {
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.data |= u32::from(result.data);
            }
            MicroOp::WriteByte => {
                let fc = self.data_fc();
                let write_addr = self.addr & 0x00FF_FFFF;
                let result = bus.write_byte(write_addr, self.data as u8, fc);
                if result.bus_error {
                    self.bus_error_exception(write_addr, false);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::WriteWord => {
                let fc = self.data_fc();
                let write_addr = self.addr & 0x00FF_FFFE;
                let result = bus.write_word(write_addr, self.data as u16, fc);
                if result.bus_error {
                    self.bus_error_exception(write_addr, false);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::WriteLongHi => {
                let fc = self.data_fc();
                let write_addr = self.addr & 0x00FF_FFFE;
                let result = bus.write_word(
                    write_addr,
                    (self.data >> 16) as u16,
                    fc,
                );
                if result.bus_error {
                    self.bus_error_exception(write_addr, false);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::WriteLongLo => {
                let fc = self.data_fc();
                let addr = self.addr.wrapping_add(2);
                let write_addr = addr & 0x00FF_FFFE;
                let result = bus.write_word(
                    write_addr,
                    (self.data & 0xFFFF) as u16,
                    fc,
                );
                if result.bus_error {
                    self.bus_error_exception(write_addr, false);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PushWord => {
                let sp_addr = self.regs.push_word();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let write_addr = sp_addr & 0x00FF_FFFE;
                let result = bus.write_word(write_addr, self.data as u16, fc);
                if result.bus_error {
                    self.bus_error_exception(write_addr, false);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PushLongHi => {
                let sp_addr = self.regs.push_long();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let write_addr = sp_addr & 0x00FF_FFFE;
                let result = bus.write_word(
                    write_addr,
                    (self.data >> 16) as u16,
                    fc,
                );
                if result.bus_error {
                    self.bus_error_exception(write_addr, false);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PushLongLo => {
                let sp_addr = self.regs.active_sp().wrapping_add(2);
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let write_addr = sp_addr & 0x00FF_FFFE;
                let result = bus.write_word(
                    write_addr,
                    (self.data & 0xFFFF) as u16,
                    fc,
                );
                if result.bus_error {
                    self.bus_error_exception(write_addr, false);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PopWord => {
                let sp_addr = self.regs.active_sp();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let read_addr = sp_addr & 0x00FF_FFFE;
                let result = bus.read_word(read_addr, fc);
                if result.bus_error {
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data);
                self.regs.pop_word();
            }
            MicroOp::PopLongHi => {
                let sp_addr = self.regs.active_sp();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let read_addr = sp_addr & 0x00FF_FFFE;
                let result = bus.read_word(read_addr, fc);
                if result.bus_error {
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data) << 16;
            }
            MicroOp::PopLongLo => {
                let sp_addr = self.regs.active_sp().wrapping_add(2);
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let read_addr = sp_addr & 0x00FF_FFFE;
                let result = bus.read_word(read_addr, fc);
                if result.bus_error {
                    self.bus_error_exception(read_addr, true);
                    return;
                }
                self.wait_cycles = result.wait_cycles;
                self.data |= u32::from(result.data);
                self.regs.pop_long();
            }
            MicroOp::Internal(n) if n > 0 => {
                // Internal processing completed — nothing to do
            }
            _ => {}
        }
    }

    // === Helper methods ===

    /// Get the function code for data accesses.
    fn data_fc(&self) -> FunctionCode {
        if self.program_space_access {
            FunctionCode::from_flags(self.regs.is_supervisor(), true)
        } else {
            FunctionCode::from_flags(self.regs.is_supervisor(), false)
        }
    }

    /// Queue prefetch refill for branch/jump target.
    ///
    /// After setting PC to target+2, this queues a FetchIRC to load the
    /// target word into IRC. When start_next_instruction runs, it will
    /// transfer IRC to IR and queue another FetchIRC for the word after.
    pub(crate) fn refill_prefetch_branch(&mut self) {
        self.micro_ops.push(MicroOp::FetchIRC);
    }

    /// Queue internal processing cycles.
    pub(crate) fn queue_internal(&mut self, cycles: u8) {
        if cycles > 0 {
            self.micro_ops.push(MicroOp::Internal(cycles));
        }
    }

    /// Read value from data register masked to size.
    #[must_use]
    pub(crate) fn read_data_reg(&self, r: u8, size: Size) -> u32 {
        let val = self.regs.d[r as usize];
        match size {
            Size::Byte => val & 0xFF,
            Size::Word => val & 0xFFFF,
            Size::Long => val,
        }
    }

    /// Write value to data register based on size (preserves upper bits).
    pub(crate) fn write_data_reg(&mut self, r: u8, value: u32, size: Size) {
        let reg = &mut self.regs.d[r as usize];
        *reg = match size {
            Size::Byte => (*reg & 0xFFFF_FF00) | (value & 0xFF),
            Size::Word => (*reg & 0xFFFF_0000) | (value & 0xFFFF),
            Size::Long => value,
        };
    }

    /// Set MOVE-style flags: clear V and C, update N and Z.
    pub(crate) fn set_flags_move(&mut self, value: u32, size: Size) {
        self.regs.sr = Status::clear_vc(self.regs.sr);
        self.regs.sr = match size {
            Size::Byte => Status::update_nz_byte(self.regs.sr, value as u8),
            Size::Word => Status::update_nz_word(self.regs.sr, value as u16),
            Size::Long => Status::update_nz_long(self.regs.sr, value),
        };
    }

    /// Trigger an illegal instruction exception.
    pub(crate) fn illegal_instruction(&mut self) {
        #[cfg(debug_assertions)]
        if self.illegal_trace_count < 128 {
            eprintln!(
                "  M68K ILLEGAL: opcode=${:04X} instr_start=${:08X} pc=${:08X} irc=${:04X}",
                self.ir,
                self.instr_start_pc,
                self.regs.pc,
                self.irc
            );
            self.illegal_trace_count += 1;
        }
        self.exception(4, 0);
    }

    /// Queue read micro-ops based on operation size. Reads from self.addr.
    pub(crate) fn queue_read_ops(&mut self, size: Size) {
        match size {
            Size::Byte => self.micro_ops.push(MicroOp::ReadByte),
            Size::Word => self.micro_ops.push(MicroOp::ReadWord),
            Size::Long => {
                self.micro_ops.push(MicroOp::ReadLongHi);
                self.micro_ops.push(MicroOp::ReadLongLo);
            }
        }
    }

    /// Queue write micro-ops based on operation size. Writes self.data to self.addr.
    pub(crate) fn queue_write_ops(&mut self, size: Size) {
        match size {
            Size::Byte => self.micro_ops.push(MicroOp::WriteByte),
            Size::Word => self.micro_ops.push(MicroOp::WriteWord),
            Size::Long => {
                self.micro_ops.push(MicroOp::WriteLongHi);
                self.micro_ops.push(MicroOp::WriteLongLo);
            }
        }
    }

    /// Trigger a privilege violation exception.
    pub(crate) fn privilege_violation(&mut self) {
        self.exception(8, 0);
    }

    /// Check supervisor mode. If in user mode, trigger privilege violation
    /// and return true (caller should return). Returns false if OK.
    pub(crate) fn check_supervisor(&mut self) -> bool {
        if self.regs.sr & 0x2000 == 0 {
            self.privilege_violation();
            true
        } else {
            false
        }
    }

    /// Check if the given bus op targets an odd address (word/long only).
    /// If so, trigger an address error and return true.
    fn check_address_error<B: M68kBus>(&mut self, op: MicroOp, _bus: &mut B) -> bool {
        let (check_addr, is_read) = match op {
            MicroOp::FetchIRC => (self.regs.pc, true),
            MicroOp::ReadWord | MicroOp::ReadLongHi => (self.addr, true),
            MicroOp::WriteWord | MicroOp::WriteLongHi => (self.addr, false),
            MicroOp::ReadLongLo => (self.addr.wrapping_add(2), true),
            MicroOp::WriteLongLo => (self.addr.wrapping_add(2), false),
            // Push/pop: check stack address for odd alignment
            MicroOp::PushWord => (self.regs.active_sp().wrapping_sub(2), false),
            MicroOp::PushLongHi => (self.regs.active_sp().wrapping_sub(4), false),
            MicroOp::PushLongLo => (self.regs.active_sp().wrapping_add(2), false),
            MicroOp::PopWord | MicroOp::PopLongHi => (self.regs.active_sp(), true),
            MicroOp::PopLongLo => (self.regs.active_sp().wrapping_add(2), true),
            _ => return false, // Byte ops, internal
        };

        if check_addr & 1 == 0 {
            return false; // Even address — no error
        }

        // Double bus fault: an address error during group 0 exception processing
        // causes the CPU to halt. No frame is pushed.
        if self.processing_group0 {
            self.state = State::Halted;
            self.micro_ops.clear();
            self.cycle = 0;
            return true;
        }

        // FetchIRC is a program-space access (instruction fetch)
        if matches!(op, MicroOp::FetchIRC) {
            self.program_space_access = true;
            self.ae_from_fetch_irc = true;
        }

        // For -(An) long predecrement: the real 68000 decrements by 2 first
        // and attempts the low word at An-2. Our code decrements by 4 at once,
        // so ReadLongHi/WriteLongHi fires at An-4. Adjust fault address by +2.
        let fault_addr = if self.predec_long_read
            && matches!(op, MicroOp::ReadLongHi | MicroOp::WriteLongHi)
        {
            check_addr.wrapping_add(2)
        } else {
            check_addr
        };
        self.predec_long_read = false;

        // Odd address detected — trigger address error
        self.micro_ops.clear();
        self.cycle = 0;
        self.address_error(fault_addr, is_read);
        true
    }
}

impl Default for Cpu68000 {
    fn default() -> Self {
        Self::new()
    }
}

// === Observable implementation ===

const M68000_QUERY_PATHS: &[&str] = &[
    "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7",
    "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7",
    "usp", "ssp",
    "pc",
    "sr", "ccr",
    "flags.x", "flags.n", "flags.z", "flags.v", "flags.c",
    "flags.s", "flags.t",
    "int_mask",
    "halted", "stopped", "cycles",
    "opcode",
];

impl Observable for Cpu68000 {
    fn query(&self, path: &str) -> Option<Value> {
        match path {
            "d0" => Some(self.regs.d[0].into()),
            "d1" => Some(self.regs.d[1].into()),
            "d2" => Some(self.regs.d[2].into()),
            "d3" => Some(self.regs.d[3].into()),
            "d4" => Some(self.regs.d[4].into()),
            "d5" => Some(self.regs.d[5].into()),
            "d6" => Some(self.regs.d[6].into()),
            "d7" => Some(self.regs.d[7].into()),
            "a0" => Some(self.regs.a(0).into()),
            "a1" => Some(self.regs.a(1).into()),
            "a2" => Some(self.regs.a(2).into()),
            "a3" => Some(self.regs.a(3).into()),
            "a4" => Some(self.regs.a(4).into()),
            "a5" => Some(self.regs.a(5).into()),
            "a6" => Some(self.regs.a(6).into()),
            "a7" => Some(self.regs.a(7).into()),
            "usp" => Some(self.regs.usp.into()),
            "ssp" => Some(self.regs.ssp.into()),
            "pc" => Some(self.regs.pc.into()),
            "sr" => Some(Value::U16(self.regs.sr)),
            "ccr" => Some(self.regs.ccr().into()),
            "flags.x" => Some((self.regs.sr & X != 0).into()),
            "flags.n" => Some((self.regs.sr & N != 0).into()),
            "flags.z" => Some((self.regs.sr & Z != 0).into()),
            "flags.v" => Some((self.regs.sr & V != 0).into()),
            "flags.c" => Some((self.regs.sr & C != 0).into()),
            "flags.s" => Some(self.regs.is_supervisor().into()),
            "flags.t" => Some(self.regs.is_trace().into()),
            "int_mask" => Some(self.regs.interrupt_mask().into()),
            "halted" => Some(matches!(self.state, State::Halted).into()),
            "stopped" => Some(matches!(self.state, State::Stopped).into()),
            "cycles" => Some(self.total_cycles.get().into()),
            "opcode" => Some(Value::U16(self.ir)),
            _ => None,
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        M68000_QUERY_PATHS
    }
}
