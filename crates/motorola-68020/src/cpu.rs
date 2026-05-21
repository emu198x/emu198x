//! `Cpu68020`: wraps a [`Cpu68010`] core and chains the 68020 ISA
//! delta on top of the 68010's via the `variant_decode_hook` exposed
//! by [`motorola_68000::Cpu68000`].
//!
//! Per [`knowledge/decisions/motorola-68020-implementation-plan.md`]:
//! Phase 1 stood up the wrapper, Phase 1.5 layers the family
//! properly — every variant wraps the previous variant rather than
//! all wrapping the 68000 directly. So `Cpu68020` wraps `Cpu68010`,
//! `Cpu68010` wraps `Cpu68000`, and the 68020 hook chains to the
//! 68010 hook for opcodes the 68020 doesn't override.
//!
//! Today the 68020-specific hook only handles `EXTB.L`. Phase 5
//! grows it to cover the bit-field family, MULL/DIVL, CHK2/CMP2,
//! TRAPcc, PACK/UNPK, CAS/CAS2, Bcc.L, scaled-index, and full
//! extension words.
//!
//! 68020 control registers (MSP / CACR / CAAR / VBR / SFC / DFC)
//! all live on the shared [`motorola_68k_common::registers::Registers`]
//! struct — the wrapper itself has no fields beyond the inner core.
//!
//! Adapted from `Emu198x-Oldest/crates/motorola-68020/src/lib.rs`,
//! which used the same wrapper-plus-Deref pattern. The old codebase
//! wrapped a `CpuModel`-flagged inner 68000; the current shape
//! stacks variant crates instead and uses a per-variant decode hook.

use std::ops::{Deref, DerefMut};

use motorola_68000::Cpu68000;
use motorola_68010::Cpu68010;

/// Motorola 68020 CPU — wraps [`Cpu68010`] and chains the 68020 ISA
/// delta on top of the 68010 hook.
pub struct Cpu68020 {
    inner: Cpu68010,
}

impl Cpu68020 {
    /// Create a 68020 with the variant-decode hook chain installed.
    /// `new()` constructs the inner 68010 first (which installs the
    /// 68010 hook), then overrides the hook with the 68020's chained
    /// dispatcher.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut inner = Cpu68010::new();
        inner.variant_decode_hook = Some(decode_68020_opcode);
        // Brief-extension-word scale factor (Xn.SIZE*1/2/4/8) is
        // 68020+ behaviour. The 68010's hook leaves the flag false;
        // the 68020 enables it here so calc_ea_start consults bits
        // 10-9 of the extension word.
        inner.variant_scaled_index = true;
        Self { inner }
    }

    /// Borrow the wrapped 68010 core.
    #[must_use]
    pub const fn as_inner(&self) -> &Cpu68010 {
        &self.inner
    }

    /// Mutably borrow the wrapped 68010 core.
    #[must_use]
    pub const fn as_inner_mut(&mut self) -> &mut Cpu68010 {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped 68010 core.
    #[must_use]
    pub fn into_inner(self) -> Cpu68010 {
        self.inner
    }
}

impl Default for Cpu68020 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Cpu68020 {
    type Target = Cpu68010;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Cpu68020 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Cpu68020> for Cpu68010 {
    fn from(cpu: Cpu68020) -> Self {
        cpu.into_inner()
    }
}

// ─── Decode hook ──────────────────────────────────────────────────

use motorola_68010::decode_68010_opcode;
use motorola_68k_common::flags::{N, Z, V, C};

/// Hook installed on [`Cpu68000::variant_decode_hook`] by every
/// [`Cpu68020`] instance. Chains to the 68010 hook for opcodes the
/// 68020 doesn't override.
pub fn decode_68020_opcode(cpu: &mut Cpu68000, opcode: u16) -> bool {
    // 68020-specific opcodes first.
    if (opcode & 0xFFF8) == 0x49C0 {
        return execute_extb_l(cpu, opcode);
    }

    // Fall through to the 68010 hook for MOVEC / MOVE-from-CCR /
    // anything else the 68000 routes to ILLEGAL.
    decode_68010_opcode(cpu, opcode)
}

/// `EXTB.L Dn` ($49C0): sign-extend bit 7 of Dn through bits 31-8.
/// N = sign of result, Z = (result == 0), V = 0, C = 0, X
/// unaffected. 68020+ only.
fn execute_extb_l(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let reg = (opcode & 7) as usize;
    let val = cpu.regs.d[reg];
    let result = (val as u8 as i8 as i32) as u32;
    cpu.regs.d[reg] = result;

    let mut sr = cpu.regs.sr & !(N | Z | V | C);
    if result == 0 {
        sr |= Z;
    }
    if (result & 0x8000_0000) != 0 {
        sr |= N;
    }
    cpu.regs.sr = sr;
    true
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
        // MSP / VBR / CACR / CAAR live on the shared Registers struct
        // and are zero after Cpu68020::new() → Cpu68010::new() →
        // Cpu68000::new() → Registers::new().
        let cpu = Cpu68020::new();
        assert_eq!(cpu.regs.msp, 0);
        assert_eq!(cpu.regs.vbr, 0);
        assert_eq!(cpu.regs.cacr, 0);
        assert_eq!(cpu.regs.caar, 0);
    }

    #[test]
    fn msp_and_ssp_are_independent_fields() {
        // The 68020 MSP and ISP are distinct registers — the SR
        // M-flag (Phase 6) selects which one A7 currently aliases.
        let mut cpu = Cpu68020::new();
        cpu.regs.msp = 0xCAFE_BABE;
        cpu.regs.ssp = 0xDEAD_BEEF;
        assert_eq!(cpu.regs.msp, 0xCAFE_BABE);
        assert_eq!(cpu.regs.ssp, 0xDEAD_BEEF);
    }

    #[test]
    fn deref_chain_reaches_inner_68000_micro_op_pipeline() {
        // Cpu68020 derefs to Cpu68010 which derefs to Cpu68000, so
        // calling setup_prefetch on the wrapper reaches the inner
        // 68000 pipeline through the deref chain.
        let mut cpu = Cpu68020::new();
        cpu.regs.pc = 0x0000_1004;
        cpu.setup_prefetch(0x4E71, 0x4E71);
        assert_eq!(cpu.ir, 0x4E71);
        assert_eq!(cpu.irc, 0x4E71);
        assert_eq!(cpu.instr_start_pc, 0x0000_1000);
        assert_eq!(cpu.instruction_starts, 1);
    }

    #[test]
    fn extb_l_sign_extends_negative_byte() {
        // EXTB.L D0 with D0 = $1234_5680 → result = $FFFF_FF80,
        // N = 1, Z = 0, V = 0, C = 0.
        let mut cpu = Cpu68020::new();
        cpu.regs.d[0] = 0x1234_5680;
        cpu.regs.sr = 0x2000; // S=1, clear flags
        let handled = super::decode_68020_opcode(&mut cpu.inner, 0x49C0);
        assert!(handled);
        assert_eq!(cpu.regs.d[0], 0xFFFF_FF80);
        assert_eq!(cpu.regs.sr & 0x000F, 0x0008); // N set
    }

    #[test]
    fn extb_l_zero_extends_zero() {
        let mut cpu = Cpu68020::new();
        cpu.regs.d[1] = 0xABCD_EF00;
        cpu.regs.sr = 0x2000;
        let handled = super::decode_68020_opcode(&mut cpu.inner, 0x49C1);
        assert!(handled);
        assert_eq!(cpu.regs.d[1], 0x0000_0000);
        assert_eq!(cpu.regs.sr & 0x000F, 0x0004); // Z set
    }

    #[test]
    fn extb_l_chains_to_68010_hook_for_movec() {
        // The 68020 hook must fall through to the 68010 hook for
        // MOVEC, which the 68020 doesn't override.
        let mut cpu = Cpu68020::new();
        cpu.regs.sr |= 0x2000;
        cpu.regs.d[0] = 0x1000;
        // MOVEC D0, VBR — ext word: DA=0, Reg=0, CR=$801
        cpu.irc = 0x801;
        let handled = super::decode_68020_opcode(&mut cpu.inner, 0x4E7B);
        assert!(handled);
        assert_eq!(cpu.regs.vbr, 0x1000);
    }
}
