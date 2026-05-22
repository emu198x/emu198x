//! `Cpu68040`: wrapper around [`motorola_68030::Cpu68030`] via the
//! family variant pattern (see
//! [`knowledge/decisions/motorola-68k-variant-pattern.md`]).
//!
//! The 68040 is a strict ISA superset of the 68030 — every
//! instruction the 68030 implements behaves identically on the
//! 68040. The 68040 deltas are MOVE16 (cache-line-sized memory
//! moves), CINV / CPUSH (cache control), the on-die FPU, the
//! direct PFLUSH / PTEST encoding (distinct from the 68030's
//! F-line cpID=0 form), and the Format `$7` long-bus-error
//! exception frame. None of these are exercised by the
//! `m68k-test-gen` corpus today, so this wrapper installs no extra
//! hooks or flags.
//!
//! When MOVE16 / CINV / CPUSH land, `Cpu68040::new()` will install
//! a `decode_68040_opcode` hook that chains to the 68030's hook.
//! The 705-line `motorola-68040/src/fpu.rs` is already in place and
//! will route through the same hook for F-line cpID=1 dispatch.

use std::ops::{Deref, DerefMut};

use motorola_68030::Cpu68030;

/// Motorola 68040 CPU.
///
/// Wraps a [`Cpu68030`] and inherits the full hook chain through
/// the inner core: 68020 + 68010 decode/continue hooks, all the
/// variant flags, the BCD / DIV / SR Musashi behaviour. No
/// 68040-specific deltas are configured yet.
pub struct Cpu68040 {
    inner: Cpu68030,
}

impl Cpu68040 {
    /// Create a 68040 with the full 68030 hook chain inherited.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: Cpu68030::new(),
        }
    }

    /// Borrow the wrapped 68030 core.
    #[must_use]
    pub const fn as_inner(&self) -> &Cpu68030 {
        &self.inner
    }

    /// Mutably borrow the wrapped 68030 core.
    #[must_use]
    pub const fn as_inner_mut(&mut self) -> &mut Cpu68030 {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped 68030 core.
    #[must_use]
    pub fn into_inner(self) -> Cpu68030 {
        self.inner
    }
}

impl Default for Cpu68040 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Cpu68040 {
    type Target = Cpu68030;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Cpu68040 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Cpu68040> for Cpu68030 {
    fn from(cpu: Cpu68040) -> Self {
        cpu.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu68040;

    #[test]
    fn new_inherits_supervisor_with_ipl_mask_seven() {
        let cpu = Cpu68040::new();
        assert!(cpu.regs.is_supervisor());
        assert_eq!(cpu.regs.interrupt_mask(), 7);
    }

    #[test]
    fn new_inherits_full_variant_chain() {
        // Deref-chain four layers deep: Cpu68040 → Cpu68030 →
        // Cpu68020 → Cpu68010 → Cpu68000. All variant flags set
        // anywhere in the chain should be visible.
        let cpu = Cpu68040::new();
        assert!(cpu.variant_decode_hook.is_some());
        assert!(cpu.variant_continue_hook.is_some());
        assert!(cpu.variant_scaled_index);
        assert!(cpu.variant_six_word_frame);
        assert!(cpu.variant_format2_vectors);
        assert!(cpu.variant_extended_sr_writes);
        assert!(cpu.variant_musashi_bcd_v);
        assert!(cpu.variant_musashi_div_overflow);
    }
}
