pub(crate) mod cycle;
pub mod registers;
pub(crate) mod tick;

use registers::Registers;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct M6502 {
    pub regs: Registers,
    pub total_cycles: u64,
    pub addr: u16,
    pub data: u8,
    pub rw: bool,
    pub sync: bool,
    pub data_in: u8,
    pub irq: bool,
    pub nmi: bool,
    pub so: bool,
    pub rdy: bool,
    pub(crate) nmi_prev: bool,
    pub(crate) so_prev: bool,
    pub halted: bool,
    pub decimal_disabled: bool,
    pub(crate) cs: cycle::CycleState,
    pub(crate) reset_phase: u8,
}

impl M6502 {
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_decimal(true)
    }

    #[must_use]
    pub fn new_2a03() -> Self {
        Self::new_with_decimal(false)
    }

    fn new_with_decimal(decimal_enabled: bool) -> Self {
        Self {
            regs: Registers::new(),
            total_cycles: 0,
            addr: 0,
            data: 0,
            rw: true,
            sync: false,
            data_in: 0,
            irq: false,
            nmi: false,
            so: true,
            rdy: true,
            nmi_prev: false,
            so_prev: true,
            halted: false,
            decimal_disabled: !decimal_enabled,
            cs: cycle::CycleState::default(),
            reset_phase: 0,
        }
    }

    pub fn reset(&mut self) {
        self.regs = Registers::new();
        self.total_cycles = 0;
        self.cs.reset();
        self.halted = false;
        self.irq = false;
        self.nmi = false;
        self.nmi_prev = false;
        self.rdy = true;
        self.reset_phase = 2;
        self.addr = 0xFFFC;
        self.rw = true;
        self.sync = false;
        self.data = 0;
        self.data_in = 0;
        self.so = true;
        self.so_prev = true;
    }

    #[must_use]
    pub fn instruction_complete(&self) -> bool {
        self.cs.cycle == 0 && self.reset_phase == 0
    }
}

impl Default for M6502 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_on_state() {
        let cpu = M6502::new();
        assert_eq!(cpu.regs.a, 0);
        assert_eq!(cpu.regs.sp, 0xFD);
        assert!(cpu.regs.interrupt_disable());
        assert_eq!(cpu.total_cycles, 0);
        assert!(!cpu.halted);
        assert!(cpu.rdy);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn reset_schedules_reset_vector_read() {
        let mut cpu = M6502::new();
        cpu.reset();
        assert_eq!(cpu.addr, 0xFFFC);
        assert!(cpu.rw);
        assert!(!cpu.sync);
        assert_eq!(cpu.reset_phase, 2);
        assert!(!cpu.instruction_complete());
    }

    #[test]
    fn reset_clears_interrupt_lines() {
        let mut cpu = M6502::new();
        cpu.irq = true;
        cpu.nmi = true;
        cpu.reset();
        assert!(!cpu.irq);
        assert!(!cpu.nmi);
    }

    #[test]
    fn reset_restores_so_high() {
        let mut cpu = M6502::new();
        cpu.so = false;
        cpu.so_prev = false;
        cpu.reset();
        assert!(cpu.so);
        assert!(cpu.so_prev);
    }

    #[test]
    fn decode_table_classifies_key_opcodes() {
        let nop = cycle::decode(0xEA);
        assert_eq!(nop.addr_mode, cycle::AddrMode::Implied);
        assert_eq!(nop.operation, cycle::Operation::Nop);

        let lda_abs = cycle::decode(0xAD);
        assert_eq!(lda_abs.addr_mode, cycle::AddrMode::Absolute);
        assert_eq!(lda_abs.operation, cycle::Operation::Lda);

        let brk = cycle::decode(0x00);
        assert_eq!(brk.addr_mode, cycle::AddrMode::Brk);
        assert_eq!(brk.operation, cycle::Operation::Brk);
    }

    struct Fixture {
        cpu: M6502,
        mem: [u8; 0x10000],
    }

    impl Fixture {
        fn with_program(start_pc: u16, program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[0xFFFC] = start_pc as u8;
            mem[0xFFFD] = (start_pc >> 8) as u8;
            for (index, byte) in program.iter().enumerate() {
                mem[usize::from(start_pc) + index] = *byte;
            }
            let mut cpu = M6502::new();
            cpu.reset();
            Self { cpu, mem }
        }

        fn step(&mut self) -> bool {
            if self.cpu.rw {
                self.cpu.data_in = self.mem[usize::from(self.cpu.addr)];
            } else {
                self.mem[usize::from(self.cpu.addr)] = self.cpu.data;
            }
            self.cpu.tick()
        }

        fn boot(&mut self) {
            assert!(!self.step());
            assert!(self.step());
            assert!(self.cpu.instruction_complete());
            assert!(self.cpu.sync);
        }

        fn run_one(&mut self) -> u64 {
            let before = self.cpu.total_cycles;
            loop {
                let done = self.step();
                if done && self.cpu.instruction_complete() {
                    break;
                }
            }
            self.cpu.total_cycles - before
        }
    }

    #[test]
    fn lda_immediate_sets_a_and_flags() {
        let mut fixture = Fixture::with_program(0x0400, &[0xA9, 0x42]);
        fixture.boot();
        let cycles = fixture.run_one();
        assert_eq!(cycles, 2);
        assert_eq!(fixture.cpu.regs.a, 0x42);
        assert!(!fixture.cpu.regs.zero());
        assert!(!fixture.cpu.regs.negative());
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
    }

    #[test]
    fn ora_zero_page_x_with_zero_index_keeps_extra_cycle() {
        let mut fixture = Fixture::with_program(0x0400, &[0x15, 0x7C, 0xEA]);
        fixture.mem[0x007C] = 0x92;
        fixture.boot();
        let cycles = fixture.run_one();
        assert_eq!(cycles, 4);
        assert_eq!(fixture.cpu.regs.a, 0x92);
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
    }

    #[test]
    fn falling_so_edge_sets_overflow_before_branch_eval() {
        let mut fixture = Fixture::with_program(0x0400, &[0x50, 0x00, 0xEA]);
        fixture.boot();
        fixture.cpu.so = false;
        let cycles = fixture.run_one();
        assert_eq!(cycles, 2);
        assert!(fixture.cpu.regs.overflow());
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
    }

    #[test]
    fn asl_zero_page_x_with_zero_index_keeps_rmw_timing() {
        let mut fixture = Fixture::with_program(0x0400, &[0x16, 0x37, 0xEA]);
        fixture.mem[0x0037] = 0x29;
        fixture.boot();
        let cycles = fixture.run_one();
        assert_eq!(cycles, 6);
        assert_eq!(fixture.mem[0x0037], 0x52);
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
    }

    #[test]
    fn sta_absolute_writes_correct_address() {
        let mut fixture = Fixture::with_program(0x0400, &[0xA9, 0x42, 0x8D, 0x00, 0x02]);
        fixture.boot();
        assert_eq!(fixture.run_one(), 2);
        assert_eq!(fixture.run_one(), 4);
        assert_eq!(fixture.mem[0x0200], 0x42);
        assert_eq!(fixture.cpu.regs.pc, 0x0405);
    }

    #[test]
    fn jmp_absolute_sets_pc() {
        let mut fixture = Fixture::with_program(0x0400, &[0x4C, 0x34, 0x12]);
        fixture.boot();
        let cycles = fixture.run_one();
        assert_eq!(cycles, 3);
        assert_eq!(fixture.cpu.regs.pc, 0x1234);
    }

    #[test]
    fn brk_pushes_state_and_reads_irq_vector() {
        let mut fixture = Fixture::with_program(0x0400, &[0x00, 0xEA]);
        fixture.mem[0xFFFE] = 0x00;
        fixture.mem[0xFFFF] = 0x30;
        fixture.boot();

        let sp_before = fixture.cpu.regs.sp;
        let cycles = fixture.run_one();
        assert_eq!(cycles, 7);
        assert_eq!(fixture.cpu.regs.pc, 0x3000);
        assert_eq!(fixture.cpu.regs.sp, sp_before.wrapping_sub(3));
        assert!(fixture.cpu.regs.interrupt_disable());

        let sp = fixture.cpu.regs.sp;
        let pushed_p = fixture.mem[0x0100 | usize::from(sp.wrapping_add(1))];
        let pushed_pcl = fixture.mem[0x0100 | usize::from(sp.wrapping_add(2))];
        let pushed_pch = fixture.mem[0x0100 | usize::from(sp.wrapping_add(3))];
        assert_eq!(pushed_pcl, 0x02);
        assert_eq!(pushed_pch, 0x04);
        assert!(pushed_p & registers::FLAG_B != 0);
    }

    #[test]
    fn nop_takes_two_cycles() {
        let mut fixture = Fixture::with_program(0x0400, &[0xEA]);
        fixture.boot();
        let cycles = fixture.run_one();
        assert_eq!(cycles, 2);
        assert_eq!(fixture.cpu.regs.pc, 0x0401);
    }

    #[test]
    fn inx_increments_x_register() {
        let mut fixture = Fixture::with_program(0x0400, &[0xA2, 0x41, 0xE8]);
        fixture.boot();
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.x, 0x41);
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.x, 0x42);
    }
}
