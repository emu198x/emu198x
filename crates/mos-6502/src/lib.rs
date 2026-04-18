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
    pub reset_phase: u8,
    /// Penultimate-cycle IRQ sample. Updated on every non-final cycle
    /// of an instruction; the value left here when a helper returns
    /// done=true is the sample from the penultimate cycle, which is
    /// what real 6502 hardware uses to decide whether to service an
    /// IRQ at the end of the current instruction.
    pub(crate) pending_irq_line: bool,
    /// I-bit state captured at the same moment as pending_irq_line.
    /// Gives CLI/SEI/PLP the correct one-instruction delay: the I-bit
    /// change in the current instruction doesn't affect IRQ servicing
    /// until the NEXT instruction boundary.
    pub(crate) pending_i_mask: bool,
    /// NMI edge-detect latch for penultimate-cycle sampling. When the
    /// NMI line rises (low-to-high active-low = high-to-low signal)
    /// during any non-final cycle, this latch is set and remains set
    /// until the NMI is serviced.
    pub(crate) pending_nmi: bool,
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
            pending_irq_line: false,
            pending_i_mask: true,
            pending_nmi: false,
        }
    }

    pub fn reset(&mut self) {
        // Per MCS6500 reference: reset is a 7-cycle sequence that
        // decrements SP three times (phantom pushes of PC + P, but the
        // bus is held read-only so no actual store occurs), sets I=1,
        // and fetches the reset vector at $FFFC/$FFFD. NMOS reset does
        // NOT clear A/X/Y/D or other P bits — those retain their
        // previous values.
        self.total_cycles = 0;
        self.cs.reset();
        self.halted = false;
        self.irq = false;
        self.nmi = false;
        self.nmi_prev = false;
        self.rdy = true;
        // Only I and SP are touched; leave A/X/Y/D/V/N/Z/C alone.
        self.regs.set_flag(registers::FLAG_I, true);
        self.regs.sp = self.regs.sp.wrapping_sub(3);
        // 7 reset cycles total: 2 internal + 3 phantom pushes + 2 vector reads.
        // We model the tail (vector low + vector high) via reset_phase.
        self.reset_phase = 7;
        self.addr = 0x0100u16.wrapping_add(u16::from(self.regs.sp));
        self.rw = true;
        self.sync = false;
        self.data = 0;
        self.data_in = 0;
        self.so = true;
        self.so_prev = true;
        self.pending_irq_line = false;
        self.pending_i_mask = true;
        self.pending_nmi = false;
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
    fn reset_schedules_seven_cycle_sequence() {
        let mut cpu = M6502::new();
        cpu.reset();
        assert_eq!(cpu.reset_phase, 7);
        assert!(cpu.rw);
        assert!(!cpu.sync);
        assert!(!cpu.instruction_complete());
    }

    #[test]
    fn reset_preserves_a_x_y_and_decimal() {
        let mut cpu = M6502::new();
        cpu.regs.a = 0x42;
        cpu.regs.x = 0x84;
        cpu.regs.y = 0x21;
        cpu.regs.set_flag(registers::FLAG_D, true);
        cpu.reset();
        assert_eq!(cpu.regs.a, 0x42);
        assert_eq!(cpu.regs.x, 0x84);
        assert_eq!(cpu.regs.y, 0x21);
        assert!(cpu.regs.flag(registers::FLAG_D));
        assert!(cpu.regs.interrupt_disable()); // I is forced to 1
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
    fn rdy_low_stalls_read_without_cycle_advance() {
        let mut cpu = M6502::new();
        cpu.rdy = false;
        cpu.rw = true;
        let before = cpu.total_cycles;
        let ret = cpu.tick();
        assert!(!ret);
        assert_eq!(cpu.total_cycles, before);
    }

    #[test]
    fn rdy_high_allows_tick_to_advance() {
        let mut cpu = M6502::new();
        cpu.rdy = true;
        let before = cpu.total_cycles;
        cpu.tick();
        assert_eq!(cpu.total_cycles, before + 1);
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
            // Reset is now a 7-cycle sequence. Step through until the
            // CPU reports instruction_complete at sync.
            for _ in 0..7 {
                if self.step() && self.cpu.instruction_complete() {
                    break;
                }
            }
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

    // ─── Penultimate-cycle IRQ sampling / CLI delay tests (task #32) ──

    /// IRQ asserted BEFORE the instruction starts should be taken at
    /// the next boundary (provided I is clear).
    #[test]
    fn irq_asserted_before_instruction_is_serviced_at_boundary() {
        // Program: CLI then LDA #imm (to clear I first), then a NOP
        // where IRQ will fire. IRQ vector points at $3000.
        let mut fixture = Fixture::with_program(0x0400, &[0x58, 0xA9, 0x00, 0xEA]);
        fixture.mem[0xFFFE] = 0x00;
        fixture.mem[0xFFFF] = 0x30;
        fixture.boot();
        fixture.run_one(); // CLI — one-instruction delay means next insn still masked
        fixture.run_one(); // LDA — I clears by end of this; IRQ now unmasked
        fixture.cpu.irq = true;
        fixture.run_one(); // NOP — penultimate-cycle sample captures IRQ high + I clear
        // Next tick should vector to the IRQ handler.
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x3000);
    }

    /// IRQ asserted with I-disable set must NOT be serviced.
    #[test]
    fn irq_ignored_when_i_bit_set() {
        let mut fixture = Fixture::with_program(0x0400, &[0x78, 0xEA, 0xEA]);
        fixture.mem[0xFFFE] = 0x00;
        fixture.mem[0xFFFF] = 0x30;
        fixture.boot();
        fixture.run_one(); // SEI — I set by end
        fixture.cpu.irq = true;
        fixture.run_one();
        fixture.run_one();
        // PC must have advanced through both NOPs, not vectored.
        assert!(fixture.cpu.regs.pc >= 0x0402);
        assert_ne!(fixture.cpu.regs.pc, 0x3000);
    }

    /// CLI one-instruction delay: after CLI, the IMMEDIATELY following
    /// instruction still runs with the old (set) I-bit sampled at its
    /// penultimate cycle. Only the instruction AFTER that one sees the
    /// IRQ taken.
    #[test]
    fn cli_has_one_instruction_irq_delay() {
        // Start with I set (reset state), run CLI then NOP with IRQ
        // high. The IRQ should NOT be taken at the boundary after
        // CLI — it's taken after the NOP.
        let mut fixture = Fixture::with_program(0x0400, &[0x58, 0xEA, 0xEA]);
        fixture.mem[0xFFFE] = 0x00;
        fixture.mem[0xFFFF] = 0x30;
        fixture.boot();
        assert!(fixture.cpu.regs.interrupt_disable()); // I=1 after reset
        fixture.cpu.irq = true;
        fixture.run_one(); // CLI — I cleared at end; penultimate sample had I=1
        // IRQ must not vector yet — NOP runs first.
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
        // Now NOP's penultimate cycle samples IRQ high + I clear; next
        // boundary vectors.
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x3000);
    }
}
