//! Motorola MC6809 CPU foundation.
//!
//! This crate starts with the external bus-facing state needed by Dragon/CoCo
//! machine wiring. Instruction execution will grow behind this boundary; the
//! public pin/register shape is deliberately small and serializable so machine
//! snapshots can use it directly.

pub mod registers;

use registers::{FLAG_F, FLAG_I, Registers};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum CpuState {
    Fetch,
    NopInternal,
    IllegalOpcode(u8),
}

/// Motorola MC6809 CPU state exposed to machine crates.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mc6809 {
    pub regs: Registers,
    pub total_cycles: u64,
    pub addr: u16,
    pub data: u8,
    pub data_in: u8,
    pub rw: bool,
    pub sync: bool,
    pub irq: bool,
    pub firq: bool,
    pub nmi: bool,
    pub halt: bool,
    pub reset_phase: u8,
    vector_hi: u8,
    state: CpuState,
}

impl Mc6809 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            total_cycles: 0,
            addr: 0,
            data: 0,
            data_in: 0,
            rw: true,
            sync: false,
            irq: false,
            firq: false,
            nmi: false,
            halt: false,
            reset_phase: 0,
            vector_hi: 0,
            state: CpuState::Fetch,
        }
    }

    /// Schedule reset-vector fetches at `$FFFE/$FFFF`.
    pub fn reset(&mut self) {
        self.regs.cc |= FLAG_F | FLAG_I;
        self.total_cycles = 0;
        self.addr = 0xFFFE;
        self.data = 0;
        self.data_in = 0;
        self.rw = true;
        self.sync = false;
        self.irq = false;
        self.firq = false;
        self.nmi = false;
        self.halt = false;
        self.reset_phase = 2;
        self.vector_hi = 0;
        self.state = CpuState::Fetch;
    }

    /// Advance one bus-visible CPU cycle.
    pub fn tick(&mut self) {
        if self.halt {
            return;
        }

        match self.reset_phase {
            2 => {
                self.vector_hi = self.data_in;
                self.addr = 0xFFFF;
                self.rw = true;
                self.sync = false;
                self.reset_phase = 1;
            }
            1 => {
                self.regs.pc = u16::from_be_bytes([self.vector_hi, self.data_in]);
                self.addr = self.regs.pc;
                self.rw = true;
                self.sync = true;
                self.reset_phase = 0;
            }
            _ => self.tick_instruction(),
        }

        self.total_cycles = self.total_cycles.saturating_add(1);
    }

    #[must_use]
    pub const fn instruction_boundary(&self) -> bool {
        self.reset_phase == 0 && self.sync
    }

    fn tick_instruction(&mut self) {
        match self.state {
            CpuState::Fetch => {
                let opcode = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.sync = false;
                match opcode {
                    0x12 => {
                        // NOP is a two-cycle instruction: opcode fetch plus
                        // one internal cycle.
                        self.state = CpuState::NopInternal;
                        self.addr = self.regs.pc;
                        self.rw = true;
                    }
                    _ => {
                        self.state = CpuState::IllegalOpcode(opcode);
                        self.halt = true;
                        self.addr = self.regs.pc;
                        self.rw = true;
                    }
                }
            }
            CpuState::NopInternal => {
                self.state = CpuState::Fetch;
                self.addr = self.regs.pc;
                self.rw = true;
                self.sync = true;
            }
            CpuState::IllegalOpcode(_) => {
                self.halt = true;
            }
        }
    }
}

impl Default for Mc6809 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{FLAG_C, FLAG_I};

    #[test]
    fn reset_fetches_vector_big_endian() {
        let mut cpu = Mc6809::new();
        cpu.reset();
        assert_eq!(cpu.addr, 0xFFFE);
        assert!(cpu.rw);
        assert_eq!(cpu.reset_phase, 2);

        cpu.data_in = 0xC0;
        cpu.tick();
        assert_eq!(cpu.addr, 0xFFFF);
        assert_eq!(cpu.reset_phase, 1);
        assert!(!cpu.sync);

        cpu.data_in = 0x00;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC000);
        assert_eq!(cpu.addr, 0xC000);
        assert_eq!(cpu.reset_phase, 0);
        assert!(cpu.sync);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn reset_sets_interrupt_masks_without_clearing_other_cc_bits() {
        let mut cpu = Mc6809::new();
        cpu.regs.set_flag(FLAG_C, true);
        cpu.regs.set_flag(FLAG_I, false);
        cpu.irq = true;
        cpu.firq = true;
        cpu.nmi = true;

        cpu.reset();

        assert!(cpu.regs.flag(FLAG_C));
        assert!(cpu.regs.irq_masked());
        assert!(cpu.regs.firq_masked());
        assert!(!cpu.irq);
        assert!(!cpu.firq);
        assert!(!cpu.nmi);
    }

    #[test]
    fn idle_tick_presents_pc_for_fetch() {
        let mut cpu = Mc6809::new();
        cpu.regs.pc = 0x1234;
        cpu.sync = true;
        cpu.tick();

        assert!(cpu.halt);
        assert_eq!(cpu.addr, 0x1235);
        assert!(cpu.rw);
        assert!(!cpu.sync);
        assert_eq!(cpu.total_cycles, 1);
    }

    #[test]
    fn nop_fetches_and_returns_to_instruction_boundary() {
        let mut cpu = Mc6809::new();
        cpu.regs.pc = 0x4000;
        cpu.addr = 0x4000;
        cpu.sync = true;
        cpu.data_in = 0x12;

        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x4001);
        assert_eq!(cpu.addr, 0x4001);
        assert!(!cpu.sync);
        assert!(!cpu.halt);

        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x4001);
        assert_eq!(cpu.addr, 0x4001);
        assert!(cpu.instruction_boundary());
        assert_eq!(cpu.total_cycles, 2);
    }

    #[test]
    fn unknown_opcode_halts_for_bringup_visibility() {
        let mut cpu = Mc6809::new();
        cpu.regs.pc = 0x4000;
        cpu.data_in = 0xFF;

        cpu.tick();

        assert!(cpu.halt);
        assert_eq!(cpu.regs.pc, 0x4001);
        assert!(!cpu.instruction_boundary());
    }
}
