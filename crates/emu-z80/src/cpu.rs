//! Z80 CPU core with per-T-state execution.

#![allow(clippy::cast_possible_truncation)] // Intentional truncation for low byte extraction.

use emu_core::{Bus, Cpu, Tickable, Ticks};

use crate::registers::Registers;

/// Current execution phase within an instruction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    /// Fetching opcode byte (M1 cycle).
    #[default]
    Fetch,
    /// Executing instruction (variable T-states).
    Execute,
}

/// Z80 CPU.
pub struct Z80<B: Bus> {
    /// All CPU registers.
    regs: Registers,
    /// Memory/IO bus.
    bus: B,
    /// Current execution phase.
    phase: Phase,
    /// T-state counter within current phase.
    t_state: u8,
    /// Current opcode being executed.
    opcode: u8,
    /// Prefix state (CB, DD, ED, FD, or combinations).
    prefix: Prefix,
    /// Pending interrupt request.
    int_pending: bool,
    /// Pending NMI request.
    nmi_pending: bool,
    /// Total T-states elapsed.
    total_ticks: Ticks,
}

/// Instruction prefix state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)] // Standard Z80 terminology.
enum Prefix {
    #[default]
    None,
    CB,
    DD,
    ED,
    FD,
    DDCB,
    FDCB,
}

impl<B: Bus> Z80<B> {
    /// Create a new Z80 with the given bus.
    pub fn new(bus: B) -> Self {
        Self {
            regs: Registers::default(),
            bus,
            phase: Phase::Fetch,
            t_state: 0,
            opcode: 0,
            prefix: Prefix::None,
            int_pending: false,
            nmi_pending: false,
            total_ticks: Ticks::ZERO,
        }
    }

    /// Access the bus.
    pub fn bus(&self) -> &B {
        &self.bus
    }

    /// Mutably access the bus.
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    /// Total T-states elapsed since reset.
    #[must_use]
    pub const fn total_ticks(&self) -> Ticks {
        self.total_ticks
    }

    /// Read byte from memory (no timing side effects).
    fn read(&mut self, addr: u16) -> u8 {
        self.bus.read(addr)
    }

    /// Write byte to memory (no timing side effects).
    fn write(&mut self, addr: u16, value: u8) {
        self.bus.write(addr, value);
    }

    /// Fetch next byte at PC and increment PC.
    fn fetch_byte(&mut self) -> u8 {
        let value = self.read(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        value
    }

    /// Increment R register (lower 7 bits only).
    fn inc_r(&mut self) {
        self.regs.r = (self.regs.r & 0x80) | ((self.regs.r.wrapping_add(1)) & 0x7F);
    }

    /// Execute one T-state of the fetch phase.
    #[allow(clippy::match_same_arms)] // T-states 0-1 are timing placeholders.
    fn tick_fetch(&mut self) {
        match self.t_state {
            0 => {
                // T1: Address out, MREQ and RD asserted
            }
            1 => {
                // T2: Memory responds
            }
            2 => {
                // T3: Data read, RFSH address out
                self.opcode = self.fetch_byte();
                self.inc_r();
            }
            3 => {
                // T4: Refresh cycle complete
                // Transition to execute phase
                self.phase = Phase::Execute;
                self.t_state = 0;
                return;
            }
            _ => unreachable!(),
        }
        self.t_state += 1;
    }

    /// Execute one T-state of the execute phase.
    fn tick_execute(&mut self) {
        // For now, handle only NOP (0x00) as a placeholder.
        // Real implementation will dispatch based on opcode and prefix.
        match self.prefix {
            Prefix::None => self.execute_unprefixed(),
            Prefix::CB => self.execute_cb(),
            Prefix::DD => self.execute_dd(),
            Prefix::ED => self.execute_ed(),
            Prefix::FD => self.execute_fd(),
            Prefix::DDCB => self.execute_ddcb(),
            Prefix::FDCB => self.execute_fdcb(),
        }
    }

    /// Execute unprefixed instruction.
    fn execute_unprefixed(&mut self) {
        match self.opcode {
            // NOP: 4 T-states total (all in fetch)
            0x00 => {
                self.instruction_complete();
            }
            // HALT
            0x76 => {
                self.regs.halted = true;
                self.instruction_complete();
            }
            // Prefix bytes - fetch another opcode
            0xCB => {
                self.prefix = Prefix::CB;
                self.phase = Phase::Fetch;
                self.t_state = 0;
            }
            0xDD => {
                self.prefix = Prefix::DD;
                self.phase = Phase::Fetch;
                self.t_state = 0;
            }
            0xED => {
                self.prefix = Prefix::ED;
                self.phase = Phase::Fetch;
                self.t_state = 0;
            }
            0xFD => {
                self.prefix = Prefix::FD;
                self.phase = Phase::Fetch;
                self.t_state = 0;
            }
            // Placeholder for unimplemented instructions
            _ => {
                // TODO: Implement remaining instructions
                self.instruction_complete();
            }
        }
    }

    /// Execute CB-prefixed instruction.
    fn execute_cb(&mut self) {
        // TODO: Implement CB instructions (bit operations)
        self.instruction_complete();
    }

    /// Execute DD-prefixed instruction (IX operations).
    fn execute_dd(&mut self) {
        match self.opcode {
            0xCB => {
                self.prefix = Prefix::DDCB;
                self.phase = Phase::Fetch;
                self.t_state = 0;
            }
            _ => {
                // TODO: Implement DD instructions
                self.instruction_complete();
            }
        }
    }

    /// Execute ED-prefixed instruction.
    fn execute_ed(&mut self) {
        // TODO: Implement ED instructions (misc/block)
        self.instruction_complete();
    }

    /// Execute FD-prefixed instruction (IY operations).
    fn execute_fd(&mut self) {
        match self.opcode {
            0xCB => {
                self.prefix = Prefix::FDCB;
                self.phase = Phase::Fetch;
                self.t_state = 0;
            }
            _ => {
                // TODO: Implement FD instructions
                self.instruction_complete();
            }
        }
    }

    /// Execute DDCB-prefixed instruction.
    fn execute_ddcb(&mut self) {
        // TODO: Implement DDCB instructions
        self.instruction_complete();
    }

    /// Execute FDCB-prefixed instruction.
    fn execute_fdcb(&mut self) {
        // TODO: Implement FDCB instructions
        self.instruction_complete();
    }

    /// Mark current instruction as complete and prepare for next.
    fn instruction_complete(&mut self) {
        self.prefix = Prefix::None;
        self.phase = Phase::Fetch;
        self.t_state = 0;

        // Check for pending interrupts
        if self.nmi_pending {
            self.nmi_pending = false;
            self.handle_nmi();
        } else if self.int_pending && self.regs.iff1 {
            self.int_pending = false;
            self.handle_int();
        }
    }

    /// Handle NMI.
    fn handle_nmi(&mut self) {
        self.regs.halted = false;
        self.regs.iff2 = self.regs.iff1;
        self.regs.iff1 = false;

        // Push PC onto stack
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.write(self.regs.sp, (self.regs.pc >> 8) as u8);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.write(self.regs.sp, self.regs.pc as u8);

        // Jump to NMI vector
        self.regs.pc = 0x0066;
    }

    /// Handle maskable interrupt.
    fn handle_int(&mut self) {
        self.regs.halted = false;
        self.regs.iff1 = false;
        self.regs.iff2 = false;

        match self.regs.im {
            0 => {
                // Mode 0: Execute instruction on data bus (usually RST)
                // For now, treat as RST 38h
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.write(self.regs.sp, (self.regs.pc >> 8) as u8);
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.write(self.regs.sp, self.regs.pc as u8);
                self.regs.pc = 0x0038;
            }
            1 => {
                // Mode 1: Jump to 0x0038
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.write(self.regs.sp, (self.regs.pc >> 8) as u8);
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.write(self.regs.sp, self.regs.pc as u8);
                self.regs.pc = 0x0038;
            }
            2 => {
                // Mode 2: Jump via vector table
                // Vector address = (I << 8) | (data_bus & 0xFE)
                // For now, assume data bus is 0xFF
                let vector_addr = (u16::from(self.regs.i) << 8) | 0xFE;
                let lo = self.read(vector_addr);
                let hi = self.read(vector_addr.wrapping_add(1));

                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.write(self.regs.sp, (self.regs.pc >> 8) as u8);
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.write(self.regs.sp, self.regs.pc as u8);

                self.regs.pc = u16::from(lo) | (u16::from(hi) << 8);
            }
            _ => {}
        }
    }
}

impl<B: Bus> Tickable for Z80<B> {
    fn tick(&mut self) {
        self.total_ticks += Ticks::new(1);

        // If halted, just burn T-states until interrupt
        if self.regs.halted {
            // Still need to handle pending interrupts
            if self.nmi_pending {
                self.nmi_pending = false;
                self.handle_nmi();
            } else if self.int_pending && self.regs.iff1 {
                self.int_pending = false;
                self.handle_int();
            }
            return;
        }

        match self.phase {
            Phase::Fetch => self.tick_fetch(),
            Phase::Execute => self.tick_execute(),
        }
    }
}

impl<B: Bus> Cpu for Z80<B> {
    type Registers = Registers;

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
        self.phase = Phase::Fetch;
        self.t_state = 0;
        self.opcode = 0;
        self.prefix = Prefix::None;
        self.int_pending = false;
        self.nmi_pending = false;
        // Note: total_ticks not reset - that's a system concern
    }
}
