//! `Cpu68030`: wrapper around [`motorola_68020::Cpu68020`] via the
//! family variant pattern (see
//! [`knowledge/decisions/motorola-68k-variant-pattern.md`]).
//!
//! The 68030 is a strict ISA superset of the 68020 — every
//! instruction the 68020 implements behaves identically on the
//! 68030. The 68030 deltas are the on-die PMMU
//! (PMOVE / PFLUSH / PTEST / PLOAD via F-line cpID=0), the data
//! cache, burst-fill bus cycles, and the Format `$B` long-bus-error
//! exception frame. The wrapper currently installs the MC68030 CACR
//! layout and external CDIS behaviour; the data-cache, burst, and MMU
//! datapaths remain separate implementation work.
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
/// hook, and variant behaviour flags through the inner core. It layers
/// the MC68030 cache-control register semantics over that inherited core.
#[derive(Clone, serde::Serialize)]
pub struct Cpu68030 {
    inner: Cpu68020,
}

impl Cpu68030 {
    /// Create a 68030 with the full 68020 hook chain inherited.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut cpu = Self {
            inner: Cpu68020::new(),
        };
        cpu.install_variant_hooks();
        cpu
    }

    /// Install the MC68030-specific cache-control binding.
    fn install_variant_hooks(&mut self) {
        // MC68030UM §6.3.1: bits 4-0 control the instruction cache and
        // bits 13-8 control the data cache. CI/CEI/CD/CED are momentary
        // clear commands and always read zero.
        self.inner.variant_cacr_write_mask = 0x0000_3F1F;
        self.inner.variant_cacr_read_zero_mask = 0x0000_0C0C;
    }

    /// Drive the MC68030 external cache-disable input.
    ///
    /// `asserted = true` corresponds to the active-low CDIS pin being
    /// asserted. Hits and fills are suppressed while asserted; existing
    /// entries are retained and become available when it is negated.
    pub fn set_cdis_asserted(&mut self, asserted: bool) {
        self.inner.variant_cache_disable_asserted = asserted;
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
        // Deserializing the inner Cpu68020 recursively restores the lower
        // variant bindings. Reinstall the MC68030 CACR layout on top.
        #[derive(serde::Deserialize)]
        struct Bare {
            inner: Cpu68020,
        }
        let bare = Bare::deserialize(d)?;
        let mut cpu = Self { inner: bare.inner };
        cpu.install_variant_hooks();
        Ok(cpu)
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu68030;

    const CACR_DEFINED: u32 = 0x0000_3F1F;
    const CACR_MOMENTARY: u32 = 0x0000_0C0C;
    const CACR_PERSISTENT: u32 = 0x0000_3313;

    fn movec_to_cacr(cpu: &mut Cpu68030, value: u32) {
        cpu.regs.sr |= 0x2000;
        cpu.regs.d[0] = value;
        cpu.irc = 0x0002; // D0, CACR
        assert!(motorola_68020::cpu::decode_68020_opcode(cpu, 0x4E7B));
    }

    fn movec_from_cacr(cpu: &mut Cpu68030, data_register: usize) {
        cpu.regs.sr |= 0x2000;
        cpu.irc = ((data_register as u16) << 12) | 0x0002;
        assert!(motorola_68020::cpu::decode_68020_opcode(cpu, 0x4E7A));
    }

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
        assert_eq!(cpu.variant_cacr_write_mask, CACR_DEFINED);
        assert_eq!(cpu.variant_cacr_read_zero_mask, CACR_MOMENTARY);
    }

    #[test]
    fn movec_to_cacr_masks_reserved_and_momentary_bits() {
        let mut cpu = Cpu68030::new();

        movec_to_cacr(&mut cpu, u32::MAX);
        assert_eq!(cpu.regs.cacr, CACR_PERSISTENT);

        movec_from_cacr(&mut cpu, 1);
        assert_eq!(cpu.regs.d[1], CACR_PERSISTENT);
    }

    #[test]
    fn movec_from_cacr_never_reports_clear_commands() {
        let mut cpu = Cpu68030::new();
        cpu.regs.cacr = CACR_MOMENTARY;

        movec_from_cacr(&mut cpu, 0);

        assert_eq!(cpu.regs.d[0], 0);
    }

    #[test]
    fn instruction_clear_commands_act_and_read_zero() {
        let mut cpu = Cpu68030::new();
        let selected = 0x0000_1000;
        let retained = 0x0000_1004;
        let cache = cpu.variant_icache.as_mut().expect("MC68030 I-cache");
        cache.fill(selected, true, 0x4E71);
        cache.fill(retained, true, 0x4E75);
        cpu.regs.caar = selected;

        movec_to_cacr(&mut cpu, 0x0000_0004); // CEI
        let cache = cpu.variant_icache.as_ref().expect("MC68030 I-cache");
        assert_eq!(cache.lookup(selected, true), None);
        assert_eq!(cache.lookup(retained, true), Some(0x4E75));
        assert_eq!(cpu.regs.cacr, 0);

        movec_to_cacr(&mut cpu, 0x0000_0008); // CI
        let cache = cpu.variant_icache.as_ref().expect("MC68030 I-cache");
        assert_eq!(cache.lookup(retained, true), None);
        assert_eq!(cpu.regs.cacr, 0);
    }

    #[test]
    fn data_clear_commands_do_not_clear_instruction_cache() {
        let mut cpu = Cpu68030::new();
        let addr = 0x0000_1000;
        cpu.variant_icache
            .as_mut()
            .expect("MC68030 I-cache")
            .fill(addr, true, 0x4E71);

        movec_to_cacr(&mut cpu, 0x0000_0C00); // CD | CED

        assert_eq!(cpu.regs.cacr, 0);
        assert_eq!(
            cpu.variant_icache
                .as_ref()
                .expect("MC68030 I-cache")
                .lookup(addr, true),
            Some(0x4E71)
        );
    }

    #[test]
    fn reset_disables_and_invalidates_instruction_cache() {
        let mut cpu = Cpu68030::new();
        let addr = 0x0000_1000;
        cpu.regs.cacr = 0x0000_0101;
        cpu.variant_icache
            .as_mut()
            .expect("MC68030 I-cache")
            .fill(addr, true, 0x4E71);

        cpu.reset_to(0x0000_2000, 0x0000_1000);

        assert_eq!(cpu.regs.cacr, 0);
        assert_eq!(
            cpu.variant_icache
                .as_ref()
                .expect("MC68030 I-cache")
                .lookup(addr, true),
            None
        );
    }

    #[test]
    fn deserialize_restores_mc68030_cache_control_and_dynamic_bus_sizing() {
        let mut cpu = Cpu68030::new();
        cpu.variant_dynamic_bus_sizing = false;
        cpu.variant_cacr_write_mask = 0;
        cpu.variant_cacr_read_zero_mask = 0;
        let encoded = rmp_serde::to_vec_named(&cpu).expect("serialize MC68030");

        let mut restored: Cpu68030 = rmp_serde::from_slice(&encoded).expect("deserialize MC68030");

        assert!(restored.variant_dynamic_bus_sizing);
        movec_to_cacr(&mut restored, u32::MAX);
        assert_eq!(restored.regs.cacr, CACR_PERSISTENT);
    }

    #[test]
    fn deserialize_preserves_warm_instruction_cache() {
        let mut cpu = Cpu68030::new();
        let addr = 0x0000_1000;
        cpu.variant_icache
            .as_mut()
            .expect("MC68030 I-cache")
            .fill(addr, true, 0x4E71);
        let encoded = rmp_serde::to_vec_named(&cpu).expect("serialize MC68030");

        let restored: Cpu68030 = rmp_serde::from_slice(&encoded).expect("deserialize MC68030");

        assert_eq!(
            restored
                .variant_icache
                .as_ref()
                .expect("restored MC68030 I-cache")
                .lookup(addr, true),
            Some(0x4E71)
        );
    }
}
