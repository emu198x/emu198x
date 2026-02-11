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
    pub(crate) pending_exception: Option<u8>,
    pub(crate) current_exception: Option<u8>,
    pub(crate) fault_addr: u32,
    pub(crate) fault_read: bool,
    pub(crate) fault_in_instruction: bool,
    pub(crate) fault_fc: u8,
    pub(crate) group0_access_info: u16,
    pub(crate) exception_pc_override: Option<u32>,
    pub(crate) program_space_access: bool,

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
            pending_exception: None,
            current_exception: None,
            fault_addr: 0,
            fault_read: true,
            fault_in_instruction: false,
            fault_fc: 0,
            group0_access_info: 0,
            exception_pc_override: None,
            program_space_access: false,
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
        self.state = State::Running;
    }

    /// Consume IRC as an extension word and queue FetchIRC to refill.
    ///
    /// Returns the value that was in IRC. Queues a 4-cycle FetchIRC
    /// to read the next word from memory at PC into IRC.
    pub(crate) fn consume_irc(&mut self) -> u16 {
        let value = self.irc;
        // Queue the refill — will read from current PC
        self.micro_ops.push_front(MicroOp::FetchIRC);
        value
    }

    /// Read IRC without consuming it (peek).
    #[must_use]
    pub(crate) fn peek_irc(&self) -> u16 {
        self.irc
    }

    /// Start executing the next instruction.
    ///
    /// Called when the micro-op queue is empty: IR <- IRC, queue FetchIRC + Execute.
    fn start_next_instruction(&mut self) {
        self.ir = self.irc;
        self.instr_start_pc = self.irc_addr;
        self.in_followup = false;
        self.followup_tag = 0;
        // Queue: FetchIRC (to refill IRC from PC), then Execute (to decode new IR)
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
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
        if self.micro_ops.is_empty() {
            self.start_next_instruction();
            // Process the instant ops that follow (Execute at the end)
            self.process_instant_ops(bus);
        }

        // Step 4: Process one cycle of the current timed op
        if let Some(op) = self.micro_ops.front()
            && !op.is_instant()
        {
            self.cycle += 1;
            let target_cycles = op.cycles();

            if self.cycle >= target_cycles {
                // Op completed — execute its side effects
                let completed_op = self.micro_ops.pop().expect("queue not empty");
                self.execute_bus_op(completed_op, bus);
                self.cycle = 0;

                // No trailing instant ops — they'll be processed as leading
                // instant ops on the next tick. This prevents the next
                // instruction's Execute from firing within the current
                // instruction's cycle budget.
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
    fn execute_instant_op<B: M68kBus>(&mut self, op: MicroOp, _bus: &mut B) {
        match op {
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
                let result = bus.read_word(self.regs.pc & 0x00FF_FFFE, fc);
                self.wait_cycles = result.wait_cycles;
                self.irc = result.data;
                self.irc_addr = self.regs.pc;
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            MicroOp::ReadByte => {
                let fc = self.data_fc();
                let result = bus.read_byte(self.addr & 0x00FF_FFFF, fc);
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data & 0xFF);
            }
            MicroOp::ReadWord => {
                let fc = self.data_fc();
                let result = bus.read_word(self.addr & 0x00FF_FFFE, fc);
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data);
            }
            MicroOp::ReadLongHi => {
                let fc = self.data_fc();
                let result = bus.read_word(self.addr & 0x00FF_FFFE, fc);
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data) << 16;
            }
            MicroOp::ReadLongLo => {
                let fc = self.data_fc();
                let addr = self.addr.wrapping_add(2);
                let result = bus.read_word(addr & 0x00FF_FFFE, fc);
                self.wait_cycles = result.wait_cycles;
                self.data |= u32::from(result.data);
            }
            MicroOp::WriteByte => {
                let fc = self.data_fc();
                let result = bus.write_byte(
                    self.addr & 0x00FF_FFFF,
                    self.data as u8,
                    fc,
                );
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::WriteWord => {
                let fc = self.data_fc();
                let result = bus.write_word(
                    self.addr & 0x00FF_FFFE,
                    self.data as u16,
                    fc,
                );
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::WriteLongHi => {
                let fc = self.data_fc();
                let result = bus.write_word(
                    self.addr & 0x00FF_FFFE,
                    (self.data >> 16) as u16,
                    fc,
                );
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::WriteLongLo => {
                let fc = self.data_fc();
                let addr = self.addr.wrapping_add(2);
                let result = bus.write_word(
                    addr & 0x00FF_FFFE,
                    (self.data & 0xFFFF) as u16,
                    fc,
                );
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PushWord => {
                let sp_addr = self.regs.push_word();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let result = bus.write_word(
                    sp_addr & 0x00FF_FFFE,
                    self.data as u16,
                    fc,
                );
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PushLongHi => {
                let sp_addr = self.regs.push_long();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let result = bus.write_word(
                    sp_addr & 0x00FF_FFFE,
                    (self.data >> 16) as u16,
                    fc,
                );
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PushLongLo => {
                let sp_addr = self.regs.active_sp().wrapping_add(2);
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let result = bus.write_word(
                    sp_addr & 0x00FF_FFFE,
                    (self.data & 0xFFFF) as u16,
                    fc,
                );
                self.wait_cycles = result.wait_cycles;
            }
            MicroOp::PopWord => {
                let sp_addr = self.regs.active_sp();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let result = bus.read_word(sp_addr & 0x00FF_FFFE, fc);
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data);
                self.regs.pop_word();
            }
            MicroOp::PopLongHi => {
                let sp_addr = self.regs.active_sp();
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let result = bus.read_word(sp_addr & 0x00FF_FFFE, fc);
                self.wait_cycles = result.wait_cycles;
                self.data = u32::from(result.data) << 16;
            }
            MicroOp::PopLongLo => {
                let sp_addr = self.regs.active_sp().wrapping_add(2);
                let fc = FunctionCode::from_flags(self.regs.is_supervisor(), false);
                let result = bus.read_word(sp_addr & 0x00FF_FFFE, fc);
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
        self.exception(4);
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
        self.exception(8);
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
