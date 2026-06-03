//! Shared single-instruction stepping for Z80-based machines.
//!
//! Every Z80 machine's debug surface needs "advance exactly one instruction".
//! The naive loop — tick while [`Z80::instruction_complete`] is false, then
//! while it is true — silently over-runs one-M-cycle instructions: that flag
//! is true throughout the next opcode fetch and flips false→true *within* a
//! single tick for a short op, so a between-tick check never sees the false.
//!
//! [`Z80Stepper`] centralises the correct loop, which watches the monotonic
//! [`Z80::instructions_retired`] counter instead. A machine supplies two tiny
//! hooks; the boundary logic lives here, once, so every machine — and the next
//! one added — single-steps correctly for free.
//!
//! [`Z80::instruction_complete`]: crate::Z80::instruction_complete
//! [`Z80::instructions_retired`]: crate::Z80::instructions_retired

/// Single-instruction stepping for a machine driven by a [`crate::Z80`].
pub trait Z80Stepper {
    /// Half-cycle safety cap per step, so a pathological non-terminating
    /// instruction (a chip bug, never user input) can't hang the caller. The
    /// longest real Z80 instruction or interrupt-acknowledge sequence is well
    /// under this; override only with a specific reason.
    const STEP_BUDGET: u32 = 1024;

    /// The CPU's retired-instruction count ([`crate::Z80::instructions_retired`]).
    fn z80_instructions_retired(&self) -> u64;

    /// Advance the whole machine by one Z80 half-cycle — tick the CPU and
    /// every chip on its clock (exactly one `Z80::tick`).
    fn step_tick(&mut self);

    /// Tick until exactly one Z80 instruction retires; return the half-cycles
    /// consumed. Stops on the retirement-counter edge, so it is immune to the
    /// transient `instruction_complete` re-assertion that over-runs short ops.
    fn step_instruction(&mut self) -> u64 {
        let target = self.z80_instructions_retired().wrapping_add(1);
        let mut ticks = 0u64;
        while ticks < u64::from(Self::STEP_BUDGET) {
            self.step_tick();
            ticks += 1;
            if self.z80_instructions_retired() == target {
                break;
            }
        }
        ticks
    }
}
