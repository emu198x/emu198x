//! `Cpu68030`: wrapper around [`motorola_68020::Cpu68020`] via the
//! family variant pattern (see
//! [`knowledge/decisions/motorola-68k-variant-pattern.md`]).
//!
//! The 68030 is a strict ISA superset of the 68020 — every
//! instruction the 68020 implements behaves identically on the
//! 68030. The 68030 deltas are the on-die PMMU
//! (PMOVE / PFLUSH / PTEST / PLOAD via F-line cpID=0), the data
//! cache, burst-fill bus cycles, and the Format `$B` long-bus-error
//! exception frame. None of these are exercised by the
//! `m68k-test-gen` corpus today, so this wrapper installs no extra
//! hooks or flags — the inner `Cpu68020` already configures
//! everything that's tested.
//!
//! When the MMU instructions land, `Cpu68030::new()` will install a
//! `decode_68030_opcode` hook that chains to the 68020's hook for
//! opcodes it doesn't own. Same shape as the 68020 chaining to the
//! 68010. The 2,421-line `motorola-68030/src/mmu.rs` already
//! contains the table-walk / ATC / TT machinery; it's waiting on
//! the decode-side wiring.

use std::ops::{Deref, DerefMut};

use motorola_68020::Cpu68020;

/// Motorola 68030 CPU.
///
/// Wraps a [`Cpu68020`] and inherits its decode hook, continue
/// hook, and variant behaviour flags through the inner core. No
/// 68030-specific deltas are configured yet — that work follows
/// once `mmu.rs` integration begins.
#[derive(Clone, serde::Serialize)]
pub struct Cpu68030 {
    inner: Cpu68020,
}

impl Cpu68030 {
    /// Create a 68030 with the full 68020 hook chain inherited.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: Cpu68020::new(),
        }
    }

    /// Borrow the wrapped 68020 core.
    #[must_use]
    pub const fn as_inner(&self) -> &Cpu68020 {
        &self.inner
    }

    /// Mutably borrow the wrapped 68020 core.
    #[must_use]
    pub const fn as_inner_mut(&mut self) -> &mut Cpu68020 {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped 68020 core.
    #[must_use]
    pub fn into_inner(self) -> Cpu68020 {
        self.inner
    }
}

impl Default for Cpu68030 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Cpu68030 {
    type Target = Cpu68020;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Cpu68030 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Cpu68030> for Cpu68020 {
    fn from(cpu: Cpu68030) -> Self {
        cpu.into_inner()
    }
}

impl<'de> serde::Deserialize<'de> for Cpu68030 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // The 68030 adds no hooks of its own (yet — PMOVE / PFLUSH /
        // PTEST will land here when the MMU is wired in). Deserializing
        // the inner Cpu68020 recursively restores every variant
        // binding through to the 68000 layer.
        #[derive(serde::Deserialize)]
        struct Bare {
            inner: Cpu68020,
        }
        let bare = Bare::deserialize(d)?;
        Ok(Self { inner: bare.inner })
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu68030;

    #[test]
    fn new_inherits_supervisor_with_ipl_mask_seven() {
        let cpu = Cpu68030::new();
        assert!(cpu.regs.is_supervisor());
        assert_eq!(cpu.regs.interrupt_mask(), 7);
    }

    #[test]
    fn new_inherits_68020_variant_flags() {
        // The 68030 wraps the 68020, which in turn wraps the 68010,
        // which wraps the 68000. The 68020-and-below flags should
        // all be set via the Deref chain.
        let cpu = Cpu68030::new();
        assert!(cpu.variant_decode_hook.is_some());
        assert!(cpu.variant_continue_hook.is_some());
        assert!(cpu.variant_scaled_index);
        assert!(cpu.variant_six_word_frame);
        assert!(cpu.variant_format2_vectors);
        assert!(cpu.variant_extended_sr_writes);
        assert!(cpu.variant_dynamic_bus_sizing);
        assert!(cpu.variant_musashi_bcd_v);
        assert!(cpu.variant_musashi_div_overflow);
    }

    #[test]
    fn deserialize_restores_dynamic_bus_sizing() {
        let mut cpu = Cpu68030::new();
        cpu.variant_dynamic_bus_sizing = false;
        let encoded = rmp_serde::to_vec_named(&cpu).expect("serialize MC68030");

        let restored: Cpu68030 = rmp_serde::from_slice(&encoded).expect("deserialize MC68030");

        assert!(restored.variant_dynamic_bus_sizing);
    }
}
