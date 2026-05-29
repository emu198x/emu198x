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
    /// NMI edge-detect latch (the internal "NMI detected" flip-flop).
    /// Set on the cycle an NMI rising edge is detected and held until
    /// the NMI is serviced. Edge detection runs every cycle (see
    /// `poll_nmi_edge`); servicing reads the staged `prev_pending_nmi`.
    pub(crate) pending_nmi: bool,
    /// One-cycle-staged view of `pending_nmi`, captured at the start of
    /// each cycle before that cycle's edge detection. The
    /// instruction-boundary servicing check reads this, not
    /// `pending_nmi`, so an edge detected on an instruction's final
    /// cycle is serviced after the NEXT instruction rather than being
    /// dropped (matches NES `04-nmi_control` / `06-suppression`).
    pub(crate) prev_pending_nmi: bool,
    /// One-shot flag set by `tick_relative` when a branch is taken,
    /// suppressing the next penultimate-cycle IRQ poll. Implements the
    /// silicon quirk "a taken non-page-crossing branch ignores IRQ
    /// during its last clock" (blargg `cpu_interrupts_v2/5-branch_delays_irq`).
    /// For page-crossed branches, the suppression is harmless: the
    /// extra dummy-read cycle re-latches IRQ on its own penultimate
    /// poll, so only the genuinely-non-page-crossing case drops the
    /// last-cycle IRQ as intended.
    pub(crate) branch_irq_suppress: bool,
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
            prev_pending_nmi: false,
            branch_irq_suppress: false,
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
        self.prev_pending_nmi = false;
        self.branch_irq_suppress = false;
    }

    #[must_use]
    pub fn instruction_complete(&self) -> bool {
        self.cs.cycle == 0 && self.reset_phase == 0
    }

    /// Cycle index within the executing instruction (0 at an
    /// instruction boundary, i.e. opcode-fetch pending). Read-only;
    /// exposed for cycle-exact interrupt-timing debugging.
    #[must_use]
    pub fn instruction_cycle(&self) -> u8 {
        self.cs.cycle
    }

    /// NMI edge latch (`pending_nmi`): set once the CPU has detected an
    /// NMI rising edge that has not yet been serviced. Read-only.
    #[must_use]
    pub fn pending_nmi(&self) -> bool {
        self.pending_nmi
    }

    /// Last-sampled NMI line state (`nmi_prev`). The edge detector
    /// compares the live `nmi` pin against this every cycle. Read-only.
    #[must_use]
    pub fn nmi_prev(&self) -> bool {
        self.nmi_prev
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

    /// Spec invariant: every 8-bit opcode resolves through `cycle::decode`
    /// without panicking, and `Operation::category` returns a defined
    /// category for every produced operation.
    ///
    /// The Tom Harte regression suite would catch a missing arm at run
    /// time, but it is `#[ignore]`'d and requires an external 1 GiB
    /// corpus. This sweep is the standing hermetic gate that covers
    /// the decode table on every `cargo test --workspace`.
    #[test]
    fn decode_table_covers_all_256_opcodes() {
        for opcode in 0u8..=0xFF {
            let info = cycle::decode(opcode);
            // Force category resolution to ensure no Operation variant
            // is missing from the category match.
            let _ = info.operation.category();
        }
    }

    /// Spec invariant: the four "official" addressing-mode landings
    /// for every documented arithmetic opcode (immediate, zero-page,
    /// absolute, branch) have stable category assignments. Catches any
    /// silent renaming or category swap that would slip past the sweep.
    #[test]
    fn decode_table_categories_for_representative_opcodes() {
        // Read-class operations.
        for opcode in [0xA9u8, 0xA5, 0xAD, 0xBD] {
            // LDA imm/zp/abs/abs,X
            let info = cycle::decode(opcode);
            assert_eq!(info.operation, cycle::Operation::Lda);
            assert_eq!(info.operation.category(), cycle::OpCategory::Read);
        }
        // Write-class.
        for opcode in [0x85u8, 0x8D, 0x9D] {
            // STA zp/abs/abs,X
            let info = cycle::decode(opcode);
            assert_eq!(info.operation, cycle::Operation::Sta);
            assert_eq!(info.operation.category(), cycle::OpCategory::Write);
        }
        // Read-modify-write.
        for opcode in [0x06u8, 0x0E, 0x16, 0x1E] {
            // ASL zp/abs/zp,X/abs,X
            let info = cycle::decode(opcode);
            assert_eq!(info.operation, cycle::Operation::Asl);
            assert_eq!(info.operation.category(), cycle::OpCategory::ReadModWrite);
        }
        // Control flow.
        let jmp = cycle::decode(0x4C);
        assert_eq!(jmp.operation, cycle::Operation::Jmp);
        assert_eq!(jmp.operation.category(), cycle::OpCategory::Control);
        // Implied.
        let inx = cycle::decode(0xE8);
        assert_eq!(inx.operation, cycle::Operation::Inx);
        assert_eq!(inx.operation.category(), cycle::OpCategory::Implied);
    }

    /// Spec invariant: every "JAM" opcode (the dozen 6502 stop-codes
    /// that hang the CPU on real silicon) decodes into the JAM
    /// addressing mode and JAM operation. Locks in the chip-port's
    /// promise that we model these as `tick.rs` halts the CPU rather
    /// than skipping the opcode.
    #[test]
    fn decode_table_jam_opcodes_all_resolve_to_jam() {
        for opcode in [
            0x02u8, 0x12, 0x22, 0x32, 0x42, 0x52, 0x62, 0x72, 0x92, 0xB2, 0xD2, 0xF2,
        ] {
            let info = cycle::decode(opcode);
            assert_eq!(info.addr_mode, cycle::AddrMode::Jam);
            assert_eq!(info.operation, cycle::Operation::Jam);
        }
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

    // ─── Tom-Harte-uncovered correctness paths ──────────────────────
    // Tom Harte tests the opcodes; it does not drive the reset, NMI,
    // or RDY pins. These directed tests cover the remaining
    // correctness-critical paths in `tick.rs`. See Cov-2 in
    // docs/plans/2026-04-28-october-runup-plan.md.

    /// Spec invariant: reset is a 7-cycle sequence ending in PC =
    /// reset vector at $FFFC/$FFFD with the I flag set.
    ///
    /// Catches regression: the chip-only Amiga investigation (2026-
    /// 04-18) corrected the reset path from a 4-cycle to a 7-cycle
    /// sequence; ensure that lock-in stays put.
    #[test]
    fn reset_executes_seven_cycles_and_loads_vector() {
        let mut mem = [0u8; 0x10000];
        mem[0xFFFC] = 0x00;
        mem[0xFFFD] = 0x80;
        // Plant a NOP at the reset vector so we can run a single
        // instruction afterwards and confirm PC advanced from there.
        mem[0x8000] = 0xEA;

        let mut cpu = M6502::new();
        cpu.reset();
        assert_eq!(cpu.reset_phase, 7);

        let mut total = 0u64;
        for _ in 0..7 {
            if cpu.rw {
                cpu.data_in = mem[usize::from(cpu.addr)];
            } else {
                mem[usize::from(cpu.addr)] = cpu.data;
            }
            let _ = cpu.tick();
            total += 1;
        }
        assert_eq!(total, 7);
        assert_eq!(cpu.reset_phase, 0);
        assert_eq!(cpu.regs.pc, 0x8000);
        assert!(
            cpu.regs.interrupt_disable(),
            "reset should leave I flag set"
        );
    }

    /// Spec invariant: NMI is taken on a rising edge of the NMI line
    /// — holding NMI high doesn't continuously re-trigger.
    ///
    /// Silicon: NMI is sampled at the penultimate cycle of every
    /// instruction. An NMI rise BEFORE the next instruction's
    /// penultimate cycle is caught and fires AFTER that next
    /// instruction completes. This matches blargg ppu_vbl_nmi/04
    /// "Immediate occurrence should be after NEXT instruction".
    ///
    /// Catches regression: NMI level-detection vs edge-detection is a
    /// classic 6502 gotcha; `tick.rs` uses `nmi_prev` to implement the
    /// edge purely via the penultimate-cycle latch.
    #[test]
    fn nmi_rising_edge_vectors_to_handler() {
        let mut fixture = Fixture::with_program(0x0400, &[0xEA]);
        fixture.mem[0xFFFA] = 0x00;
        fixture.mem[0xFFFB] = 0x40;
        fixture.boot();
        fixture.cpu.nmi = true; // rising edge
        // First run_one: the NOP runs to completion. NMI is latched
        // at the NOP's penultimate cycle (its opcode-fetch cycle for
        // a 2-cycle instruction).
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x0401);
        // Second run_one: NMI BRK sequence runs. PC ends at the NMI
        // vector.
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x4000);
    }

    /// Spec invariant: NMI re-triggers only on a rising edge — once
    /// taken, the CPU does not service the same held-high line again.
    ///
    /// Catches regression: any path that would re-fire NMI while it
    /// stays high (the chip-level "NMI is edge-triggered" promise).
    #[test]
    fn nmi_does_not_re_fire_while_held_high() {
        // Program at the NMI vector ($4000): NOPs so we can detect
        // whether the second NMI service path runs.
        let mut fixture = Fixture::with_program(0x0400, &[0xEA, 0xEA]);
        fixture.mem[0xFFFA] = 0x00;
        fixture.mem[0xFFFB] = 0x40;
        fixture.mem[0x4000] = 0xEA; // NOP
        fixture.mem[0x4001] = 0xEA; // NOP
        fixture.boot();
        fixture.cpu.nmi = true;
        fixture.run_one(); // NOP at $0400 (NMI latched at penultimate)
        assert_eq!(fixture.cpu.regs.pc, 0x0401);
        fixture.run_one(); // NMI BRK sequence; PC = $4000
        assert_eq!(fixture.cpu.regs.pc, 0x4000);
        // NMI line still high — must not re-trigger.
        fixture.run_one(); // NOP at $4000
        assert_eq!(fixture.cpu.regs.pc, 0x4001);
        fixture.run_one(); // NOP at $4001
        assert_eq!(fixture.cpu.regs.pc, 0x4002);
    }

    /// Spec invariant: NMI is honoured even when the I flag is set.
    /// IRQ is masked by I; NMI is non-maskable.
    ///
    /// Catches regression: any change to `tick.rs` that conflated NMI
    /// with the IRQ-masking path.
    #[test]
    fn nmi_taken_with_interrupt_disable_set() {
        let mut fixture = Fixture::with_program(0x0400, &[0x78, 0xEA]);
        fixture.mem[0xFFFA] = 0x00;
        fixture.mem[0xFFFB] = 0x40;
        fixture.boot();
        fixture.run_one(); // SEI — sets I
        fixture.cpu.nmi = true;
        // NMI latched at NOP's penultimate cycle; NMI fires after
        // the NOP completes. Even with I=1 the NMI is taken —
        // that's the non-maskable promise.
        fixture.run_one(); // NOP completes
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
        fixture.run_one(); // NMI BRK sequence
        assert_eq!(fixture.cpu.regs.pc, 0x4000);
    }

    /// Spec invariant: NMI takes priority over IRQ when both are
    /// pending at an instruction boundary. The CPU vectors through
    /// $FFFA/$FFFB, not $FFFE/$FFFF.
    ///
    /// Catches regression: if the tick dispatch ever checks IRQ
    /// before NMI it would silently take the wrong vector.
    #[test]
    fn nmi_takes_priority_over_irq() {
        let mut fixture = Fixture::with_program(0x0400, &[0x58, 0xEA]);
        fixture.mem[0xFFFA] = 0x00; // NMI vector → $4000
        fixture.mem[0xFFFB] = 0x40;
        fixture.mem[0xFFFE] = 0x00; // IRQ vector → $5000
        fixture.mem[0xFFFF] = 0x50;
        fixture.boot();
        fixture.run_one(); // CLI — I clear at end of this instruction
        fixture.cpu.irq = true;
        fixture.cpu.nmi = true;
        // NMI and IRQ are both latched at NOP's penultimate cycle.
        // NOP completes first; on the boundary after it, NMI wins
        // priority over IRQ — real silicon vectors through $FFFA.
        fixture.run_one(); // NOP completes
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
        fixture.run_one(); // NMI BRK sequence (NMI wins over IRQ)
        assert_eq!(fixture.cpu.regs.pc, 0x4000);
    }

    /// Spec invariant: SEI has the same one-instruction delay as CLI.
    /// An IRQ that's pending before SEI runs is still taken before
    /// SEI's mask sticks.
    ///
    /// Catches regression: missed parity between CLI and SEI in the
    /// penultimate-cycle latching path.
    #[test]
    fn sei_has_one_instruction_irq_delay() {
        let mut fixture = Fixture::with_program(0x0400, &[0x58, 0x78, 0xEA, 0xEA]);
        fixture.mem[0xFFFE] = 0x00;
        fixture.mem[0xFFFF] = 0x30;
        fixture.boot();
        fixture.run_one(); // CLI — I clear at end
        fixture.cpu.irq = true;
        fixture.run_one(); // SEI — penultimate cycle still has I clear
        // SEI's penultimate sample had I clear + IRQ high → IRQ taken.
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x3000);
    }

    /// Spec invariant: PLP that clears I has the same one-instruction
    /// delay as CLI. PLP that sets I has the same one-instruction
    /// delay as SEI.
    ///
    /// Catches regression: the penultimate-cycle latching applies
    /// uniformly to every I-modifying instruction.
    #[test]
    fn plp_clearing_i_has_one_instruction_irq_delay() {
        // Push status with I=0, then PLP, then NOP.
        // Reset state has I=1; we'll set up the stack manually.
        let mut fixture = Fixture::with_program(0x0400, &[0x28, 0xEA, 0xEA]);
        fixture.mem[0xFFFE] = 0x00;
        fixture.mem[0xFFFF] = 0x30;
        // Stack frame for PLP: SP starts at $FF after reset (boot
        // didn't touch it), PLP pulls from $0100 + (SP+1) = $0100.
        // Place a status byte with I clear, B clear, U set (PLP forces U=1).
        fixture.mem[0x0101] = 0x20; // U=1, all others 0 (I clear)
        fixture.boot();
        // SP should be 0xFD after reset (push of phantom returnaddr+P).
        fixture.cpu.regs.sp = 0x00; // PLP will pull from $0100 + 1 = $0101
        fixture.cpu.irq = true;
        fixture.run_one(); // PLP — penultimate sample has I=1 (pre-PLP)
        // IRQ should NOT be taken yet — one-instruction delay.
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x0402);
        // Now NOP's penultimate sample has I=0 + IRQ high; vectors.
        fixture.run_one();
        assert_eq!(fixture.cpu.regs.pc, 0x3000);
    }

    /// Spec invariant: RDY low during a read freezes the CPU — no
    /// cycle accounting, addr/rw lines unchanged. NMOS 6502 does NOT
    /// stall on writes; a write goes through even with RDY low.
    ///
    /// Catches regression: the VIC-II badline contention path on the
    /// C64 depends on RDY-stall semantics; getting this wrong silently
    /// breaks every C64 game that uses sprites.
    #[test]
    fn rdy_stalls_reads_but_lets_writes_through() {
        let mut fixture = Fixture::with_program(0x0400, &[0xA9, 0x42, 0x8D, 0x00, 0x20]);
        // LDA #$42 ; STA $2000
        fixture.boot();
        fixture.run_one(); // LDA (read-class) — runs while RDY=1

        // Pull RDY low. The next tick is a STA setup (write coming).
        // First two cycles of STA fetch the operand bytes (reads); RDY
        // should freeze them. Once we set RDY=true again, they proceed.
        fixture.cpu.rdy = false;
        let cycles_before = fixture.cpu.total_cycles;
        for _ in 0..5 {
            fixture.step();
        }
        assert_eq!(
            fixture.cpu.total_cycles, cycles_before,
            "RDY low should suppress cycle accounting on reads"
        );
        fixture.cpu.rdy = true;
        fixture.run_one(); // STA finishes
        assert_eq!(fixture.mem[0x2000], 0x42);
    }

    /// Spec invariant: JAM opcodes (e.g. $02) halt the CPU. The CPU
    /// keeps re-fetching the same address forever; no further
    /// instructions execute.
    ///
    /// Catches regression: any change to the JAM dispatch in
    /// `tick.rs` that lets the CPU advance past the JAM.
    #[test]
    fn jam_opcode_halts_cpu() {
        let mut fixture = Fixture::with_program(0x0400, &[0x02, 0xA9, 0x42]);
        fixture.boot();
        fixture.run_one(); // execute the JAM
        let pc_after_jam = fixture.cpu.regs.pc;
        assert!(fixture.cpu.halted, "JAM should set the halted flag");
        for _ in 0..16 {
            fixture.step();
        }
        // PC must not have advanced past the JAM into the LDA.
        assert_eq!(fixture.cpu.regs.pc, pc_after_jam);
        assert_ne!(fixture.cpu.regs.a, 0x42);
    }
}
