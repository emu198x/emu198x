//! Motorola MC6809 CPU foundation.
//!
//! This crate starts with the external bus-facing state needed by Dragon/CoCo
//! machine wiring. Instruction execution will grow behind this boundary; the
//! public pin/register shape is deliberately small and serializable so machine
//! snapshots can use it directly.

pub mod registers;

use registers::{FLAG_C, FLAG_F, FLAG_I, FLAG_N, FLAG_V, FLAG_Z, Registers};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum CpuState {
    Fetch,
    Prefix10,
    NopInternal,
    ClrAInternal,
    ClrBInternal,
    ReadImm8(Imm8Op),
    ReadImm16Hi(Imm16Op),
    ReadImm16Lo { op: Imm16Op, hi: u8 },
    ReadRel8(Rel8Op),
    ReadDirectOperand(Mem8Op),
    ReadDirectValue(Mem8Op),
    ReadExtendedHi(ExtOp),
    ReadExtendedLo { op: ExtOp, hi: u8 },
    ReadExtendedValue(Mem8Op),
    WriteDirectOperand(Store8Op),
    WriteValue,
    IllegalOpcode(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Imm8Op {
    Lda,
    Ldb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Imm16Op {
    Ldx,
    Ldu,
    Lds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Rel8Op {
    Bra,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Mem8Op {
    Lda,
    Ldb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Store8Op {
    Sta,
    Stb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum ExtOp {
    Load(Mem8Op),
    Store(Store8Op),
    Jmp,
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
                    0x10 => self.read_next(CpuState::Prefix10),
                    0x12 => {
                        // NOP is a two-cycle instruction: opcode fetch plus
                        // one internal cycle.
                        self.state = CpuState::NopInternal;
                        self.addr = self.regs.pc;
                        self.rw = true;
                    }
                    0x20 => self.read_next(CpuState::ReadRel8(Rel8Op::Bra)),
                    0x4F => {
                        self.clear_a();
                        self.read_next(CpuState::ClrAInternal);
                    }
                    0x5F => {
                        self.clear_b();
                        self.read_next(CpuState::ClrBInternal);
                    }
                    0x7E => self.read_next(CpuState::ReadExtendedHi(ExtOp::Jmp)),
                    0x86 => self.read_next(CpuState::ReadImm8(Imm8Op::Lda)),
                    0x8E => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Ldx)),
                    0x96 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Lda)),
                    0x97 => self.read_next(CpuState::WriteDirectOperand(Store8Op::Sta)),
                    0xB6 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Lda))),
                    0xB7 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store(Store8Op::Sta))),
                    0xC6 => self.read_next(CpuState::ReadImm8(Imm8Op::Ldb)),
                    0xCE => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Ldu)),
                    0xD6 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Ldb)),
                    0xD7 => self.read_next(CpuState::WriteDirectOperand(Store8Op::Stb)),
                    0xF6 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Ldb))),
                    0xF7 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store(Store8Op::Stb))),
                    _ => self.trap_illegal(opcode),
                }
            }
            CpuState::Prefix10 => {
                let opcode = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match opcode {
                    0xCE => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Lds)),
                    _ => self.trap_illegal(opcode),
                }
            }
            CpuState::NopInternal => {
                self.next_fetch();
            }
            CpuState::ClrAInternal | CpuState::ClrBInternal => {
                self.next_fetch();
            }
            CpuState::ReadImm8(op) => {
                let value = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.load_imm8(op, value);
                self.next_fetch();
            }
            CpuState::ReadImm16Hi(op) => {
                let hi = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.read_next(CpuState::ReadImm16Lo { op, hi });
            }
            CpuState::ReadImm16Lo { op, hi } => {
                let value = u16::from_be_bytes([hi, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.load_imm16(op, value);
                self.next_fetch();
            }
            CpuState::ReadRel8(Rel8Op::Bra) => {
                let offset = self.data_in as i8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.regs.pc = self.regs.pc.wrapping_add_signed(i16::from(offset));
                self.next_fetch();
            }
            CpuState::ReadDirectOperand(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.state = CpuState::ReadDirectValue(op);
                self.addr = addr;
                self.rw = true;
                self.sync = false;
            }
            CpuState::ReadDirectValue(op) => {
                self.load_mem8(op, self.data_in);
                self.next_fetch();
            }
            CpuState::ReadExtendedHi(op) => {
                let hi = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.read_next(CpuState::ReadExtendedLo { op, hi });
            }
            CpuState::ReadExtendedLo { op, hi } => {
                let addr = u16::from_be_bytes([hi, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match op {
                    ExtOp::Load(op) => {
                        self.state = CpuState::ReadExtendedValue(op);
                        self.addr = addr;
                        self.rw = true;
                        self.sync = false;
                    }
                    ExtOp::Store(op) => self.prepare_store(op, addr),
                    ExtOp::Jmp => {
                        self.regs.pc = addr;
                        self.next_fetch();
                    }
                }
            }
            CpuState::ReadExtendedValue(op) => {
                self.load_mem8(op, self.data_in);
                self.next_fetch();
            }
            CpuState::WriteDirectOperand(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.prepare_store(op, addr);
            }
            CpuState::WriteValue => {
                self.next_fetch();
            }
            CpuState::IllegalOpcode(_) => {
                self.halt = true;
            }
        }
    }

    fn read_next(&mut self, state: CpuState) {
        self.state = state;
        self.addr = self.regs.pc;
        self.rw = true;
        self.sync = false;
    }

    fn next_fetch(&mut self) {
        self.state = CpuState::Fetch;
        self.addr = self.regs.pc;
        self.rw = true;
        self.sync = true;
    }

    fn trap_illegal(&mut self, opcode: u8) {
        self.state = CpuState::IllegalOpcode(opcode);
        self.halt = true;
        self.addr = self.regs.pc;
        self.rw = true;
        self.sync = false;
    }

    fn clear_a(&mut self) {
        self.regs.a = 0;
        self.set_nz8(0);
        self.regs.set_flag(FLAG_V, false);
        self.regs.set_flag(FLAG_C, false);
    }

    fn clear_b(&mut self) {
        self.regs.b = 0;
        self.set_nz8(0);
        self.regs.set_flag(FLAG_V, false);
        self.regs.set_flag(FLAG_C, false);
    }

    fn load_imm8(&mut self, op: Imm8Op, value: u8) {
        match op {
            Imm8Op::Lda => self.regs.a = value,
            Imm8Op::Ldb => self.regs.b = value,
        }
        self.set_load_flags8(value);
    }

    fn load_mem8(&mut self, op: Mem8Op, value: u8) {
        match op {
            Mem8Op::Lda => self.regs.a = value,
            Mem8Op::Ldb => self.regs.b = value,
        }
        self.set_load_flags8(value);
    }

    fn load_imm16(&mut self, op: Imm16Op, value: u16) {
        match op {
            Imm16Op::Ldx => self.regs.x = value,
            Imm16Op::Ldu => self.regs.u = value,
            Imm16Op::Lds => self.regs.s = value,
        }
        self.set_nz16(value);
        self.regs.set_flag(FLAG_V, false);
    }

    fn prepare_store(&mut self, op: Store8Op, addr: u16) {
        let value = match op {
            Store8Op::Sta => self.regs.a,
            Store8Op::Stb => self.regs.b,
        };
        self.set_load_flags8(value);
        self.state = CpuState::WriteValue;
        self.addr = addr;
        self.data = value;
        self.rw = false;
        self.sync = false;
    }

    fn set_load_flags8(&mut self, value: u8) {
        self.set_nz8(value);
        self.regs.set_flag(FLAG_V, false);
    }

    fn set_nz8(&mut self, value: u8) {
        self.regs.set_flag(FLAG_N, value & 0x80 != 0);
        self.regs.set_flag(FLAG_Z, value == 0);
    }

    fn set_nz16(&mut self, value: u16) {
        self.regs.set_flag(FLAG_N, value & 0x8000 != 0);
        self.regs.set_flag(FLAG_Z, value == 0);
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

    fn run_cycle(cpu: &mut Mc6809, memory: &mut [u8; 0x10000]) {
        if cpu.rw {
            cpu.data_in = memory[cpu.addr as usize];
        } else {
            memory[cpu.addr as usize] = cpu.data;
        }
        cpu.tick();
    }

    fn run_cycles(cpu: &mut Mc6809, memory: &mut [u8; 0x10000], count: usize) {
        for _ in 0..count {
            run_cycle(cpu, memory);
        }
    }

    fn cpu_at(pc: u16) -> Mc6809 {
        let mut cpu = Mc6809::new();
        cpu.regs.pc = pc;
        cpu.addr = pc;
        cpu.sync = true;
        cpu
    }

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
        let mut cpu = cpu_at(0x4000);
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
        let mut cpu = cpu_at(0x4000);
        cpu.data_in = 0xFF;

        cpu.tick();

        assert!(cpu.halt);
        assert_eq!(cpu.regs.pc, 0x4001);
        assert!(!cpu.instruction_boundary());
    }

    #[test]
    fn lda_and_ldb_immediate_update_nzv_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x86; // LDA #$80
        memory[0x4001] = 0x80;
        memory[0x4002] = 0xC6; // LDB #$00
        memory[0x4003] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.set_flag(FLAG_V, true);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x80);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0x00);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
    }

    #[test]
    fn sixteen_bit_immediate_loads_x_u_and_s() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x8E; // LDX #$1234
        memory[0x4001] = 0x12;
        memory[0x4002] = 0x34;
        memory[0x4003] = 0xCE; // LDU #$8000
        memory[0x4004] = 0x80;
        memory[0x4005] = 0x00;
        memory[0x4006] = 0x10; // LDS #$0000
        memory[0x4007] = 0xCE;
        memory[0x4008] = 0x00;
        memory[0x4009] = 0x00;
        let mut cpu = cpu_at(0x4000);

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.x, 0x1234);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.u, 0x8000);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));

        run_cycles(&mut cpu, &mut memory, 4);
        assert_eq!(cpu.regs.s, 0x0000);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_Z));
    }

    #[test]
    fn direct_loads_use_direct_page_register() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x96; // LDA <$34
        memory[0x4001] = 0x34;
        memory[0x1234] = 0x7F;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;

        run_cycles(&mut cpu, &mut memory, 3);

        assert_eq!(cpu.regs.a, 0x7F);
        assert_eq!(cpu.regs.pc, 0x4002);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn extended_load_store_and_direct_store_use_bus_cycles() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xB6; // LDA $2345
        memory[0x4001] = 0x23;
        memory[0x4002] = 0x45;
        memory[0x2345] = 0xA5;
        memory[0x4003] = 0xD7; // STB <$20
        memory[0x4004] = 0x20;
        memory[0x4005] = 0xF7; // STB $3456
        memory[0x4006] = 0x34;
        memory[0x4007] = 0x56;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.b = 0x5A;

        run_cycles(&mut cpu, &mut memory, 4);
        assert_eq!(cpu.regs.a, 0xA5);
        assert!(cpu.regs.flag(FLAG_N));

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(memory[0x1220], 0x5A);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 4);
        assert_eq!(memory[0x3456], 0x5A);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn bra_and_jmp_change_pc() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x20; // BRA +2
        memory[0x4001] = 0x02;
        memory[0x4004] = 0x7E; // JMP $4567
        memory[0x4005] = 0x45;
        memory[0x4006] = 0x67;
        let mut cpu = cpu_at(0x4000);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.pc, 0x4004);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.pc, 0x4567);
        assert_eq!(cpu.addr, 0x4567);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn clear_a_and_b_set_expected_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x4F; // CLRA
        memory[0x4001] = 0x5F; // CLRB
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0xFF;
        cpu.regs.b = 0x80;
        cpu.regs.set_flag(FLAG_C, true);
        cpu.regs.set_flag(FLAG_V, true);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(!cpu.regs.flag(FLAG_C));

        cpu.regs.set_flag(FLAG_C, true);
        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
    }
}
