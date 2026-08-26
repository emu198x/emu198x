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
    /// Machine-step safety cap per instruction, so a pathological
    /// non-terminating instruction (a chip bug, never user input) cannot hang
    /// the caller. Every current machine implements one step as one Z80
    /// T-state; the longest real instruction or interrupt-acknowledge sequence
    /// is well under this cap. Override only with a specific reason.
    const STEP_BUDGET: u32 = 1024;

    /// The CPU's retired-instruction count ([`crate::Z80::instructions_retired`]).
    fn z80_instructions_retired(&self) -> u64;

    /// Advance the whole machine by one native CPU timing unit.
    ///
    /// Current machines use one Z80 T-state here and therefore call
    /// [`crate::Z80::tick`] twice: once for each half-cycle. The implementation
    /// must advance every other component that shares that machine timing unit
    /// as well.
    fn step_tick(&mut self);

    /// Step until exactly one Z80 instruction retires; return the machine
    /// timing units consumed. Stops on the retirement-counter edge, so it is
    /// immune to the transient `instruction_complete` re-assertion that
    /// over-runs short operations.
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
