//! Interrupt-master-enable control and SoC-level halt instructions.
//!
//! - `DI` / `EI` — toggle the interrupt master enable. `EI` arms the
//!   one-instruction delay used by the dispatch path.
//! - `HALT` — suspend CPU clock until an interrupt becomes pending,
//!   with the documented "HALT bug" when `IME=0` and an interrupt is
//!   already pending.
//! - `STOP` — halt the entire SoC (CPU + LCD) until a button-press
//!   wakes it. Modelled as a sticky flag here; the machine layer
//!   resets it when the joypad pin transitions.

use crate::Sm83;

impl Sm83 {
    /// `DI` — clear `IME` and any pending promotion.
    pub(super) fn op_di(&mut self) {
        self.ime = false;
        self.ime_pending = false;
        self.finish_instruction();
    }

    /// `EI` arms the one-instruction delay: `ime_pending` flips at
    /// this m-cycle, and the next opcode boundary promotes it to
    /// `ime`. That's why a pending interrupt right after an `EI`
    /// isn't taken until the instruction following `EI` completes —
    /// exactly what Blargg and mooneye-gb verify.
    pub(super) fn op_ei(&mut self) {
        self.ime_pending = true;
        self.finish_instruction();
    }

    /// `HALT` — see crate-level `halt_mode` / `halt_bug`.
    pub(super) fn op_halt(&mut self) {
        // HALT bug: with IME=0 and an interrupt already pending, the
        // CPU does NOT enter halt_mode. Instead, the byte following
        // HALT is fetched but PC doesn't advance past it, causing a
        // single-byte re-execution. The latch is consumed on the next
        // opcode boundary.
        if !self.ime && self.irq_pending != 0 {
            self.halt_bug = true;
        } else {
            self.halt_mode = true;
        }
        self.finish_instruction();
    }

    /// `STOP` — set the sticky `stopped` flag and skip the second
    /// instruction byte.
    pub(super) fn op_stop(&mut self) {
        // STOP is a two-byte instruction that skips the second byte
        // (documented as "usually $00"). On real hardware it halts
        // the entire SoC until a button press; here we stash the
        // condition and let the machine clear it when the joypad
        // transitions.
        self.pc = self.pc.wrapping_add(1);
        self.stopped = true;
        self.finish_instruction();
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{TestBus, boot};
    use crate::Sm83;

    // -- HALT, STOP, DI, EI ---------------------------------------------

    #[test]
    fn halt_enters_halt_mode_when_ime_clear_and_no_irq() {
        let (mut cpu, mut bus) = boot(&[0x76]);
        cpu.ime = false;
        cpu.irq_pending = 0;
        bus.step(&mut cpu);
        assert!(cpu.halt_mode);
        assert!(!cpu.halt_bug);
    }

    #[test]
    fn halt_triggers_halt_bug_when_ime_clear_and_irq_pending() {
        let (mut cpu, mut bus) = boot(&[0x76]);
        cpu.ime = false;
        cpu.irq_pending = 0x01;
        bus.step(&mut cpu);
        assert!(!cpu.halt_mode);
        assert!(cpu.halt_bug);
    }

    #[test]
    fn stop_sets_stopped_flag_and_skips_next_byte() {
        let (mut cpu, mut bus) = boot(&[0x10, 0x00]);
        bus.step(&mut cpu);
        assert!(cpu.stopped);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn di_clears_ime_and_pending() {
        let (mut cpu, mut bus) = boot(&[0xF3]);
        cpu.ime = true;
        cpu.ime_pending = true;
        bus.step(&mut cpu);
        assert!(!cpu.ime);
        assert!(!cpu.ime_pending);
    }

    #[test]
    fn ei_then_nop_delays_ime_by_one_instruction() {
        let (mut cpu, mut bus) = boot(&[0xFB, 0x00]); // EI ; NOP
        assert!(!cpu.ime);
        bus.step(&mut cpu); // EI
        assert!(!cpu.ime, "EI alone must not enable IME yet");
        assert!(cpu.ime_pending);
        bus.step(&mut cpu); // NOP (promotion happens at this boundary)
        assert!(cpu.ime);
    }

    // -- EI + IRQ dispatch interaction (lives here because the EI half
    //    of the test is the load-bearing semantic). ---------------------

    #[test]
    fn ei_then_nop_then_irq_dispatches_after_nop_completes() {
        let mut bus = TestBus::new();
        // EI at $0100, NOP at $0101, NOP at $0102 (return address).
        bus.ram[0x0100] = 0xFB;
        bus.ram[0x0101] = 0x00;
        bus.ram[0x0102] = 0x00;
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.irq_pending = 0x01;

        bus.step(&mut cpu); // EI runs — IME still off at this boundary
        assert!(!cpu.ime);
        assert!(cpu.ime_pending);
        assert!(!cpu.dispatching);

        bus.step(&mut cpu); // NOP boundary: dispatch check sees ime=false → no dispatch; IME promotes after.
        assert!(cpu.ime);
        assert!(!cpu.dispatching, "dispatch must wait until after the NOP");
        assert_eq!(cpu.pc, 0x0102, "NOP at $0101 has completed");

        // Boundary that follows NOP: dispatch fires (5 ticks).
        bus.run(&mut cpu, 5);
        assert!(!cpu.dispatching);
        assert_eq!(cpu.pc, 0x0040);
        // Pushed return address is $0102.
        assert_eq!(bus.ram[(cpu.sp.wrapping_add(1)) as usize], 0x01);
        assert_eq!(bus.ram[cpu.sp as usize], 0x02);
    }

    // -- HALT and HALT bug ----------------------------------------------

    #[test]
    fn halt_with_ime_set_dispatches_when_irq_arrives() {
        let mut bus = TestBus::new();
        bus.ram[0x0100] = 0x76; // HALT
        bus.ram[0x0101] = 0x00; // NOP (return address)
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.ime = true;

        bus.step(&mut cpu); // HALT runs
        assert!(cpu.halt_mode);
        assert!(!cpu.halt_bug);

        // CPU stays halted while no IRQ is pending.
        for _ in 0..5 {
            bus.step(&mut cpu);
        }
        assert!(cpu.halt_mode);
        assert_eq!(cpu.pc, 0x0101);

        // Inject an IRQ. The next tick wakes HALT and the boundary
        // immediately enters dispatch.
        cpu.irq_pending = 0x01;
        bus.run(&mut cpu, 5);
        assert!(!cpu.halt_mode);
        assert_eq!(cpu.pc, 0x0040);
        assert_eq!(bus.ram[cpu.sp as usize], 0x01); // pushed PC lo == $01
        assert_eq!(bus.ram[(cpu.sp.wrapping_add(1)) as usize], 0x01);
    }

    #[test]
    fn halt_with_ime_clear_wakes_without_dispatching() {
        let mut bus = TestBus::new();
        bus.ram[0x0100] = 0x76; // HALT
        bus.ram[0x0101] = 0x3E; // LD A, $42 (proves PC continues normally)
        bus.ram[0x0102] = 0x42;
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.ime = false;
        cpu.irq_pending = 0;

        bus.step(&mut cpu); // HALT runs (no halt-bug — irq_pending == 0)
        assert!(cpu.halt_mode);

        bus.step(&mut cpu); // halt loop, still no IRQ
        assert!(cpu.halt_mode);

        cpu.irq_pending = 0x01; // inject IRQ
        bus.step(&mut cpu); // wake; falls through to opcode at $0101 (LD A,d8 m-cycle 1)
        assert!(!cpu.halt_mode);
        assert!(!cpu.dispatching, "no dispatch — IME is clear");

        bus.step(&mut cpu); // LD A,d8 m-cycle 2 latches A
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn halt_bug_causes_next_opcode_to_re_execute() {
        // HALT at $0100 with IME=0 + irq_pending=0x01 triggers the
        // HALT bug: the byte at $0101 is fetched, but PC fails to
        // advance past it the first time. So an INC A at $0101 runs
        // twice in a row.
        let mut bus = TestBus::new();
        bus.ram[0x0100] = 0x76; // HALT
        bus.ram[0x0101] = 0x3C; // INC A
        bus.ram[0x0102] = 0x00; // NOP
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        cpu.a = 0x00;
        cpu.f = 0;
        cpu.ime = false;
        cpu.irq_pending = 0x01;

        bus.step(&mut cpu); // HALT runs — not halt_mode, halt_bug latched.
        assert!(!cpu.halt_mode);
        assert!(cpu.halt_bug);
        assert_eq!(cpu.pc, 0x0101);

        // First INC A — boundary clears halt_bug, PC stays at $0101.
        bus.step(&mut cpu);
        assert!(!cpu.halt_bug);
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.pc, 0x0101, "PC failed to advance — that's the bug");

        // Second time the byte is fetched, PC advances normally.
        bus.step(&mut cpu);
        assert_eq!(cpu.a, 0x02);
        assert_eq!(cpu.pc, 0x0102);
    }

}
