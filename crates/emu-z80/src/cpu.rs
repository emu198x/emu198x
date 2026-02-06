//! Z80 CPU core with per-T-state execution.

#![allow(clippy::cast_possible_truncation)] // Intentional truncation for low byte extraction.
#![allow(clippy::cast_possible_wrap)] // Intentional i8 casts for displacements.
#![allow(clippy::struct_excessive_bools)] // CPU state requires multiple boolean flags.
#![allow(dead_code)] // Helper functions will be used as more instructions are implemented.

use emu_core::{Bus, Cpu, Observable, Ticks, Value};

use crate::flags::{CF, HF, NF, PF, SF, XF, YF, ZF};
use crate::microcode::{MicroOp, MicroOpQueue};
use crate::registers::Registers;

/// Z80 CPU.
///
/// The CPU does not own the bus. Instead, the bus is passed to `tick()` on
/// each T-state. This allows the bus to be shared with other components
/// (e.g., ULA) that may also need bus access or inject wait states.
pub struct Z80 {
    // === Registers ===
    /// Main register set.
    pub(crate) regs: Registers,

    // === Execution state ===
    /// Queue of micro-operations for current instruction.
    micro_ops: MicroOpQueue,
    /// T-state counter within current micro-op.
    t_state: u8,
    /// Total T-states for current micro-op (for Internal ops).
    t_total: u8,
    /// Pending wait states from memory contention.
    wait_states: u8,

    // === Instruction decode state ===
    /// Current opcode being executed.
    opcode: u8,
    /// Prefix state (0 = none, 0xCB, 0xDD, 0xED, 0xFD).
    prefix: u8,
    /// Second prefix for DDCB/FDCB.
    prefix2: u8,
    /// Displacement for IX+d / IY+d addressing.
    displacement: i8,
    /// Temporary address register.
    addr: u16,
    /// Temporary data (low byte).
    data_lo: u8,
    /// Temporary data (high byte).
    data_hi: u8,
    /// True if next Execute should use followup logic.
    in_followup: bool,
    /// Followup stage counter for multi-stage instructions (1, 2, 3...).
    followup_stage: u8,
    /// True if we're handling IM2 interrupt and need to set PC from vector.
    im2_pending: bool,

    // === Interrupt state ===
    /// Pending interrupt request.
    int_pending: bool,
    /// Pending NMI request.
    nmi_pending: bool,
    /// NMI edge detector (to detect rising edge).
    nmi_last: bool,

    // === Timing ===
    /// Total T-states elapsed.
    total_ticks: Ticks,
}

impl Z80 {
    /// Create a new Z80.
    #[must_use]
    pub fn new() -> Self {
        let mut cpu = Self {
            regs: Registers::default(),
            micro_ops: MicroOpQueue::new(),
            t_state: 0,
            t_total: 0,
            wait_states: 0,
            opcode: 0,
            prefix: 0,
            prefix2: 0,
            displacement: 0,
            addr: 0,
            data_lo: 0,
            data_hi: 0,
            in_followup: false,
            followup_stage: 0,
            im2_pending: false,
            int_pending: false,
            nmi_pending: false,
            nmi_last: false,
            total_ticks: Ticks::ZERO,
        };
        // Start with a fetch
        cpu.micro_ops.push(MicroOp::FetchOpcode);
        cpu
    }

    /// Total T-states elapsed since creation.
    #[must_use]
    pub const fn total_ticks(&self) -> Ticks {
        self.total_ticks
    }

    /// Read byte from memory, accumulating any wait states.
    fn read<B: Bus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        let result = bus.read(u32::from(addr));
        self.wait_states = self.wait_states.saturating_add(result.wait);
        result.data
    }

    /// Write byte to memory, accumulating any wait states.
    fn write<B: Bus>(&mut self, bus: &mut B, addr: u16, value: u8) {
        let wait = bus.write(u32::from(addr), value);
        self.wait_states = self.wait_states.saturating_add(wait);
    }

    /// Read byte from I/O port, accumulating any wait states.
    fn io_read<B: Bus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        let result = bus.io_read(u32::from(addr));
        self.wait_states = self.wait_states.saturating_add(result.wait);
        result.data
    }

    /// Write byte to I/O port, accumulating any wait states.
    fn io_write<B: Bus>(&mut self, bus: &mut B, addr: u16, value: u8) {
        let wait = bus.io_write(u32::from(addr), value);
        self.wait_states = self.wait_states.saturating_add(wait);
    }
}

impl Default for Z80 {
    fn default() -> Self {
        Self::new()
    }
}

impl Z80 {
    /// Increment R register (lower 7 bits only).
    fn inc_r(&mut self) {
        self.regs.r = (self.regs.r & 0x80) | ((self.regs.r.wrapping_add(1)) & 0x7F);
    }

    /// Get the effective index register (IX or IY based on prefix).
    fn get_index_reg(&self) -> u16 {
        match self.prefix {
            0xDD => self.regs.ix,
            0xFD => self.regs.iy,
            _ => self.regs.hl(),
        }
    }

    /// Set the effective index register.
    fn set_index_reg(&mut self, value: u16) {
        match self.prefix {
            0xDD => self.regs.ix = value,
            0xFD => self.regs.iy = value,
            _ => self.regs.set_hl(value),
        }
    }

    /// Queue micro-ops for the next instruction fetch.
    fn queue_fetch(&mut self) {
        self.micro_ops.clear();
        self.prefix = 0;
        self.prefix2 = 0;
        self.in_followup = false;
        self.followup_stage = 0;
        self.micro_ops.push(MicroOp::FetchOpcode);
    }

    /// Force a return from a subroutine call.
    ///
    /// This pops the return address from the stack and sets PC, then
    /// clears the micro-op queue so the next tick starts fresh.
    /// Used by test harnesses to skip trap instructions after handling
    /// system calls (e.g., CP/M BDOS emulation).
    ///
    /// Only available in test builds.
    #[cfg(feature = "test-utils")]
    pub fn force_ret<B: Bus>(&mut self, bus: &mut B) {
        // Pop return address from stack (low byte first, then high)
        let sp = self.regs.sp;
        let lo = self.read(bus, sp);
        let hi = self.read(bus, sp.wrapping_add(1));
        self.regs.sp = sp.wrapping_add(2);

        // Set PC to return address
        let ret_addr = u16::from(lo) | (u16::from(hi) << 8);
        self.regs.pc = ret_addr;

        // Clear micro-op queue and reset state for next instruction
        self.queue_fetch();
        self.t_state = 0;
    }

    /// Set the program counter.
    ///
    /// Only available in test builds.
    #[cfg(feature = "test-utils")]
    pub fn set_pc(&mut self, value: u16) {
        self.regs.pc = value;
    }

    /// Set the stack pointer.
    ///
    /// Only available in test builds.
    #[cfg(feature = "test-utils")]
    pub fn set_sp(&mut self, value: u16) {
        self.regs.sp = value;
    }

    /// Get the C register.
    pub fn c(&self) -> u8 {
        self.regs.c
    }

    /// Get the E register.
    pub fn e(&self) -> u8 {
        self.regs.e
    }

    /// Get the DE register pair.
    pub fn de(&self) -> u16 {
        self.regs.de()
    }

    /// Get the stack pointer.
    pub fn sp(&self) -> u16 {
        self.regs.sp
    }

    /// Get the A register.
    #[cfg(feature = "test-utils")]
    pub fn a(&self) -> u8 {
        self.regs.a
    }

    /// Get the F register (flags).
    #[cfg(feature = "test-utils")]
    pub fn f(&self) -> u8 {
        self.regs.f
    }

    /// Get the BC register pair.
    #[cfg(feature = "test-utils")]
    pub fn bc(&self) -> u16 {
        self.regs.bc()
    }

    /// Get the HL register pair.
    #[cfg(feature = "test-utils")]
    pub fn hl(&self) -> u16 {
        self.regs.hl()
    }

    /// Get current micro-op for debugging.
    #[cfg(feature = "test-utils")]
    pub fn current_micro_op(&self) -> Option<MicroOp> {
        self.micro_ops.current()
    }

    /// Get t_state for debugging.
    #[cfg(feature = "test-utils")]
    pub fn t_state(&self) -> u8 {
        self.t_state
    }

    /// Get queue state for debugging.
    #[cfg(feature = "test-utils")]
    pub fn queue_state(&self) -> (u8, u8) {
        (self.micro_ops.pos(), self.micro_ops.len())
    }

    /// Simulate RET instruction - pop return address from stack and jump.
    ///
    /// Used by test harnesses to return from BDOS calls.
    /// Only available in test builds.
    #[cfg(feature = "test-utils")]
    pub fn ret<B: Bus>(&mut self, bus: &mut B) {
        let sp = self.regs.sp;
        let lo = self.read(bus, sp);
        let hi = self.read(bus, sp.wrapping_add(1));
        self.regs.sp = sp.wrapping_add(2);
        self.regs.pc = u16::from(lo) | (u16::from(hi) << 8);
        self.micro_ops.clear();
        self.queue_fetch();
        self.t_state = 0;
    }

    /// Execute one complete instruction.
    ///
    /// Returns the number of T-states consumed.
    /// Used by test harnesses that need instruction-level granularity.
    ///
    /// Only available in test builds.
    #[cfg(feature = "test-utils")]
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let mut ticks = 0u32;
        let max_ticks = 100; // Safety limit

        // Run at least one tick
        self.tick(bus);
        ticks += 1;

        // Continue until queue has only FetchOpcode waiting (ready for next instruction)
        while !(self.micro_ops.is_empty() ||
                (self.t_state == 0 && matches!(self.micro_ops.current(), Some(MicroOp::FetchOpcode)))) {
            self.tick(bus);
            ticks += 1;
            if ticks >= max_ticks {
                // This shouldn't happen - instruction taking too long
                break;
            }
        }

        ticks
    }

    /// Execute one T-state of CPU operation.
    fn tick_internal<B: Bus>(&mut self, bus: &mut B) {
        let Some(op) = self.micro_ops.current() else {
            // Queue empty - shouldn't happen, but queue a fetch
            self.queue_fetch();
            return;
        };

        match op {
            MicroOp::FetchOpcode => self.tick_fetch_opcode(bus),
            MicroOp::FetchDisplacement => self.tick_fetch_displacement(bus),
            MicroOp::ReadImm8 => self.tick_read_imm8(bus),
            MicroOp::ReadImm16Lo => self.tick_read_imm16_lo(bus),
            MicroOp::ReadImm16Hi => self.tick_read_imm16_hi(bus),
            MicroOp::ReadMem => self.tick_read_mem(bus),
            MicroOp::ReadMem16Lo => self.tick_read_mem16_lo(bus),
            MicroOp::ReadMem16Hi => self.tick_read_mem16_hi(bus),
            MicroOp::WriteMem => self.tick_write_mem(bus),
            MicroOp::WriteMem16Lo => self.tick_write_mem16_lo(bus),
            MicroOp::WriteMem16Hi => self.tick_write_mem16_hi(bus),
            MicroOp::WriteMemHiFirst => self.tick_write_mem_hi_first(bus),
            MicroOp::WriteMemLoSecond => self.tick_write_mem_lo_second(bus),
            MicroOp::IoRead => self.tick_io_read(bus),
            MicroOp::IoWrite => self.tick_io_write(bus),
            MicroOp::Internal => self.tick_internal_op(),
            MicroOp::Execute => {
                self.decode_and_execute();
                self.micro_ops.advance();
            }
        }
    }

    /// T-states for M1 (opcode fetch) cycle.
    #[allow(clippy::match_same_arms)]
    fn tick_fetch_opcode<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {
                // T1: PC on address bus, MREQ+RD asserted
            }
            1 => {
                // T2: Memory responds (WAIT sampling)
            }
            2 => {
                // T3: Data read from memory
                self.opcode = self.read(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.inc_r();
            }
            3 => {
                // T4: Refresh cycle, decode instruction
                self.t_state = 0;
                self.micro_ops.advance();

                // Check for prefix bytes
                if self.prefix == 0 {
                    match self.opcode {
                        0xCB | 0xDD | 0xED | 0xFD => {
                            self.prefix = self.opcode;
                            self.micro_ops.push(MicroOp::FetchOpcode);
                            return;
                        }
                        _ => {}
                    }
                } else if self.prefix == 0xDD || self.prefix == 0xFD {
                    // Check for DDCB or FDCB
                    if self.opcode == 0xCB {
                        self.prefix2 = 0xCB;
                        // DDCB/FDCB: fetch displacement, then opcode
                        self.micro_ops.push(MicroOp::FetchDisplacement);
                        self.micro_ops.push(MicroOp::FetchOpcode);
                        return;
                    }
                    // Check for prefix chains (DD DD, FD FD, DD FD, FD DD)
                    match self.opcode {
                        0xDD => {
                            self.prefix = 0xDD;
                            self.micro_ops.push(MicroOp::FetchOpcode);
                            return;
                        }
                        0xFD => {
                            self.prefix = 0xFD;
                            self.micro_ops.push(MicroOp::FetchOpcode);
                            return;
                        }
                        0xED => {
                            // ED after DD/FD cancels the prefix
                            self.prefix = 0xED;
                            self.micro_ops.push(MicroOp::FetchOpcode);
                            return;
                        }
                        _ => {}
                    }
                }

                // Queue execution
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for reading displacement byte (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_fetch_displacement<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {
                // T1: PC on address bus
            }
            1 => {
                // T2: Memory responds
            }
            2 => {
                // T3: Data read
                self.displacement = self.read(bus, self.regs.pc) as i8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for reading immediate byte (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_read_imm8<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.data_lo = self.read(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for reading immediate word low byte (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_read_imm16_lo<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.data_lo = self.read(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for reading immediate word high byte (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_read_imm16_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.data_hi = self.read(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for memory read (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_read_mem<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.data_lo = self.read(bus, self.addr);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for memory read low byte (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_read_mem16_lo<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.data_lo = self.read(bus, self.addr);
                self.addr = self.addr.wrapping_add(1);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for memory read high byte (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_read_mem16_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.data_hi = self.read(bus, self.addr);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for memory write (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_write_mem<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.write(bus, self.addr, self.data_lo);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for memory write low byte of word (3 T-states).
    /// Writes data_lo to addr, then increments addr for the high byte.
    #[allow(clippy::match_same_arms)]
    fn tick_write_mem16_lo<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.write(bus, self.addr, self.data_lo);
                self.addr = self.addr.wrapping_add(1);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for memory write high byte of word (3 T-states).
    /// Writes data_hi to addr.
    #[allow(clippy::match_same_arms)]
    fn tick_write_mem16_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {
                self.write(bus, self.addr, self.data_hi);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for PUSH high byte first (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_write_mem_hi_first<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
            }
            1 => {}
            2 => {
                self.write(bus, self.regs.sp, self.data_hi);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for PUSH low byte second (3 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_write_mem_lo_second<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
            }
            1 => {}
            2 => {
                self.write(bus, self.regs.sp, self.data_lo);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for I/O read (4 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_io_read<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {}
            3 => {
                self.data_lo = self.io_read(bus, self.addr);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// T-states for I/O write (4 T-states).
    #[allow(clippy::match_same_arms)]
    fn tick_io_write<B: Bus>(&mut self, bus: &mut B) {
        match self.t_state {
            0 => {}
            1 => {}
            2 => {}
            3 => {
                self.io_write(bus, self.addr, self.data_lo);
                self.t_state = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// Internal operation - burns T-states.
    fn tick_internal_op(&mut self) {
        self.t_state += 1;
        if self.t_state >= self.t_total {
            self.t_state = 0;
            self.micro_ops.advance();
        }
    }

    /// Add internal T-states to the queue.
    fn queue_internal(&mut self, t_states: u8) {
        self.t_total = t_states;
        self.micro_ops.push(MicroOp::Internal);
    }

    /// Decode and execute the current instruction.
    fn decode_and_execute(&mut self) {
        // Handle IM2 vector read completion
        if self.im2_pending {
            self.im2_pending = false;
            self.regs.pc = u16::from(self.data_lo) | (u16::from(self.data_hi) << 8);
            return;
        }

        if self.in_followup {
            self.in_followup = false;
            self.execute_followup();
            return;
        }

        if self.prefix2 == 0xCB {
            // DDCB or FDCB prefix
            self.execute_ddcb_fdcb();
        } else {
            match self.prefix {
                0 => self.execute_unprefixed(),
                0xCB => self.execute_cb(),
                0xDD | 0xFD => self.execute_dd_fd(),
                0xED => self.execute_ed(),
                _ => self.queue_fetch(),
            }
        }
    }

    /// Queue an Execute micro-op for followup after data read.
    fn queue_execute_followup(&mut self) {
        self.in_followup = true;
        self.followup_stage += 1;
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Get register by 3-bit encoding (bits 5-3 or 2-0).
    fn get_reg8(&self, r: u8) -> u8 {
        match r & 7 {
            0 => self.regs.b,
            1 => self.regs.c,
            2 => self.regs.d,
            3 => self.regs.e,
            4 => self.regs.h,
            5 => self.regs.l,
            6 => 0, // (HL) - should be handled specially
            7 => self.regs.a,
            _ => unreachable!(),
        }
    }

    /// Set register by 3-bit encoding.
    fn set_reg8(&mut self, r: u8, value: u8) {
        match r & 7 {
            0 => self.regs.b = value,
            1 => self.regs.c = value,
            2 => self.regs.d = value,
            3 => self.regs.e = value,
            4 => self.regs.h = value,
            5 => self.regs.l = value,
            6 => {} // (HL) - should be handled specially
            7 => self.regs.a = value,
            _ => unreachable!(),
        }
    }

    /// Get register by 3-bit encoding with IX/IY prefix (undocumented IXH/IXL/IYH/IYL).
    fn get_reg8_indexed(&self, r: u8) -> u8 {
        match r & 7 {
            0 => self.regs.b,
            1 => self.regs.c,
            2 => self.regs.d,
            3 => self.regs.e,
            4 => (self.get_index_reg() >> 8) as u8, // IXH/IYH
            5 => self.get_index_reg() as u8,        // IXL/IYL
            6 => 0, // (IX+d)/(IY+d) - handled specially
            7 => self.regs.a,
            _ => unreachable!(),
        }
    }

    /// Set register by 3-bit encoding with IX/IY prefix (undocumented IXH/IXL/IYH/IYL).
    fn set_reg8_indexed(&mut self, r: u8, value: u8) {
        match r & 7 {
            0 => self.regs.b = value,
            1 => self.regs.c = value,
            2 => self.regs.d = value,
            3 => self.regs.e = value,
            4 => {
                // IXH/IYH
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0x00FF) | (u16::from(value) << 8));
            }
            5 => {
                // IXL/IYL
                let idx = self.get_index_reg();
                self.set_index_reg((idx & 0xFF00) | u16::from(value));
            }
            6 => {} // (IX+d)/(IY+d) - handled specially
            7 => self.regs.a = value,
            _ => unreachable!(),
        }
    }

    /// Get register pair by 2-bit encoding (for 16-bit ops).
    fn get_reg16(&self, rp: u8) -> u16 {
        match rp & 3 {
            0 => self.regs.bc(),
            1 => self.regs.de(),
            2 => self.get_index_reg(),
            3 => self.regs.sp,
            _ => unreachable!(),
        }
    }

    /// Set register pair by 2-bit encoding.
    fn set_reg16(&mut self, rp: u8, value: u16) {
        match rp & 3 {
            0 => self.regs.set_bc(value),
            1 => self.regs.set_de(value),
            2 => self.set_index_reg(value),
            3 => self.regs.sp = value,
            _ => unreachable!(),
        }
    }

    /// Get register pair for PUSH/POP (AF instead of SP).
    fn get_reg16_af(&self, rp: u8) -> u16 {
        match rp & 3 {
            0 => self.regs.bc(),
            1 => self.regs.de(),
            2 => self.get_index_reg(),
            3 => self.regs.af(),
            _ => unreachable!(),
        }
    }

    /// Set register pair for PUSH/POP.
    fn set_reg16_af(&mut self, rp: u8, value: u16) {
        match rp & 3 {
            0 => self.regs.set_bc(value),
            1 => self.regs.set_de(value),
            2 => self.set_index_reg(value),
            3 => self.regs.set_af(value),
            _ => unreachable!(),
        }
    }

    /// Evaluate condition code.
    fn condition(&self, cc: u8) -> bool {
        match cc & 7 {
            0 => self.regs.f & ZF == 0, // NZ
            1 => self.regs.f & ZF != 0, // Z
            2 => self.regs.f & CF == 0, // NC
            3 => self.regs.f & CF != 0, // C
            4 => self.regs.f & PF == 0, // PO
            5 => self.regs.f & PF != 0, // PE
            6 => self.regs.f & SF == 0, // P
            7 => self.regs.f & SF != 0, // M
            _ => unreachable!(),
        }
    }
}

// Instruction execution split into separate file for readability
mod execute;

impl Z80 {
    /// Handle NMI.
    fn handle_nmi(&mut self) {
        self.regs.iff2 = self.regs.iff1;
        self.regs.iff1 = false;

        // Push PC (11 T-states total for NMI)
        self.data_hi = (self.regs.pc >> 8) as u8;
        self.data_lo = self.regs.pc as u8;
        self.micro_ops.clear();
        self.queue_internal(5); // Internal T-states
        self.micro_ops.push(MicroOp::WriteMemHiFirst);
        self.micro_ops.push(MicroOp::WriteMemLoSecond);
        self.regs.pc = 0x0066;
        self.micro_ops.push(MicroOp::FetchOpcode);
    }

    /// Handle maskable interrupt.
    fn handle_int(&mut self) {
        self.regs.iff1 = false;
        self.regs.iff2 = false;

        match self.regs.im {
            0 | 1 => {
                // Mode 0/1: Jump to 0x0038 (13 T-states)
                self.data_hi = (self.regs.pc >> 8) as u8;
                self.data_lo = self.regs.pc as u8;
                self.micro_ops.clear();
                self.queue_internal(7);
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);
                self.regs.pc = 0x0038;
                self.micro_ops.push(MicroOp::FetchOpcode);
            }
            2 => {
                // Mode 2: Vector table (19 T-states)
                self.data_hi = (self.regs.pc >> 8) as u8;
                self.data_lo = self.regs.pc as u8;
                self.micro_ops.clear();
                self.queue_internal(7);
                self.micro_ops.push(MicroOp::WriteMemHiFirst);
                self.micro_ops.push(MicroOp::WriteMemLoSecond);

                // Read vector - data bus provides low byte (typically 0xFF)
                self.addr = (u16::from(self.regs.i) << 8) | 0xFE;
                self.micro_ops.push(MicroOp::ReadMem16Lo);
                self.micro_ops.push(MicroOp::ReadMem16Hi);

                // Mark that we need to set PC when reads complete
                self.im2_pending = true;
                self.micro_ops.push(MicroOp::Execute);
                self.micro_ops.push(MicroOp::FetchOpcode);
            }
            _ => {
                self.queue_fetch();
            }
        }
    }
}

impl Cpu for Z80 {
    type Registers = Registers;

    fn tick<B: Bus>(&mut self, bus: &mut B) {
        self.total_ticks += Ticks::new(1);

        // If we have pending wait states from contention, burn them first
        if self.wait_states > 0 {
            self.wait_states -= 1;
            return;
        }

        // If halted, just burn T-states until interrupt
        if self.regs.halted {
            if self.nmi_pending {
                self.nmi_pending = false;
                self.regs.halted = false;
                self.handle_nmi();
            } else if self.int_pending && self.regs.iff1 {
                self.int_pending = false;
                self.regs.halted = false;
                self.handle_int();
            }
            return;
        }

        self.tick_internal(bus);

        // Check for instruction complete and pending interrupts
        if self.micro_ops.is_empty() {
            if self.nmi_pending {
                self.nmi_pending = false;
                self.handle_nmi();
            } else if self.int_pending && self.regs.iff1 {
                self.int_pending = false;
                self.handle_int();
            } else {
                self.queue_fetch();
            }
        }
    }

    fn pc(&self) -> u16 {
        self.regs.pc
    }

    fn registers(&self) -> Self::Registers {
        self.regs
    }

    fn is_halted(&self) -> bool {
        self.regs.halted
    }

    fn interrupt(&mut self) -> bool {
        if self.regs.iff1 {
            self.int_pending = true;
            true
        } else {
            false
        }
    }

    fn nmi(&mut self) {
        self.nmi_pending = true;
    }

    fn reset(&mut self) {
        self.regs = Registers::default();
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::FetchOpcode);
        self.t_state = 0;
        self.t_total = 0;
        self.wait_states = 0;
        self.opcode = 0;
        self.prefix = 0;
        self.prefix2 = 0;
        self.displacement = 0;
        self.addr = 0;
        self.data_lo = 0;
        self.data_hi = 0;
        self.in_followup = false;
        self.followup_stage = 0;
        self.im2_pending = false;
        self.int_pending = false;
        self.nmi_pending = false;
        self.nmi_last = false;
    }
}

/// All query paths supported by the Z80.
const Z80_QUERY_PATHS: &[&str] = &[
    // Main registers
    "a", "f", "b", "c", "d", "e", "h", "l",
    // Register pairs
    "af", "bc", "de", "hl",
    // Alternate registers
    "a'", "f'", "b'", "c'", "d'", "e'", "h'", "l'",
    "af'", "bc'", "de'", "hl'",
    // Index registers
    "ix", "iy", "ixh", "ixl", "iyh", "iyl",
    // Other registers
    "sp", "pc", "i", "r",
    // Flags (individual)
    "flags.s", "flags.z", "flags.y", "flags.h",
    "flags.x", "flags.p", "flags.n", "flags.c",
    // Interrupt state
    "iff1", "iff2", "im",
    // CPU state
    "halted", "ticks",
    // Current instruction state
    "opcode", "prefix", "t_state",
];

impl Observable for Z80 {
    fn query(&self, path: &str) -> Option<Value> {
        match path {
            // Main registers
            "a" => Some(self.regs.a.into()),
            "f" => Some(self.regs.f.into()),
            "b" => Some(self.regs.b.into()),
            "c" => Some(self.regs.c.into()),
            "d" => Some(self.regs.d.into()),
            "e" => Some(self.regs.e.into()),
            "h" => Some(self.regs.h.into()),
            "l" => Some(self.regs.l.into()),

            // Register pairs
            "af" => Some(self.regs.af().into()),
            "bc" => Some(self.regs.bc().into()),
            "de" => Some(self.regs.de().into()),
            "hl" => Some(self.regs.hl().into()),

            // Alternate registers
            "a'" => Some(self.regs.a_alt.into()),
            "f'" => Some(self.regs.f_alt.into()),
            "b'" => Some(self.regs.b_alt.into()),
            "c'" => Some(self.regs.c_alt.into()),
            "d'" => Some(self.regs.d_alt.into()),
            "e'" => Some(self.regs.e_alt.into()),
            "h'" => Some(self.regs.h_alt.into()),
            "l'" => Some(self.regs.l_alt.into()),

            // Alternate pairs
            "af'" => Some(((u16::from(self.regs.a_alt) << 8) | u16::from(self.regs.f_alt)).into()),
            "bc'" => Some(((u16::from(self.regs.b_alt) << 8) | u16::from(self.regs.c_alt)).into()),
            "de'" => Some(((u16::from(self.regs.d_alt) << 8) | u16::from(self.regs.e_alt)).into()),
            "hl'" => Some(((u16::from(self.regs.h_alt) << 8) | u16::from(self.regs.l_alt)).into()),

            // Index registers
            "ix" => Some(self.regs.ix.into()),
            "iy" => Some(self.regs.iy.into()),
            "ixh" => Some(((self.regs.ix >> 8) as u8).into()),
            "ixl" => Some((self.regs.ix as u8).into()),
            "iyh" => Some(((self.regs.iy >> 8) as u8).into()),
            "iyl" => Some((self.regs.iy as u8).into()),

            // Other registers
            "sp" => Some(self.regs.sp.into()),
            "pc" => Some(self.regs.pc.into()),
            "i" => Some(self.regs.i.into()),
            "r" => Some(self.regs.r.into()),

            // Individual flags
            "flags.s" => Some((self.regs.f & SF != 0).into()),
            "flags.z" => Some((self.regs.f & ZF != 0).into()),
            "flags.y" => Some((self.regs.f & YF != 0).into()),
            "flags.h" => Some((self.regs.f & HF != 0).into()),
            "flags.x" => Some((self.regs.f & XF != 0).into()),
            "flags.p" => Some((self.regs.f & PF != 0).into()),
            "flags.n" => Some((self.regs.f & NF != 0).into()),
            "flags.c" => Some((self.regs.f & CF != 0).into()),

            // Interrupt state
            "iff1" => Some(self.regs.iff1.into()),
            "iff2" => Some(self.regs.iff2.into()),
            "im" => Some(self.regs.im.into()),

            // CPU state
            "halted" => Some(self.regs.halted.into()),
            "ticks" => Some(self.total_ticks.get().into()),

            // Current instruction state
            "opcode" => Some(self.opcode.into()),
            "prefix" => Some(self.prefix.into()),
            "t_state" => Some(self.t_state.into()),

            _ => None,
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        Z80_QUERY_PATHS
    }
}
