//! `Cpu68020`: thin wrapper around the shared 68000 core that holds the
//! 68020-specific control-register state.
//!
//! Phase 1 of the implementation plan
//! ([`knowledge/decisions/motorola-68020-implementation-plan.md`]):
//! the wrapper exists, owns the new registers (MSP / VBR / CACR / CAAR),
//! and forwards everything else to [`Cpu68000`] via `Deref`. No
//! behaviour differs from the 68000 yet — that's deliberate; the
//! Phase 0 Tom Harte baseline is the floor we keep at 86.33 % while
//! the wrapper lands, and then climb from as later phases route
//! 68020-specific decode / addressing / exception handling through this
//! struct.
//!
//! Adapted from `Emu198x-Oldest/crates/motorola-68020/src/lib.rs`,
//! which used the same wrapper-plus-Deref pattern over a `CpuModel`-
//! parameterised `InnerCpu68000`. The current `motorola-68000` crate
//! no longer carries that capability flag (stripped 2026-04-29), so we
//! pin the variant identity in the type system instead of at runtime.

use std::ops::{Deref, DerefMut};

use motorola_68000::Cpu68000;

/// Motorola 68020 CPU.
///
/// Holds a [`Cpu68000`] core plus the four 68020 control registers
/// (Master Stack Pointer, Vector Base Register, Cache Control
/// Register, Cache Address Register). All 68000-shared behaviour is
/// reached through `Deref` / `DerefMut`; `Cpu68020`-specific fields
/// are accessed directly.
///
/// The CACR / CAAR fields will start mattering when Phase 7
/// (instruction cache) lands. MSP routes through Phase 6 (exception
/// frames). VBR is used by Phase 1.5 (MOVEC / 68010-era control
/// registers).
pub struct Cpu68020 {
    inner: Cpu68000,
    /// Master Stack Pointer. Selected when SR.M (bit 12) is set; the
    /// 68000-shared `regs.ssp` becomes the *Interrupt* Stack Pointer.
    pub msp: u32,
    /// Vector Base Register. Address of the start of the 256-vector
    /// exception table. On the 68000 the table is fixed at 0; on the
    /// 68010+ it can be relocated by writing this register via MOVEC.
    pub vbr: u32,
    /// Cache Control Register. Bits: EI (enable I-cache), FI (freeze),
    /// CI (clear I-cache), CD (clear data — vestigial on 68020).
    pub cacr: u32,
    /// Cache Address Register. Targets a single I-cache line for
    /// CINV / CPUSH.
    pub caar: u32,
}

impl Cpu68020 {
    /// Create a 68020 in reset state: the inner 68000 core is reset
    /// (supervisor, IPL mask 7, all GP regs zero) and the four
    /// 68020-specific registers start zero.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: Cpu68000::new(),
            msp: 0,
            vbr: 0,
            cacr: 0,
            caar: 0,
        }
    }

    /// Borrow the wrapped 68000 core directly.
    #[must_use]
    pub const fn as_inner(&self) -> &Cpu68000 {
        &self.inner
    }

    /// Mutably borrow the wrapped 68000 core directly.
    #[must_use]
    pub const fn as_inner_mut(&mut self) -> &mut Cpu68000 {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped 68000 core.
    #[must_use]
    pub fn into_inner(self) -> Cpu68000 {
        self.inner
    }
}

impl Default for Cpu68020 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Cpu68020 {
    type Target = Cpu68000;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Cpu68020 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Cpu68020> for Cpu68000 {
    fn from(cpu: Cpu68020) -> Self {
        cpu.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu68020;

    #[test]
    fn new_starts_supervisor_with_ipl_mask_seven_like_the_inner_core() {
        let cpu = Cpu68020::new();
        // Inner 68000 starts in supervisor mode with IPL mask 7.
        // Deref makes `cpu.regs` reach the inner state directly.
        assert!(cpu.regs.is_supervisor());
        assert_eq!(cpu.regs.interrupt_mask(), 7);
    }

    #[test]
    fn new_starts_with_zero_control_registers() {
        let cpu = Cpu68020::new();
        assert_eq!(cpu.msp, 0);
        assert_eq!(cpu.vbr, 0);
        assert_eq!(cpu.cacr, 0);
        assert_eq!(cpu.caar, 0);
    }

    #[test]
    fn control_registers_are_independent_of_the_inner_core() {
        // Mutating the wrapper's MSP must not touch the inner SSP,
        // and vice versa — the 68020 MSP and ISP are different
        // registers and Phase 6 will route them separately.
        let mut cpu = Cpu68020::new();
        cpu.msp = 0xCAFE_BABE;
        cpu.regs.ssp = 0xDEAD_BEEF;
        assert_eq!(cpu.msp, 0xCAFE_BABE);
        assert_eq!(cpu.regs.ssp, 0xDEAD_BEEF);
    }

    #[test]
    fn deref_mut_reaches_inner_micro_op_pipeline() {
        // setup_prefetch lives on Cpu68000; calling it through the
        // wrapper must initialise the inner pipeline so the Tom Harte
        // harness keeps working.
        let mut cpu = Cpu68020::new();
        cpu.regs.pc = 0x0000_1004;
        cpu.setup_prefetch(0x4E71, 0x4E71);
        assert_eq!(cpu.ir, 0x4E71);
        assert_eq!(cpu.irc, 0x4E71);
        assert_eq!(cpu.instr_start_pc, 0x0000_1000);
        assert_eq!(cpu.instruction_starts, 1);
    }
}
