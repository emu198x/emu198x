//! `Cpu68010`: wraps the shared 68000 core and installs the 68010
//! ISA delta via the `variant_decode_hook` exposed by
//! [`motorola_68000::Cpu68000`].
//!
//! The wrapper itself owns nothing the 68000 doesn't already have —
//! all 68010 control-register slots (`VBR`, `SFC`, `DFC`, …) live on
//! [`motorola_68k_common::registers::Registers`], which is shared
//! across every member of the family. The wrapper exists to:
//!
//! 1. Disambiguate the variant at the type level so machine code can
//!    construct "an A2000HD CPU" without runtime gating.
//! 2. Install the decode hook in [`Cpu68010::new`] so the 68000 core
//!    routes 68010-specific opcodes here instead of taking ILLEGAL.
//!
//! The hook in this crate covers the 68010-introduced opcodes that
//! complete in a single Execute step:
//!
//! - `MOVEC` (`$4E7A` / `$4E7B`) — read/write VBR / SFC / DFC / USP
//!   through the standard control-register namespace.
//! - `MOVE from CCR` (`$42C0` with mode 0) — the 68010 added a
//!   non-privileged read of the CCR alongside the 68000's privileged
//!   `MOVE from SR`. Memory destinations (modes 2-7) need the
//!   multi-step EA pipeline and are deferred to a later phase.
//!
//! Deferred to a later phase because they require multi-step
//! continuation dispatch (which today only the 68000 core knows
//! how to do for its own tags):
//!
//! - `RTD #d16` (`$4E74`) — pops PC then adjusts SP by d16.
//! - `MOVES` (`$0Exx`) — privileged data move using SFC/DFC.
//! - Loop mode optimisation on DBcc.
//! - 68010 6-word exception frames + format word.
//!
//! `BKPT` (`$4848`-`$484F`) is left to fall through to the 68000's
//! ILLEGAL trap, which matches the 68010 behaviour exactly when no
//! debug controller is attached.

use std::ops::{Deref, DerefMut};

use motorola_68000::Cpu68000;
use motorola_68000::microcode::MicroOp;

/// Variant follow-up tag reserved by the 68010 / 68020 / etc.
/// crates. The 68000 uses tag numbers up to ≈ 80; 200+ avoids any
/// collision.
///
/// RTD pop sequence: PopLongHi → save PC hi → PopLongLo → combine
/// PC, adjust SP by `d16`, finalise.
const TAG_RTD_PC_HI: u8 = 200;
const TAG_RTD_PC_LO: u8 = 201;

/// Motorola 68010 CPU.
///
/// A type wrapper over [`Cpu68000`] that installs the 68010 decode
/// hook. All 68000-shared state is reachable via `Deref` /
/// `DerefMut`; 68010-specific control registers (`VBR`, `SFC`,
/// `DFC`) live on `cpu.regs` and are shared with every 68k variant.
pub struct Cpu68010 {
    inner: Cpu68000,
}

impl Cpu68010 {
    /// Create a 68010 with the variant-decode hook installed.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut inner = Cpu68000::new();
        inner.variant_decode_hook = Some(decode_68010_opcode);
        inner.variant_continue_hook = Some(continue_68010_opcode);
        // 68010+ pushes an eight-byte exception frame with a
        // Format/Vector word at the top. The 68020 inherits this
        // through Cpu68010::new().
        inner.variant_six_word_frame = true;
        Self { inner }
    }

    /// Borrow the wrapped 68000 core.
    #[must_use]
    pub const fn as_inner(&self) -> &Cpu68000 {
        &self.inner
    }

    /// Mutably borrow the wrapped 68000 core.
    #[must_use]
    pub const fn as_inner_mut(&mut self) -> &mut Cpu68000 {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped 68000 core (with
    /// the variant hook still installed — strip it manually if you
    /// want a pure 68000).
    #[must_use]
    pub fn into_inner(self) -> Cpu68000 {
        self.inner
    }
}

impl Default for Cpu68010 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Cpu68010 {
    type Target = Cpu68000;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Cpu68010 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Cpu68010> for Cpu68000 {
    fn from(cpu: Cpu68010) -> Self {
        cpu.into_inner()
    }
}

// ─── Decode hook ──────────────────────────────────────────────────

/// Hook installed on [`Cpu68000::variant_decode_hook`] by every
/// [`Cpu68010`] instance. Returns `true` if the opcode was handled;
/// returning `false` lets the 68000 core dispatch its default
/// ILLEGAL trap. Public so [`crate::Cpu68020`]'s hook can chain to
/// this one for opcodes the 68020 doesn't override.
pub fn decode_68010_opcode(cpu: &mut Cpu68000, opcode: u16) -> bool {
    match opcode {
        // MOVE from CCR, Dn destination (mode 0).
        op if (op & 0xFFF8) == 0x42C0 => execute_move_from_ccr_to_dn(cpu, op),

        // RTD #d16 ($4E74). Pops PC from the stack, then adjusts SP
        // by a 16-bit sign-extended displacement that follows the
        // opcode as an extension word. Multi-step; bus cycles run
        // through the continuation hook.
        0x4E74 => execute_rtd(cpu),

        // MOVEC — read CR into Rn ($4E7A) / write Rn into CR ($4E7B).
        // Privileged; both forms take a single 16-bit extension word.
        0x4E7A => execute_movec_cr_to_rn(cpu),
        0x4E7B => execute_movec_rn_to_cr(cpu),

        // Everything else (BKPT, plus the 68020+ family) falls
        // through. The 68000's default ILLEGAL trap is the correct
        // 68010 behaviour for BKPT with no debugger attached.
        _ => false,
    }
}

/// Continuation hook installed alongside the decode hook in
/// [`Cpu68010::new`]. Dispatches the 68010-reserved follow-up tags;
/// returns `false` for any tag the 68010 doesn't claim so the
/// 68000's default `continue_instruction` dispatch can run.
pub fn continue_68010_opcode(cpu: &mut Cpu68000) -> bool {
    match cpu.followup_tag {
        TAG_RTD_PC_HI => {
            // PopLongHi has placed the PC high word in self.data
            // (already shifted << 16). Queue PopLongLo to combine.
            cpu.followup_tag = TAG_RTD_PC_LO;
            cpu.micro_ops.push(MicroOp::PopLongLo);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        TAG_RTD_PC_LO => {
            // self.data now holds the full 32-bit return address.
            // The stack pointer has already advanced 4 bytes via the
            // pops; RTD additionally moves it by `d16`.
            let pc = cpu.data;
            let d16 = cpu.variant_pending_disp;
            let sp = cpu.regs.active_sp();
            cpu.regs.set_active_sp(sp.wrapping_add(d16));
            cpu.regs.pc = pc;
            cpu.next_fetch_addr = pc;
            cpu.micro_ops.clear();
            cpu.micro_ops.push(MicroOp::FetchIRC);
            cpu.micro_ops.push(MicroOp::PromoteIRC);
            cpu.in_followup = false;
            true
        }
        _ => false,
    }
}

/// RTD #d16 ($4E74). Initial dispatch: consume the displacement
/// extension word, stash sign-extended into `variant_pending_disp`,
/// and queue the PC pop. The rest of the work happens in the
/// continuation hook.
fn execute_rtd(cpu: &mut Cpu68000) -> bool {
    let ext = cpu.consume_irc();
    cpu.variant_pending_disp = i32::from(ext as i16) as u32;
    cpu.in_followup = true;
    cpu.followup_tag = TAG_RTD_PC_HI;
    cpu.micro_ops.push(MicroOp::PopLongHi);
    cpu.micro_ops.push(MicroOp::Execute);
    true
}

/// `MOVE CCR, Dn` — write the low byte of SR into bits 7-0 of Dn,
/// zero bits 15-8, leave bits 31-16 untouched. The 68010
/// User's Manual specifies the operation transfers a *word*
/// (CCR & 0xFF in the low byte, zero in the high byte) — only the
/// low byte of Dn is conceptually affected when storing to a register
/// because Dn.W is the destination size; bits 31-16 are preserved.
fn execute_move_from_ccr_to_dn(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let reg = (opcode & 7) as usize;
    let ccr = cpu.regs.sr & 0x00FF;
    cpu.regs.d[reg] = (cpu.regs.d[reg] & 0xFFFF_0000) | u32::from(ccr);
    true
}

/// `MOVEC Rc, Rn` ($4E7A): copy the named control register into Rn.
///
/// Extension word: `DA(1) | Reg(3) | unused(0) | CR(12)`.
/// Privileged — takes a privilege-violation exception when called
/// in user mode.
fn execute_movec_cr_to_rn(cpu: &mut Cpu68000) -> bool {
    if !cpu.regs.is_supervisor() {
        cpu.begin_group1_exception(8, cpu.instr_start_pc);
        return true;
    }
    let ext = cpu.consume_irc();
    let Some(value) = read_control_register(cpu, ext) else {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    };
    write_extension_register(cpu, ext, value);
    true
}

/// `MOVEC Rn, Rc` ($4E7B): copy Rn into the named control register.
fn execute_movec_rn_to_cr(cpu: &mut Cpu68000) -> bool {
    if !cpu.regs.is_supervisor() {
        cpu.begin_group1_exception(8, cpu.instr_start_pc);
        return true;
    }
    let ext = cpu.consume_irc();
    let value = read_extension_register(cpu, ext);
    if !write_control_register(cpu, ext, value) {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
    }
    true
}

/// Decode the data/address register field of a MOVEC extension word
/// and read the named GP register.
fn read_extension_register(cpu: &Cpu68000, ext: u16) -> u32 {
    let reg = ((ext >> 12) & 7) as usize;
    let is_address = (ext & 0x8000) != 0;
    if is_address {
        cpu.regs.a(reg)
    } else {
        cpu.regs.d[reg]
    }
}

/// Decode the data/address register field of a MOVEC extension word
/// and write the named GP register.
fn write_extension_register(cpu: &mut Cpu68000, ext: u16, value: u32) {
    let reg = ((ext >> 12) & 7) as usize;
    let is_address = (ext & 0x8000) != 0;
    if is_address {
        cpu.regs.set_a(reg, value);
    } else {
        cpu.regs.d[reg] = value;
    }
}

/// Read the control register named in the low 12 bits of a MOVEC
/// extension word. Returns `None` for an unknown / 68020+ register
/// (the caller raises ILLEGAL).
fn read_control_register(cpu: &Cpu68000, ext: u16) -> Option<u32> {
    match ext & 0x0FFF {
        0x000 => Some(u32::from(cpu.regs.sfc)),
        0x001 => Some(u32::from(cpu.regs.dfc)),
        0x800 => Some(cpu.regs.usp),
        0x801 => Some(cpu.regs.vbr),
        _ => None,
    }
}

/// Write a value to the named control register. Returns `false` if
/// the CR number is unknown / 68020+ (the caller raises ILLEGAL).
///
/// SFC / DFC are 3-bit registers — only bits 2-0 of the source are
/// kept. VBR keeps a full 32 bits.
fn write_control_register(cpu: &mut Cpu68000, ext: u16, value: u32) -> bool {
    match ext & 0x0FFF {
        0x000 => {
            cpu.regs.sfc = (value & 0x7) as u8;
            true
        }
        0x001 => {
            cpu.regs.dfc = (value & 0x7) as u8;
            true
        }
        0x800 => {
            cpu.regs.usp = value;
            true
        }
        0x801 => {
            cpu.regs.vbr = value;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu68010;

    #[test]
    fn new_installs_variant_hook() {
        let cpu = Cpu68010::new();
        assert!(cpu.variant_decode_hook.is_some());
    }

    #[test]
    fn move_from_ccr_to_d0_copies_low_byte_of_sr() {
        let mut cpu = Cpu68010::new();
        cpu.regs.sr = 0x271F; // S=1, IPL=7, CCR = $1F (all five flags set)
        cpu.regs.d[0] = 0xDEAD_BEEF;
        let handled = super::decode_68010_opcode(&mut cpu.inner, 0x42C0);
        assert!(handled);
        // bits 31-16 preserved, bits 15-8 zeroed, bits 7-0 = CCR
        assert_eq!(cpu.regs.d[0], 0xDEAD_001F);
    }

    #[test]
    fn movec_writes_vbr_in_supervisor_mode() {
        let mut cpu = Cpu68010::new();
        cpu.regs.sr |= 0x2000; // ensure supervisor
        cpu.regs.d[3] = 0x0040_0000;
        // Encode MOVEC ext word: D/A=0 (data), Reg=3, CR=$801 (VBR)
        let ext = (3u16 << 12) | 0x801;
        cpu.irc = ext;
        let handled = super::decode_68010_opcode(&mut cpu.inner, 0x4E7B);
        assert!(handled);
        assert_eq!(cpu.regs.vbr, 0x0040_0000);
    }

    #[test]
    fn movec_reads_vbr_back_into_an() {
        let mut cpu = Cpu68010::new();
        cpu.regs.sr |= 0x2000;
        cpu.regs.vbr = 0x00F8_0000;
        // Encode MOVEC ext: D/A=1 (address), Reg=2, CR=$801 (VBR)
        let ext = (1u16 << 15) | (2u16 << 12) | 0x801;
        cpu.irc = ext;
        let handled = super::decode_68010_opcode(&mut cpu.inner, 0x4E7A);
        assert!(handled);
        assert_eq!(cpu.regs.a(2), 0x00F8_0000);
    }

    #[test]
    fn movec_in_user_mode_raises_privilege_violation() {
        let mut cpu = Cpu68010::new();
        cpu.regs.sr = 0x0000; // user mode, no flags
        cpu.regs.ssp = 0x0001_0000;
        cpu.regs.vbr = 0x0000;
        cpu.irc = 0x801;
        // VBR write attempt from user mode: hook should still return
        // true (it took the trap) and the VBR should be untouched.
        let handled = super::decode_68010_opcode(&mut cpu.inner, 0x4E7B);
        assert!(handled);
        assert_eq!(cpu.regs.vbr, 0x0000);
    }

    #[test]
    fn movec_with_unknown_68020_register_falls_through_to_illegal() {
        // CR $002 = CACR (68020+). The 68010 hook must NOT write it;
        // returning true and signalling ILLEGAL keeps the trap path.
        let mut cpu = Cpu68010::new();
        cpu.regs.sr |= 0x2000;
        cpu.regs.ssp = 0x0001_0000;
        cpu.regs.d[0] = 0xCAFE_BABE;
        cpu.irc = 0x002;
        let handled = super::decode_68010_opcode(&mut cpu.inner, 0x4E7B);
        assert!(handled);
        // CACR must not have been written — the 68010 doesn't know it.
        assert_eq!(cpu.regs.cacr, 0);
    }
}
