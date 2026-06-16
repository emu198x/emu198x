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
#[derive(Clone, serde::Serialize)]
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
        let mut cpu = Self {
            inner: Cpu68010::new(),
        };
        cpu.install_variant_hooks();
        cpu
    }

    /// Install (or re-install) the 68020-specific hooks and flags on
    /// the wrapped `Cpu68000`. The 68010 base layer is installed
    /// recursively via `Cpu68010::install_variant_hooks` whenever a
    /// fresh `Cpu68010` is built or deserialized; this method layers
    /// the 68020-only deltas on top.
    fn install_variant_hooks(&mut self) {
        self.inner.variant_decode_hook = Some(decode_68020_opcode);
        // 68020+ adds the bit-field memory pipeline tags
        // (TAG_BF_MEM_READ/EXEC/WRITE). The continue hook dispatches
        // those tags and falls through to the 68010 chain for
        // anything it doesn't claim (TAG_RTD_* on 68010).
        self.inner.variant_continue_hook = Some(continue_68020_opcode);
        // Brief-extension-word scale factor (Xn.SIZE*1/2/4/8) is
        // 68020+ behaviour. The 68010's hook leaves the flag false;
        // the 68020 enables it here so calc_ea_start consults bits
        // 10-9 of the extension word.
        self.inner.variant_scaled_index = true;
        // The 68020 widens the SR write mask to include the M-flag
        // (bit 12) — MOVE-to-SR / ORI-to-SR / EORI-to-SR /
        // ANDI-to-SR / STOP / RTE all read this flag. The 68010
        // leaves it false (only the 68000-shared 0xA71F bits are
        // writable).
        self.inner.variant_extended_sr_writes = true;
        // The 68020+ promotes CHK / CHK2 / divide-by-zero / TRAPV /
        // TRAPcc / Trace to a 12-byte Format-$2 exception frame
        // with an extra Instruction-Address long at the top.
        // M68000PRM § 8.6.3.
        self.inner.variant_format2_vectors = true;
        // The 68020+ promotes group-0 (bus/address error) to a
        // 28-byte Format-$A "short bus fault" frame. KS 3.1's
        // vec-2/3 handler reads at Format-$A field offsets (SR at
        // SP+0, PC at SP+2, F/V at SP+6, ...). M68000PRM § 8.6.4.
        self.inner.variant_format_a_group0 = true;
        // ── Timing (TimingClass::M68020) — see the 68k cycle-timing
        // plan (#41/#110/#111). The 68020 uses a 3-clock minimum bus
        // cycle (vs the 68000's 4) and a barrel shifter that completes
        // in constant time regardless of shift count.
        self.inner.variant_min_bus_clocks = 3;
        self.inner.variant_constant_shift_timing = true;
        // The 68020 has a 256-byte on-chip instruction cache (64
        // direct-mapped long-word entries). A program-space prefetch
        // that hits skips the external bus cycle, so cached code does
        // not contend with Agnus for chip RAM. The cache starts
        // disabled (CACR.E = 0, like real hardware); Kickstart enables
        // it via MOVEC. Rebuilt empty here on every construct/deserialize
        // — a cold cache is transparent. See `motorola_68000::icache`.
        self.inner.variant_icache = Some(motorola_68000::ICache::new());
        // Indexed and computed effective-address calculations cost the
        // 68020's clocks (M68020UM § 8.2.3 Calculate EA, Cache Case —
        // the no-overlap column our sequential engine targets) instead
        // of the 68000 model's flat 2-clock approximation: brief
        // (d8,An,Xn) = 4, full-format base+index = 6, predecrement = 2
        // (#41 Phase 4). The 68000/68010 keep the flat 2.
        self.inner.variant_um_ea_calc_timing = true;

        // Bcc/BSR/BRA decode the 32-bit displacement form ($FF in the
        // 8-bit field). On the 68000/68010 that is a normal 8-bit branch
        // with displacement −1, so it must be a core flag (#114).
        self.inner.variant_long_branch = true;
    }

    /// Configure whether a 68881/68882 FPU coprocessor is attached.
    ///
    /// The FPU is a *machine* property, not a CPU-model one: a full
    /// 68020 with no 68881 fitted, and the 68EC020 (A1200/CD32) which
    /// has no coprocessor interface, both take the vector-11 F-line trap.
    /// Default is no FPU; a machine that wires one calls this with
    /// `true`. When unset, all `$Fxxx` opcodes trap as before.
    pub fn set_fpu_present(&mut self, present: bool) {
        self.inner.variant_fpu_present = present;
    }

    /// Select the FPU coprocessor model: `false` = MC68881 (the default),
    /// `true` = MC68882. The two are arithmetically identical; the choice
    /// only affects the FSAVE/FRESTORE internal-state frame size (68881 =
    /// 28 bytes, 68882 = 60 bytes). A machine wires whichever part it fits.
    pub fn set_fpu_68882(&mut self, is_68882: bool) {
        self.inner.variant_fpu_is_68882 = is_68882;
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

impl<'de> serde::Deserialize<'de> for Cpu68020 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Deserialize the inner Cpu68010 first — that recursively
        // restores the 68010-layer hook bindings. Then layer the
        // 68020-specific deltas on top.
        #[derive(serde::Deserialize)]
        struct Bare {
            inner: Cpu68010,
        }
        let bare = Bare::deserialize(d)?;
        let mut cpu = Self { inner: bare.inner };
        cpu.install_variant_hooks();
        Ok(cpu)
    }
}

// ─── Decode hook ──────────────────────────────────────────────────

use motorola_68k_common::flags::{C, N, V, Z};
use motorola_68010::decode_68010_opcode;

/// Hook installed on [`Cpu68000::variant_decode_hook`] by every
/// [`Cpu68020`] instance. Chains to the 68010 hook for opcodes the
/// 68020 doesn't override.
pub fn decode_68020_opcode(cpu: &mut Cpu68000, opcode: u16) -> bool {
    // 68020-specific opcodes first.
    if (opcode & 0xFFF8) == 0x49C0 {
        return execute_extb_l(cpu, opcode);
    }

    // MULU.L / MULS.L ($4C00-$4C3F): 32×32 multiply, register or
    // memory source. The wrapper only handles Dn sources today —
    // memory EAs need the multi-step continuation pipeline.
    if (opcode & 0xFFC0) == 0x4C00 {
        return execute_mull(cpu, opcode);
    }

    // DIVU.L / DIVS.L ($4C40-$4C7F): same shape as MULL.
    if (opcode & 0xFFC0) == 0x4C40 {
        return execute_divl(cpu, opcode);
    }

    // Bit-field family: 1110 1xxx 11 MMMRRR. Sub-op in bits 10-8:
    //   000=BFTST 001=BFEXTU 010=BFCHG 011=BFEXTS
    //   100=BFCLR 101=BFFFO  110=BFSET 111=BFINS
    if (opcode & 0xF8C0) == 0xE8C0 {
        return execute_bf(cpu, opcode);
    }

    // PACK ($8140) / UNPK ($8180): the core routes these here. Bit 3
    // selects register (0) or memory predecrement (1) operands.
    if (opcode & 0xF1F0) == 0x8140 {
        return execute_pack(cpu, opcode);
    }
    if (opcode & 0xF1F0) == 0x8180 {
        return execute_unpk(cpu, opcode);
    }

    // TRAPcc ($50F8 / $50FA / $50FC + cc): the core routes the Scc
    // sub-encoding mode 111 / reg ≥ 2 here. Reg 2/3/4 select the
    // word-operand / long-operand / no-operand forms.
    if (opcode & 0xF0F8) == 0x50F8 {
        return execute_trapcc(cpu, opcode);
    }

    // CHK2 / CMP2 (`0000 0ss0 11 mmmrrr`): the core routes the size-3
    // immediate group's `11` sub-encoding here. Bit 11 of the *extension*
    // word selects CHK2 (1) vs CMP2 (0); the opcode's bit 11 is 0 — which
    // separates this from CAS (`0000 1ss0 11 …`, bit 11 = 1) that shares
    // the same routing. ss in bits 10-9 is the operand size.
    if (opcode & 0xF9C0) == 0x00C0 {
        return execute_chk2_cmp2(cpu, opcode);
    }

    // CAS (`0000 1ss0 11 mmmrrr` + ext word): atomic compare-and-swap.
    // ss in bits 10-9 (01=byte, 10=word, 11=long); opcode bit 11 = 1
    // separates it from CHK2/CMP2 (bit 11 = 0). CAS.B/W arrive via the
    // size-3 immediate-group routing, CAS.L via the core's $0EC0 arm.
    // The CAS2 encoding (EA = 111 100, immediate) shares this mask and
    // is handled separately (deferred).
    if (opcode & 0xF9C0) == 0x08C0 {
        return execute_cas(cpu, opcode);
    }

    // CALLM / RTM ($06C0-$06FF): the 68020 module-call mechanism
    // (descriptor-based call/return with external access-control
    // hardware). Deliberately unimplemented — take the illegal-
    // instruction exception, matching WinUAE, Musashi, and the 68030+
    // (which dropped these opcodes entirely). No oracle exists to
    // validate a faithful implementation against, no Amiga software uses
    // them, and Type-1 module calls need access-control hardware no Amiga
    // has. See knowledge/decisions/callm-rtm-illegal.md. (These already
    // fall through to the illegal path; this arm makes the choice
    // explicit and is pinned by tests/callm_rtm.rs.)
    if (opcode & 0xFFC0) == 0x06C0 {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    }

    // F-line ($Fxxx): coprocessor space. The core routes the whole
    // $Fxxx range here (ahead of its vector-11 fallback). Only cpID-1
    // (68881/2 FPU) on an FPU-equipped machine is claimed; everything
    // else declines (returns false) so the core takes the vector-11
    // F-line emulator trap — correct for the 68EC020 (A1200/CD32) and a
    // full 020 with no FPU fitted.
    if (opcode & 0xF000) == 0xF000 {
        return decode_fpu_fline(cpu, opcode);
    }

    // MOVEC ($4E7A / $4E7B): intercepted at the 68020 layer so the
    // 68020-additional control registers (CACR / CAAR / MSP / ISP)
    // resolve before falling through to the 68010 hook for the
    // 68010-basic four (SFC / DFC / USP / VBR).
    if opcode == 0x4E7A {
        return execute_movec_68020_cr_to_rn(cpu);
    }
    if opcode == 0x4E7B {
        return execute_movec_68020_rn_to_cr(cpu);
    }

    // Fall through to the 68010 hook for MOVE-from-CCR / RTD /
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

/// TRAPcc (`$50F8` / `$50FA` / `$50FC` + cc): conditionally take the
/// TRAPcc exception (vector 7) based on the condition field in bits
/// 11-8. The optional operand is *not* used as data — it follows the
/// opcode purely so the trap handler can find it via the stacked PC —
/// so the instruction only steps the prefetch past it. 68020+ only;
/// M68000PRM § 6.2.40. The reg field selects the operand size:
///
///   reg 2 (`$50FA`): one word operand
///   reg 3 (`$50FB`→`$50FC`-1): one long operand  ← encoded as reg 3
///   reg 4 (`$50FC`): no operand
///
/// Reg 5/6/7 are not defined for TRAPcc and take ILLEGAL.
fn execute_trapcc(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let operand_words: u32 = match opcode & 7 {
        2 => 1, // TRAPcc.W — one extension word
        3 => 2, // TRAPcc.L — two extension words
        4 => 0, // TRAPcc   — no operand
        _ => {
            cpu.begin_group1_exception(4, cpu.instr_start_pc);
            return true;
        }
    };

    // Step the prefetch past the operand words. Each `consume_irc`
    // queues one FetchIRC; the values are discarded (the operand is not
    // data). On the not-taken path this leaves IRC pointing at the next
    // instruction; on the taken path `begin_group1_exception` clears
    // the queue, so the skip is harmless.
    for _ in 0..operand_words {
        cpu.consume_irc();
    }

    let cond = ((opcode >> 8) & 0x0F) as u8;
    if cpu.check_condition(cond) {
        // Stacked PC points past the whole instruction (opcode +
        // operand). The Format-$2 frame also captures instr_start_pc as
        // the Instruction Address (handled in begin_group1_exception).
        let next_pc = cpu.instr_start_pc.wrapping_add(2 + operand_words * 2);
        cpu.begin_group1_exception(7, next_pc);
    }
    true
}

/// PACK (`$8140` + adjustment word): take the 16-bit source, add the
/// immediate adjustment, then pack the two BCD digits at bits [11:8]
/// and [3:0] into one byte. M68000PRM § 6.2.27. No flags affected.
///
/// Register form (`Dy,Dx`) only for now; the rare `-(Ay),-(Ax)` memory
/// form (bit 3 set) needs the predecrement byte pipeline and is a noted
/// follow-up — it takes ILLEGAL until then.
fn execute_pack(cpu: &mut Cpu68000, opcode: u16) -> bool {
    if opcode & 0x08 != 0 {
        // Memory predecrement form — deferred. TODO(#114): implement
        // the -(Ay),-(Ax) byte pipeline (two reads → pack → one write).
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    }
    let adj = cpu.consume_irc();
    let src_reg = (opcode & 7) as usize;
    let dst_reg = ((opcode >> 9) & 7) as usize;
    let src = (cpu.regs.d[src_reg] as u16).wrapping_add(adj);
    let packed = (((src & 0x0F00) >> 4) | (src & 0x000F)) as u8;
    cpu.regs.d[dst_reg] = (cpu.regs.d[dst_reg] & 0xFFFF_FF00) | u32::from(packed);
    true
}

/// UNPK (`$8180` + adjustment word): take the source byte, spread its
/// two nibbles to bits [11:8] and [3:0], then add the immediate
/// adjustment. M68000PRM § 6.2.27. No flags affected. Register form
/// (`Dy,Dx`) only; the memory form is a noted follow-up (see PACK).
fn execute_unpk(cpu: &mut Cpu68000, opcode: u16) -> bool {
    if opcode & 0x08 != 0 {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    }
    let adj = cpu.consume_irc();
    let src_reg = (opcode & 7) as usize;
    let dst_reg = ((opcode >> 9) & 7) as usize;
    let src = (cpu.regs.d[src_reg] & 0xFF) as u16;
    let unpacked = ((src & 0x00F0) << 4) | (src & 0x000F);
    let result = unpacked.wrapping_add(adj);
    cpu.regs.d[dst_reg] = (cpu.regs.d[dst_reg] & 0xFFFF_0000) | u32::from(result);
    true
}

// ─── MULL / DIVL helpers ───────────────────────────────────────────
//
// Extension word format (M68000PRM § 6.2.5 / 6.2.7):
//
//   bit  15: 0 (reserved)
//   bits 14-12: Dl  — low / single-result register
//   bit  11: signed flag (0 = unsigned, 1 = signed)
//   bit  10: size flag (0 = 32-bit form, 1 = 64-bit form)
//   bits 9-3: 0 (reserved)
//   bits 2-0: Dh  — high register (64-bit form) or remainder
//                   register when Dh ≠ Dl on 32-bit DIVx.L

/// Begin a memory-source MUL.L / DIV.L. The spec word (`ext`) is
/// already consumed; stash it and fetch the 32-bit source operand
/// through the shared `TAG_FETCH_SRC_*` EA pipeline. The variant
/// continue hook reclaims `TAG_FETCH_SRC_DATA` (queues the long read)
/// and finishes at `TAG_V_MULDIV_MEM_EXEC`. Immediate source
/// (`#<data>`) is not handled here — it has no EA address for the
/// long read — and takes ILLEGAL pending a follow-up.
fn begin_muldiv_mem_source(cpu: &mut Cpu68000, opcode: u16, ext: u16) -> bool {
    let ea_mode = ((opcode >> 3) & 7) as u8;
    let ea_reg = (opcode & 7) as u8;
    // Immediate (mode 7 / reg 4) — deferred.
    if ea_mode == 7 && ea_reg == 4 {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    }
    cpu.variant_ext_word = ext;
    cpu.src_mode = motorola_68000::addressing::AddrMode::decode(ea_mode, ea_reg);
    cpu.size = motorola_68000::alu::Size::Long;
    cpu.in_followup = true;
    cpu.followup_tag = motorola_68000::cpu::TAG_FETCH_SRC_EA;
    cpu.continue_instruction();
    true
}

/// CHK2 / CMP2 (`0000 0ss0 11 mmmrrr` + extension word). Compares a
/// register against a pair of bounds held in memory at the effective
/// address: the lower bound at `[EA]` and the upper at `[EA + size]`.
/// The extension word holds bit 15 = D/A (register file), bits 14-12 =
/// register number, bit 11 = CHK2 (1) / CMP2 (0).
///
/// CMP2 only sets the flags; CHK2 additionally traps vector 6 when the
/// register is out of bounds. Both reads happen through the shared
/// memory-operand pipeline: this sets up the EA fetch, the core reaches
/// `TAG_FETCH_SRC_DATA`, and the continue hook chains the two bound
/// reads (`TAG_V_CHK2_LOWER` → `TAG_V_CHK2_UPPER`) before computing.
///
/// Only control addressing modes are valid (M68000PRM § 6.2.2); other
/// modes take ILLEGAL.
fn execute_chk2_cmp2(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let Some(size) = motorola_68000::alu::Size::from_bits((opcode >> 9) as u8) else {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    };
    let ext = cpu.consume_irc();
    let ea_mode = ((opcode >> 3) & 7) as u8;
    let ea_reg = (opcode & 7) as u8;

    // Control modes only: (An), (d16,An), (d8,An,Xn), abs, (d16,PC),
    // (d8,PC,Xn). Reject Dn / An / (An)+ / -(An) / immediate.
    let control = match ea_mode {
        2 | 5 | 6 => true,
        7 => matches!(ea_reg, 0..=3),
        _ => false,
    };
    if !control {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    }

    cpu.variant_ext_word = ext;
    cpu.src_mode = motorola_68000::addressing::AddrMode::decode(ea_mode, ea_reg);
    cpu.size = size;
    cpu.in_followup = true;
    cpu.followup_tag = motorola_68000::cpu::TAG_FETCH_SRC_EA;
    cpu.continue_instruction();
    true
}

/// Finish CHK2 / CMP2 once both bounds have been read. `src_val` holds
/// the raw lower bound, `self.data` the raw upper bound, and
/// `variant_ext_word` the spec word. Sets Z and C (leaving N/V/X), and
/// on CHK2 traps vector 6 when the register is out of bounds.
///
/// Matches Musashi (`m68k_in.c`): the compare value is the register
/// masked to the operand size, sign-extended only for data registers —
/// address registers keep the masked (zero-extended) value, Musashi's
/// quirk. The bounds are always read as signed. Z is set when the
/// register equals either bound; C marks out-of-range, handling the
/// `lower > upper` (wrapped range) case.
fn compute_chk2_cmp2(cpu: &mut Cpu68000) {
    let ext = cpu.variant_ext_word;
    let size = cpu.size;
    let reg_idx = ((ext >> 12) & 0x0F) as usize;
    let is_chk2 = (ext & 0x0800) != 0;
    let is_addr = (ext & 0x8000) != 0;

    let raw_reg = if is_addr {
        cpu.regs.a(reg_idx & 7)
    } else {
        cpu.regs.d[reg_idx & 7]
    };

    // Compare value: mask to size, then sign-extend for data registers
    // only (Musashi leaves An zero-extended).
    let masked = raw_reg & size_mask(size);
    let compare: i32 = if is_addr {
        masked as i32
    } else {
        sign_extend(masked, size)
    };

    let lower = sign_extend(cpu.src_val & size_mask(size), size);
    let upper = sign_extend(cpu.data & size_mask(size), size);

    let z = compare == lower || compare == upper;
    // Musashi guards this with `lower <= upper ? … : …`, but both ternary
    // branches reduce to the same expression by commutativity of `||`
    // (`m68k_in.c` chk2cmp2) — so out-of-range is simply "below the lower
    // bound or above the upper bound", with no special wrapped-range case.
    let c = compare < lower || compare > upper;

    let mut sr = cpu.regs.sr & !(Z | C);
    if z {
        sr |= Z;
    }
    if c {
        sr |= C;
    }
    cpu.regs.sr = sr;

    // CHK2 traps vector 6 (CHK/CHK2) when out of bounds. next_pc is the
    // address past the whole instruction: opcode + spec word + EA
    // extension words.
    if is_chk2 && c {
        let ea_ext = cpu.src_mode.map_or(0, |m| u32::from(m.ext_word_count()));
        let next_pc = cpu.instr_start_pc.wrapping_add(4 + ea_ext * 2);
        cpu.begin_group1_exception(6, next_pc);
    }
}

/// Size mask for a value: 0xFF / 0xFFFF / 0xFFFFFFFF.
fn size_mask(size: motorola_68000::alu::Size) -> u32 {
    match size {
        motorola_68000::alu::Size::Byte => 0xFF,
        motorola_68000::alu::Size::Word => 0xFFFF,
        motorola_68000::alu::Size::Long => 0xFFFF_FFFF,
    }
}

/// Sign-extend a size-masked value to a signed 32-bit integer.
fn sign_extend(masked: u32, size: motorola_68000::alu::Size) -> i32 {
    match size {
        motorola_68000::alu::Size::Byte => masked as u8 as i8 as i32,
        motorola_68000::alu::Size::Word => masked as u16 as i16 as i32,
        motorola_68000::alu::Size::Long => masked as i32,
    }
}

/// CAS (`0000 1ss0 11 mmmrrr` + extension word): atomic compare-and-swap.
/// Reads the destination at the effective address, compares it with the
/// compare register Dc (extension bits 2-0), and:
///
/// - if equal: writes the update register Du (extension bits 8-6) back
///   to the effective address;
/// - if not equal: loads the read value into Dc.
///
/// Flags are the result of the comparison `dest - Dc` (subtract flags,
/// X preserved — the same as CMP). M68000PRM § 6.2.3.
///
/// Only memory alterable modes are valid; the immediate EA (`111 100`)
/// is the CAS2 marker and is deferred. Other invalid modes take ILLEGAL.
fn execute_cas(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let ea_mode = ((opcode >> 3) & 7) as u8;
    let ea_reg = (opcode & 7) as u8;

    // The immediate EA (`111 100`) is the CAS2 marker, not a real EA —
    // route the dual-address form before consuming the extension word
    // (CAS2 gathers a 32-bit spec from both following words).
    if ea_mode == 7 && ea_reg == 4 {
        return execute_cas2(cpu, opcode);
    }

    let size = match (opcode >> 9) & 3 {
        1 => motorola_68000::alu::Size::Byte,
        2 => motorola_68000::alu::Size::Word,
        3 => motorola_68000::alu::Size::Long,
        _ => {
            cpu.begin_group1_exception(4, cpu.instr_start_pc);
            return true;
        }
    };
    let ext = cpu.consume_irc();

    // Memory alterable modes only: (An), (An)+, -(An), (d16,An),
    // (d8,An,Xn), abs.
    let alterable = match ea_mode {
        2..=6 => true,
        7 => matches!(ea_reg, 0 | 1),
        _ => false,
    };
    if !alterable {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    }

    cpu.variant_ext_word = ext;
    cpu.src_mode = motorola_68000::addressing::AddrMode::decode(ea_mode, ea_reg);
    cpu.size = size;
    cpu.in_followup = true;
    cpu.followup_tag = motorola_68000::cpu::TAG_FETCH_SRC_EA;
    cpu.continue_instruction();
    true
}

/// Index the data/address register file by a 4-bit field (0-7 = D0-D7,
/// 8-15 = A0-A7), matching Musashi's `REG_DA`.
fn reg_da(cpu: &Cpu68000, idx: u32) -> u32 {
    let idx = (idx & 15) as usize;
    if idx < 8 {
        cpu.regs.d[idx]
    } else {
        cpu.regs.a(idx - 8)
    }
}

/// CAS2 (`$0CFC` / `$0EFC` + two extension words): dual-address atomic
/// compare-and-swap. Two register-held pointers Rn1/Rn2 address the
/// destinations; each is compared with its compare register Dc1/Dc2.
/// If *both* match, Du1/Du2 are written back; otherwise both read values
/// are loaded into Dc1/Dc2. M68000PRM § 6.2.4.
///
/// The 32-bit spec word (both extension words) packs, per 16-bit half:
/// bit 15 = D/A of Rn, bits 14-12 = Rn number, bits 8-6 = Du, bits 2-0 =
/// Dc. The high half describes operand 1, the low half operand 2.
///
/// The flags reflect the operand-1 comparison, or operand-2's if
/// operand 1 matched (Musashi `m68k_in.c` cas2). Word/long only.
fn execute_cas2(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let size = match (opcode >> 9) & 3 {
        2 => motorola_68000::alu::Size::Word,
        3 => motorola_68000::alu::Size::Long,
        _ => {
            cpu.begin_group1_exception(4, cpu.instr_start_pc);
            return true;
        }
    };
    cpu.size = size;
    // Extension word 1 is prefetched in `irc`; stash it in the high half
    // of `src_val` and fetch extension word 2.
    cpu.src_val = u32::from(cpu.irc) << 16;
    cpu.in_followup = true;
    cpu.micro_ops.push(MicroOp::FetchIRC);
    cpu.followup_tag = TAG_V_CAS2_GATHER;
    cpu.micro_ops.push(MicroOp::Execute);
    true
}

/// Compute the CAS2 result once both destinations are read. Sets the
/// comparison flags and returns `true` when both compares matched (the
/// caller must write Du1/Du2 back); on a mismatch it loads both read
/// values into Dc1/Dc2 here and returns `false`.
fn finish_cas2(cpu: &mut Cpu68000, word2: u32, dest1: u32, dest2: u32) -> bool {
    let size = cpu.size;
    let dc1 = ((word2 >> 16) & 7) as usize;
    let dc2 = (word2 & 7) as usize;
    let cmp = motorola_68000::cpu::AluOp::Cmp;

    // Flags from dest1 - Dc1; if equal, recompute from dest2 - Dc2 (so
    // the final flags reflect operand 2 when operand 1 matched).
    cpu.exec_alu(cmp, cpu.regs.d[dc1], dest1, size);
    let both_eq = if cpu.regs.sr & Z != 0 {
        cpu.exec_alu(cmp, cpu.regs.d[dc2], dest2, size);
        cpu.regs.sr & Z != 0
    } else {
        false
    };

    if both_eq {
        return true;
    }

    // Mismatch: load both read values into the compare registers. The
    // sign-extend-vs-preserve choice uses the operand's Rn D/A bit
    // (bit 31 for operand 1, bit 15 for operand 2) — Musashi's quirk.
    load_cas2_compare(cpu, dc1, dest1, size, word2 & 0x8000_0000 != 0);
    load_cas2_compare(cpu, dc2, dest2, size, word2 & 0x0000_8000 != 0);
    false
}

/// Load a CAS2 read value into a compare data register on a mismatch.
/// Long replaces the whole register; word either sign-extends (when the
/// operand's Rn D/A bit is set) or replaces just the low word.
fn load_cas2_compare(
    cpu: &mut Cpu68000,
    dc: usize,
    dest: u32,
    size: motorola_68000::alu::Size,
    da: bool,
) {
    match size {
        motorola_68000::alu::Size::Long => cpu.regs.d[dc] = dest,
        _ => {
            cpu.regs.d[dc] = if da {
                dest as u16 as i16 as i32 as u32
            } else {
                (cpu.regs.d[dc] & 0xFFFF_0000) | (dest & 0xFFFF)
            };
        }
    }
}

/// Decode an F-line ($Fxxx) coprocessor opcode. Only cpID-1 (the
/// 68881/68882 FPU) on an FPU-equipped machine is claimed; any other
/// coprocessor ID, or an FPU-less machine, returns `false` so the core
/// takes the vector-11 F-line emulator trap.
///
/// The opcode layout is `1111 ccc ttt mmmrrr`: ccc = coprocessor ID
/// (bits 11-9), ttt = operation class (bits 8-6), mmmrrr = EA / further
/// encoding. FPU classes: 0 = cpGEN (FADD/FMOVE/…), 1 = cpScc, 2 =
/// cpBcc.W, 3 = cpBcc.L, 4 = cpSAVE, 5 = cpRESTORE.
fn decode_fpu_fline(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let cp_id = (opcode >> 9) & 7;
    if cp_id != 1 || !cpu.variant_fpu_present {
        return false;
    }
    let op_class = (opcode >> 6) & 7;
    let claimed = match op_class {
        // cpGEN: the arithmetic/move class (FMOVE/FABS/FNEG/FTST/… + the
        // arithmetic ops). The reg-to-reg non-arithmetic subset is wired;
        // arithmetic and memory operands decline for now.
        0 => execute_fpgen(cpu),
        // cpBcc.W: branch on FP condition, 16-bit displacement. Covers
        // FNOP (FBF.W with a zero displacement).
        2 => execute_fbcc_w(cpu, opcode),
        // cpBcc.L: branch on FP condition, 32-bit displacement.
        3 => execute_fbcc_l(cpu, opcode),
        // cpScc (1): set a byte on the FP condition (FScc; FDBcc/FTRAPcc
        // share this class but are handled / declined inside).
        1 => execute_fscc(cpu, opcode),
        // cpSAVE (4): FSAVE — store the FPU's internal state to memory.
        4 => begin_fsave(cpu, opcode),
        // cpRESTORE (5) is not wired yet — decline so it takes the
        // vector-11 trap until FRESTORE lands.
        _ => false,
    };

    // Any executed 68881/2 FP instruction (op-classes 0-3) takes the FPU
    // out of the null (reset) state into the idle state — WinUAE
    // `maybe_idle_state`. FSAVE (4) reports the state without changing it;
    // FRESTORE (5) sets the state itself from the restored frame.
    if claimed && matches!(op_class, 0..=3) {
        cpu.variant_fpu_state = 1;
    }
    claimed
}

/// cpGEN (cpID-1 op-class 0): the FPU general instruction. The extension
/// word selects the operation. Bit 14 (R/M) chooses a register source
/// (0) or an external source via the EA / FMOVECR (1). Bits 12-10 are the
/// source register (R/M = 0) or the source format (R/M = 1); bits 9-7 are
/// the destination Fpn; bits 6-0 are the opmode (the operation).
///
/// The register-to-register non-arithmetic ops (FMOVE/FABS/FNEG/FTST,
/// pure bit ops) and the first arithmetic pair (FADD/FSUB, via the
/// SoftFloat `floatx80` port) are wired. The remaining arithmetic
/// (FMUL/FDIV/FCMP/FSQRT/FINT/…) and external/FMOVECR operands decline
/// (vector-11 trap) until their ports / the EA chain land. Decoded per
/// Musashi `fpgen_rm_reg`.
fn execute_fpgen(cpu: &mut Cpu68000) -> bool {
    use motorola_68k_common::softfloat::{self, RoundingMode};

    // Peek the extension word without advancing the prefetch, so a
    // declined op leaves no side effect for the core's vector-11 path.
    let w2 = cpu.irc;
    let mode = RoundingMode::from_fpcr_bits(cpu.regs.fpcr_rounding_mode());

    // cpGEN sub-op (Musashi `m68040_fpu_op0`), bits 15-13 of the extension
    // word: 0 = ALU FP,FP; 2 = ALU ea,FP (both `fpgen_rm_reg`); 3 = FMOVE
    // FPn → ea (store); 4/5 = FMOVE FPCR/FPSR/FPIAR ↔ ea; 6/7 = FMOVEM
    // register list ↔ ea (not wired yet).
    match (w2 >> 13) & 7 {
        0 | 2 => {}                                     // fall through to fpgen_rm_reg
        3 => return begin_fp_store(cpu, mode),          // FMOVE FPn → memory
        4 | 5 => return execute_fmove_control(cpu, w2), // FMOVE FPcr ↔ ea
        6 | 7 => return begin_fmovem(cpu, w2),          // FMOVEM list ↔ ea
        _ => return false,                              // sub-op 1: unused → vector 11
    }

    let rm = (w2 >> 14) & 1;

    // FMOVECR (R/M = 1, source specifier 7): load a constant from the
    // 68881/2 on-chip ROM into the destination Fpn. The low 7 bits select
    // the ROM entry (they are NOT an opmode here), so this is handled
    // before the opmode decode and needs no EA fetch.
    if rm == 1 && (w2 >> 10) & 7 == 7 {
        let _ = cpu.consume_irc();
        let dst = ((w2 >> 7) & 7) as usize;
        let v = softfloat::fmovecr(mode, (w2 & 0x7F) as u8);
        cpu.regs.fp[dst] = v;
        motorola_68k_common::fpu::set_condition_codes(&mut cpu.regs, v);
        return true;
    }

    let mut opmode = w2 & 0x7F;

    // Rounding precision: the FSxxx/FDxxx opmode prefix (bits 6 and 2 encode
    // single/double) overrides the FPCR rounding-precision field (bits 7-6).
    // Strip the prefix bits to recover the base opmode.
    let prefix_precision = if opmode & 0x44 == 0x44 {
        opmode &= !0x44;
        Some(32)
    } else if opmode & 0x40 != 0 {
        opmode &= !0x40;
        Some(64)
    } else {
        None
    };
    let precision = prefix_precision.unwrap_or_else(|| match cpu.regs.fpcr_rounding_precision() {
        1 => 32,
        2 => 64,
        _ => 80,
    });

    // Only these opmodes are backed by the SoftFloat `floatx80` port — FMOVE/
    // FABS/FNEG/FTST/FADD/FSUB/FMUL/FDIV/FSQRT/FCMP/FINT/FINTRZ/FGETEXP/FGETMAN/
    // FSCALE/FMOD/FREM/FSGLMUL/FSGLDIV. The transcendentals decline (vector-11
    // trap) until their backends land.
    if !matches!(
        opmode,
        0x00 | 0x18
            | 0x1A
            | 0x3A
            | 0x22
            | 0x28
            | 0x23
            | 0x20
            | 0x04
            | 0x1E
            | 0x1F
            | 0x21
            | 0x25
            | 0x26
            | 0x38
            | 0x01
            | 0x03
            | 0x24
            | 0x27
    ) {
        return false;
    }

    if rm == 0 {
        // Register-to-register: the source is an Fpn. Consume the extension
        // word and run synchronously.
        let _ = cpu.consume_irc();
        let src = ((w2 >> 10) & 7) as usize;
        let dst = ((w2 >> 7) & 7) as usize;
        let source = cpu.regs.fp[src];
        softfloat::clear_exception_flags();
        apply_fp_opmode(cpu, opmode, source, dst, mode, precision);
        return true;
    }

    // R/M = 1: the source is a memory operand fetched via the EA. Carry the
    // rounding precision so `handle_fp_mem_exec` applies it once loaded.
    cpu.fp_mem_precision = precision;
    begin_fp_memory(cpu, opmode)
}

/// Apply an FPU opmode with `source` as the (already-loaded) source
/// operand and `dst` as the destination Fpn, then set the condition
/// codes. Shared by the register-to-register and memory-operand paths.
/// Decoded per Musashi `fpgen_rm_reg`.
fn apply_fp_opmode(
    cpu: &mut Cpu68000,
    opmode: u16,
    source: motorola_68k_common::registers::FpReg,
    dst: usize,
    mode: motorola_68k_common::softfloat::RoundingMode,
    precision: i32,
) {
    use motorola_68k_common::registers::FpReg;
    use motorola_68k_common::softfloat;

    match opmode {
        // FMOVE/FABS/FNEG round the source to the rounding precision (identity
        // at extended precision; single/double round per FPCR or the prefix).
        0x00 => cpu.regs.fp[dst] = softfloat::floatx80_move(precision, mode, source),
        0x18 => cpu.regs.fp[dst] = softfloat::floatx80_abs(precision, mode, source),
        0x1A => cpu.regs.fp[dst] = softfloat::floatx80_neg(precision, mode, source),
        0x3A => {} // FTST — flags only
        // Binary ops: dst = dst op source (Musashi passes REG_FP[dst] first),
        // at the selected rounding precision with the FPCR rounding mode.
        0x22 => {
            cpu.regs.fp[dst] = softfloat::floatx80_add(precision, mode, cpu.regs.fp[dst], source);
        }
        0x28 => {
            cpu.regs.fp[dst] = softfloat::floatx80_sub(precision, mode, cpu.regs.fp[dst], source);
        }
        0x23 => {
            cpu.regs.fp[dst] = softfloat::floatx80_mul(precision, mode, cpu.regs.fp[dst], source);
        }
        0x20 => {
            cpu.regs.fp[dst] = softfloat::floatx80_div(precision, mode, cpu.regs.fp[dst], source);
        }
        // FSGLMUL/FSGLDIV: single-precision multiply/divide. FSGLMUL also
        // truncates its operands to single precision first (dedicated paths,
        // not FMUL/FDIV at single rounding); they ignore the FPCR precision.
        0x27 => cpu.regs.fp[dst] = softfloat::floatx80_sglmul(mode, cpu.regs.fp[dst], source),
        0x24 => cpu.regs.fp[dst] = softfloat::floatx80_sgldiv(mode, cpu.regs.fp[dst], source),
        // FMOD/FREM: dst = dst mod/rem source, and set the FPSR quotient byte.
        0x21 => {
            let r = softfloat::floatx80_mod(precision, mode, cpu.regs.fp[dst], source);
            cpu.regs.fp[dst] = r.value;
            cpu.regs.set_fpsr_quotient(r.quotient, r.sign);
        }
        0x25 => {
            let r = softfloat::floatx80_rem(precision, mode, cpu.regs.fp[dst], source);
            cpu.regs.fp[dst] = r.value;
            cpu.regs.set_fpsr_quotient(r.quotient, r.sign);
        }
        // Unary ops on the source, written to dst.
        0x04 => cpu.regs.fp[dst] = softfloat::floatx80_sqrt(precision, mode, source), // FSQRT
        0x1E => cpu.regs.fp[dst] = softfloat::floatx80_getexp(source),                // FGETEXP
        0x1F => cpu.regs.fp[dst] = softfloat::floatx80_getman(source),                // FGETMAN
        // FSCALE: scale dst by 2^(integer part of source), rounded to precision.
        0x26 => {
            cpu.regs.fp[dst] = softfloat::floatx80_scale(precision, mode, cpu.regs.fp[dst], source);
        }
        0x01 => {
            // FINT: round source to an integer (per the FPCR mode) and back.
            let n = softfloat::floatx80_to_int32(mode, source);
            cpu.regs.fp[dst] = softfloat::int32_to_floatx80(n);
        }
        0x03 => {
            // FINTRZ: round-to-zero variant, independent of the FPCR mode.
            let n = softfloat::floatx80_to_int32_round_to_zero(source);
            cpu.regs.fp[dst] = softfloat::int32_to_floatx80(n);
        }
        0x38 => {
            // FCMP: set condition codes from dst − source without writing a
            // register. Musashi special-cases infinities (when neither
            // operand is a NaN) to avoid an inf − inf invalid result.
            let dst_v = cpu.regs.fp[dst];
            let inf_sign = |v: FpReg| -> i32 {
                if v.is_infinite() {
                    if v.is_negative() { -1 } else { 1 }
                } else {
                    0
                }
            };
            let d = inf_sign(dst_v);
            let s = inf_sign(source);
            if !dst_v.is_nan() && !source.is_nan() && (d != 0 || s != 0) {
                let (mut n, mut z) = (false, false);
                if s < 0 {
                    if d < 0 {
                        n = true;
                        z = true;
                    }
                } else if s > 0 {
                    if d > 0 {
                        z = true;
                    } else {
                        n = true;
                    }
                } else if d < 0 {
                    n = true;
                }
                cpu.regs.set_fpsr_cc(n, z, false, false);
            } else {
                let res = softfloat::floatx80_sub(80, mode, dst_v, source);
                motorola_68k_common::fpu::set_condition_codes(&mut cpu.regs, res);
            }
            motorola_68k_common::fpu::apply_exceptions(&mut cpu.regs);
            return;
        }
        _ => return, // opmode filtered by the caller
    }

    // The writing ops report their condition codes from the destination;
    // FTST reports from the (unwritten) source. FCMP returned above.
    let cc_value = if opmode == 0x3A {
        source
    } else {
        cpu.regs.fp[dst]
    };
    motorola_68k_common::fpu::set_condition_codes(&mut cpu.regs, cc_value);
    // Fold the IEEE exceptions this operation raised (including any operand
    // format conversion done before this call) into the FPSR. The caller
    // cleared the accumulator before the operation.
    motorola_68k_common::fpu::apply_exceptions(&mut cpu.regs);
}

/// Begin an FPU memory-source operand fetch (cpGEN R/M = 1). The FP
/// extension word selects the operand format (and hence its byte size);
/// the opcode's EA field selects the address. Only the instant addressing
/// modes — `(An)`, `(An)+`, `-(An)` — compute the base address
/// synchronously (auto-increment/decrement by the operand size). The
/// non-auto-increment modes — `d16(An)`, `(d8,An,Xn)` and the full 68020
/// extension formats, AbsShort/Long, and PC-relative — are routed through
/// the core's `calc_ea_start` and resumed at `TAG_FETCH_SRC_DATA`. All six
/// source formats — including the 12-byte packed-decimal real (format 3) —
/// and the immediate (`#data`) mode are handled.
///
/// All decline checks happen before any state mutation, so a declined op
/// leaves the prefetch and registers untouched for the core's trap path.
fn begin_fp_memory(cpu: &mut Cpu68000, opmode: u16) -> bool {
    let w2 = cpu.irc;
    let format = ((w2 >> 10) & 7) as u8;
    let bytes_total: u8 = match format {
        6 => 1,            // Byte integer
        4 => 2,            // Word integer
        0 | 1 => 4,        // Long integer / Single
        5 => 8,            // Double
        2 | 3 => 12,       // Extended / Packed-decimal
        _ => return false, // (formats 0-6 cover every cpGEN source)
    };
    let ea_mode = ((cpu.ir >> 3) & 7) as u8;
    let ea_reg = (cpu.ir & 7) as u8;
    // Modes we can fetch: (An)/(An)+/-(An) directly; the static address
    // modes (d16(An), indexed, abs, PC-relative) via the core EA
    // machinery; and immediate (#data, 7/4) from the instruction stream.
    // Dn/An-direct aren't valid memory operands and decline.
    let static_mode = match ea_mode {
        5 | 6 => true,
        7 => matches!(ea_reg, 0..=3),
        _ => false,
    };
    let immediate = ea_mode == 7 && ea_reg == 4;
    if !matches!(ea_mode, 2..=4) && !static_mode && !immediate {
        return false;
    }

    // Committed: consume the FP extension word and stash the operand
    // parameters before either path runs.
    let _ = cpu.consume_irc();
    cpu.fp_mem_buf = 0;
    cpu.fp_mem_bytes_total = bytes_total;
    cpu.fp_mem_bytes_done = 0;
    cpu.fp_mem_format = format;
    cpu.fp_mem_opmode = opmode as u8;
    cpu.fp_mem_dst = ((w2 >> 7) & 7) as u8;
    cpu.in_followup = true;

    if immediate {
        // The operand follows the FP extension word inline, one or more
        // words. The FP-ext-word FetchIRC refills IRC with the first
        // operand word before TAG_V_FP_IMM_READ runs.
        cpu.followup_tag = motorola_68000::cpu::TAG_V_FP_IMM_READ;
        cpu.micro_ops.push(MicroOp::Execute);
        return true;
    }

    if static_mode {
        // Let the core resolve the EA (it may consume extension words and
        // span several Execute cycles). We resume at TAG_FETCH_SRC_DATA via
        // the continue hook, where `addr` holds the resolved address.
        // Queue an Execute (rather than calling continue_instruction now)
        // so the FP extension word's FetchIRC refills IRC with the EA's
        // first extension word before calc_ea_start reads it.
        cpu.fp_mem_pending = true;
        cpu.size = motorola_68000::alu::Size::Long; // unused by static modes
        cpu.src_mode = motorola_68000::addressing::AddrMode::decode(ea_mode, ea_reg);
        cpu.followup_tag = motorola_68000::cpu::TAG_FETCH_SRC_EA;
        cpu.micro_ops.push(MicroOp::Execute);
        return true;
    }

    // Auto-increment/decrement modes: resolve the base address here,
    // stepping the register by the operand size.
    let ea_reg = ea_reg as usize;
    let step = u32::from(bytes_total);
    let base = match ea_mode {
        2 => cpu.regs.a(ea_reg),
        3 => {
            let a = cpu.regs.a(ea_reg);
            cpu.regs.set_a(ea_reg, a.wrapping_add(step));
            a
        }
        _ => {
            let a = cpu.regs.a(ea_reg).wrapping_sub(step);
            cpu.regs.set_a(ea_reg, a);
            a
        }
    };
    cpu.addr = base;
    start_fp_read(cpu);
    true
}

/// Kick off the byte-at-a-time operand read. The operand parameters and
/// the base address (`addr`) must already be set; shared by the
/// auto-increment modes and the `calc_ea_start`-resolved static modes.
fn start_fp_read(cpu: &mut Cpu68000) {
    cpu.fp_mem_buf = 0;
    cpu.fp_mem_bytes_done = 0;
    cpu.followup_tag = TAG_V_FP_MEM_READ;
    cpu.micro_ops.push(MicroOp::ReadByte);
    cpu.micro_ops.push(MicroOp::Execute);
}

/// `TAG_V_FP_MEM_READ`: a byte of the FP memory operand has been read into
/// `cpu.data`. Accumulate it big-endian and either queue the next byte or
/// hand off to `TAG_V_FP_MEM_EXEC`.
fn handle_fp_mem_read(cpu: &mut Cpu68000) {
    cpu.fp_mem_buf = (cpu.fp_mem_buf << 8) | u128::from(cpu.data & 0xFF);
    cpu.fp_mem_bytes_done += 1;
    if cpu.fp_mem_bytes_done < cpu.fp_mem_bytes_total {
        cpu.addr = cpu.addr.wrapping_add(1);
        cpu.micro_ops.push(MicroOp::ReadByte);
        cpu.micro_ops.push(MicroOp::Execute);
    } else {
        // A FMOVEM register transfer hands back to its controller; a plain
        // FMOVE operand goes to the format-parse exec.
        cpu.followup_tag = if cpu.fp_movem_active {
            TAG_V_FMOVEM_STEP
        } else {
            TAG_V_FP_MEM_EXEC
        };
        cpu.micro_ops.push(MicroOp::Execute);
    }
}

/// `TAG_V_FP_IMM_READ`: an immediate operand word is in `irc`. Accumulate
/// it (word-aligned, big-endian) and either read the next word or run the
/// op. The operand spans `ceil(bytes_total / 2)` words: byte/word = 1,
/// long/single = 2, double = 4, extended = 6.
fn handle_fp_imm_read(cpu: &mut Cpu68000) {
    cpu.fp_mem_buf = (cpu.fp_mem_buf << 16) | u128::from(cpu.irc);
    cpu.fp_mem_bytes_done += 2;
    let _ = cpu.consume_irc(); // advance the prefetch to the next word
    if cpu.fp_mem_bytes_done < cpu.fp_mem_bytes_total {
        cpu.micro_ops.push(MicroOp::Execute);
    } else {
        cpu.followup_tag = TAG_V_FP_MEM_EXEC;
        cpu.micro_ops.push(MicroOp::Execute);
    }
}

/// `TAG_V_FP_MEM_EXEC`: the whole operand is in `fp_mem_buf` (big-endian
/// in the low bytes). Convert it to a `floatx80` by its format and apply
/// the stashed opmode, exactly as the register-to-register path would.
fn handle_fp_mem_exec(cpu: &mut Cpu68000) {
    use motorola_68k_common::registers::FpReg;
    use motorola_68k_common::softfloat::{self, RoundingMode};

    // Control-register move (FMOVE.L ea,FPcr): the 0xFF format sentinel
    // means the 32-bit operand goes to a control register (mask in
    // fp_mem_dst), not an Fpn.
    if cpu.fp_mem_format == 0xFF {
        let value = cpu.fp_mem_buf as u32;
        let mask = cpu.fp_mem_dst;
        if mask & 4 != 0 {
            cpu.regs.fpcr = value;
        }
        if mask & 2 != 0 {
            cpu.regs.fpsr = value;
        }
        if mask & 1 != 0 {
            cpu.regs.fpiar = value;
        }
        cpu.fp_mem_format = 0;
        cpu.in_followup = false;
        return;
    }

    // Clear the IEEE exception accumulator before the operand conversion so
    // a signalling-NaN widen (SNAN) is captured alongside the opmode's flags;
    // apply_fp_opmode folds them into the FPSR at the end.
    softfloat::clear_exception_flags();
    let buf = cpu.fp_mem_buf;
    let mode = RoundingMode::from_fpcr_bits(cpu.regs.fpcr_rounding_mode());
    let source = match cpu.fp_mem_format {
        0 => softfloat::int32_to_floatx80(buf as u32 as i32), // Long
        1 => softfloat::float32_to_floatx80(buf as u32),      // Single
        // Extended: 12 bytes — high word (sign+exp) is bytes 0-1, the
        // 16-bit pad word (bytes 2-3) is ignored, the 64-bit mantissa is
        // bytes 4-11.
        2 => FpReg::new(((buf >> 80) & 0xFFFF) as u16, buf as u64),
        // Packed-decimal: the 12 bytes are three big-endian longwords.
        3 => softfloat::pack_decimal_to_floatx80(
            [(buf >> 64) as u32, (buf >> 32) as u32, buf as u32],
            mode,
        ),
        4 => softfloat::int32_to_floatx80(i32::from(buf as u16 as i16)), // Word
        5 => softfloat::float64_to_floatx80(buf as u64),                 // Double
        6 => softfloat::int32_to_floatx80(i32::from(buf as u8 as i8)),   // Byte
        _ => FpReg::ZERO,
    };
    let opmode = u16::from(cpu.fp_mem_opmode);
    let dst = cpu.fp_mem_dst as usize;
    apply_fp_opmode(cpu, opmode, source, dst, mode, cpu.fp_mem_precision);
    cpu.in_followup = false;
}

/// Begin an FPU memory *store* (FMOVE FPn → `<ea>`, cpGEN sub-op 3). The
/// extension word's destination-format field selects how the source Fpn
/// is narrowed; the opcode's EA field selects the address. All destination
/// formats are handled, including the 12-byte packed-decimal real with both
/// a static (format 3) and a dynamic (format 7, `P{Dn}`) k-factor, across
/// the auto-increment and static (control) addressing modes. PC-relative and
/// immediate are not valid store destinations. FMOVE-to-memory sets no
/// condition codes.
///
/// Decline checks run before any state mutation.
fn begin_fp_store(cpu: &mut Cpu68000, mode: motorola_68k_common::softfloat::RoundingMode) -> bool {
    use motorola_68k_common::softfloat;

    let w2 = cpu.irc;
    let format = ((w2 >> 10) & 7) as u8; // destination format
    let bytes_total: u8 = match format {
        6 => 1,            // Byte integer
        4 => 2,            // Word integer
        0 | 1 => 4,        // Long integer / Single
        5 => 8,            // Double
        2 => 12,           // Extended
        3 | 7 => 12,       // Packed-decimal (static / dynamic k-factor)
        _ => return false, // (formats 0-7 cover every cpGEN destination)
    };
    let ea_mode = ((cpu.ir >> 3) & 7) as u8;
    let ea_reg_bits = (cpu.ir & 7) as u8;
    // Stores target alterable memory: the auto-increment modes plus the
    // static modes (d16(An), indexed, abs). PC-relative and immediate are
    // not valid store destinations.
    let static_mode = match ea_mode {
        5 | 6 => true,
        7 => matches!(ea_reg_bits, 0 | 1),
        _ => false,
    };
    if !matches!(ea_mode, 2..=4) && !static_mode {
        return false;
    }

    // Committed: consume the FP extension word and narrow the source Fpn
    // to the destination format, packed big-endian in the low bytes.
    let _ = cpu.consume_irc();
    let src = ((w2 >> 7) & 7) as usize;
    let v = cpu.regs.fp[src];
    // Narrowing a register to the store format raises IEEE exceptions
    // (overflow/underflow/inexact, or invalid on an out-of-range integer or
    // a signalling NaN). Extended store is exact. Fold them into the FPSR.
    // Packed-decimal k-factor (M68000PRM § 4.4): static (format 3) reads it
    // from the command word's low 7 bits; dynamic (format 7) reads it from the
    // Dn selected by ext-word bits 6-4. Mask to 7 bits and sign-extend bit 6.
    let kfactor = {
        let raw = if format == 7 {
            cpu.regs.d[((w2 >> 4) & 7) as usize]
        } else {
            u32::from(w2)
        };
        let mut k = (raw & 0x7F) as i32;
        if k & 0x40 != 0 {
            k |= !0x3F;
        }
        k
    };
    softfloat::clear_exception_flags();
    let buf: u128 = match format {
        0 => u128::from(softfloat::floatx80_to_int32(mode, v) as u32),
        1 => u128::from(softfloat::floatx80_to_float32(mode, v)),
        // Extended: high word (bytes 0-1), a zero pad word (bytes 2-3),
        // then the 64-bit mantissa (bytes 4-11).
        2 => (u128::from(v.high) << 80) | u128::from(v.low),
        // Packed-decimal: three big-endian longwords.
        3 | 7 => {
            let wrd = softfloat::floatx80_to_pack_decimal(v, kfactor, mode);
            (u128::from(wrd[0]) << 64) | (u128::from(wrd[1]) << 32) | u128::from(wrd[2])
        }
        4 => u128::from(softfloat::floatx80_to_int32(mode, v) as u16),
        5 => u128::from(softfloat::floatx80_to_float64(mode, v)),
        6 => u128::from(softfloat::floatx80_to_int32(mode, v) as u8),
        _ => 0,
    };
    motorola_68k_common::fpu::apply_exceptions(&mut cpu.regs);

    cpu.fp_mem_buf = buf;
    cpu.fp_mem_bytes_total = bytes_total;

    if static_mode {
        // Resolve the destination address via the core EA machinery, then
        // start the write at TAG_FETCH_SRC_DATA (fp_mem_store = true).
        cpu.fp_mem_pending = true;
        cpu.fp_mem_store = true;
        cpu.size = motorola_68000::alu::Size::Long;
        cpu.src_mode = motorola_68000::addressing::AddrMode::decode(ea_mode, ea_reg_bits);
        cpu.in_followup = true;
        cpu.followup_tag = motorola_68000::cpu::TAG_FETCH_SRC_EA;
        cpu.micro_ops.push(MicroOp::Execute);
        return true;
    }

    let ea_reg = ea_reg_bits as usize;
    let step = u32::from(bytes_total);
    let base = match ea_mode {
        2 => cpu.regs.a(ea_reg),
        3 => {
            let a = cpu.regs.a(ea_reg);
            cpu.regs.set_a(ea_reg, a.wrapping_add(step));
            a
        }
        _ => {
            let a = cpu.regs.a(ea_reg).wrapping_sub(step);
            cpu.regs.set_a(ea_reg, a);
            a
        }
    };
    cpu.addr = base;
    cpu.in_followup = true;
    start_fp_write(cpu);
    true
}

/// Kick off the byte-at-a-time store. The operand (`fp_mem_buf`),
/// `fp_mem_bytes_total`, and the base address (`addr`) must already be
/// set; shared by the auto-increment and EA-resolved store paths.
fn start_fp_write(cpu: &mut Cpu68000) {
    cpu.fp_mem_bytes_done = 0;
    let shift = 8 * u32::from(cpu.fp_mem_bytes_total - 1);
    cpu.data = ((cpu.fp_mem_buf >> shift) & 0xFF) as u32;
    cpu.followup_tag = TAG_V_FP_MEM_WRITE;
    cpu.micro_ops.push(MicroOp::WriteByte);
    cpu.micro_ops.push(MicroOp::Execute);
}

/// `TAG_V_FP_MEM_WRITE`: a byte of the store operand has been written.
/// Advance to the next byte (big-endian) or finish the instruction.
fn handle_fp_mem_write(cpu: &mut Cpu68000) {
    cpu.fp_mem_bytes_done += 1;
    if cpu.fp_mem_bytes_done < cpu.fp_mem_bytes_total {
        cpu.addr = cpu.addr.wrapping_add(1);
        let shift = 8 * u32::from(cpu.fp_mem_bytes_total - 1 - cpu.fp_mem_bytes_done);
        cpu.data = ((cpu.fp_mem_buf >> shift) & 0xFF) as u32;
        cpu.micro_ops.push(MicroOp::WriteByte);
        cpu.micro_ops.push(MicroOp::Execute);
    } else if cpu.fp_movem_active {
        // A FMOVEM register store hands back to its controller for the
        // next register.
        cpu.followup_tag = TAG_V_FMOVEM_STEP;
        cpu.micro_ops.push(MicroOp::Execute);
    } else {
        cpu.in_followup = false;
        cpu.followup_tag = 0;
    }
}

/// FMOVEM register list ↔ memory (cpGEN sub-op 6/7, Musashi `fmovem`).
/// Each register is a 12-byte extended-format transfer. The two common
/// idioms are wired: `FMOVEM <list>,-(An)` (static predecrement store —
/// the prologue save) and `FMOVEM (An)+,<list>` (static postincrement
/// load — the epilogue restore). The dynamic-list, control-addressing,
/// and other combinations decline (vector-11 trap).
///
/// The register list is in the extension word's low byte. Predecrement
/// stores `REG_FP[i]` for each set bit i = 0..7 (so the lowest-numbered
/// register lands at the highest address); postincrement loads
/// `REG_FP[7-i]` — the mirror order, so a save/restore pair round-trips.
fn begin_fmovem(cpu: &mut Cpu68000, w2: u16) -> bool {
    let dir = (w2 >> 13) & 1;
    let w2_mode = (w2 >> 11) & 3;
    let reglist = (w2 & 0xFF) as u8;
    let ea_mode = ((cpu.ir >> 3) & 7) as u8;
    let ea_reg = (cpu.ir & 7) as u8;

    // dir 1 = registers → memory (predecrement, EA = -(An));
    // dir 0 = memory → registers (postincrement, EA = (An)+).
    let predec_store = dir == 1 && w2_mode == 0 && ea_mode == 4;
    let postinc_load = dir == 0 && w2_mode == 2 && ea_mode == 3;
    if !predec_store && !postinc_load {
        return false;
    }

    let _ = cpu.consume_irc();
    cpu.fp_movem_active = true;
    cpu.fp_movem_store = predec_store;
    cpu.fp_movem_list = reglist;
    cpu.fp_movem_cur = 0xFF; // no register processed yet
    cpu.fp_movem_an = cpu.regs.a(ea_reg as usize);
    cpu.fp_movem_areg = ea_reg;
    cpu.in_followup = true;
    cpu.followup_tag = motorola_68000::cpu::TAG_V_FMOVEM_STEP;
    cpu.micro_ops.push(MicroOp::Execute);
    true
}

/// `TAG_V_FMOVEM_STEP`: process the register just transferred (for a load,
/// unpack the 12 bytes into its Fpn), then start the next register's
/// transfer or finish the instruction (writing the stepped pointer back
/// to the An register).
fn handle_fmovem_step(cpu: &mut Cpu68000) {
    use motorola_68k_common::registers::FpReg;

    // Unpack the register a load just read (extended: high word + 64-bit
    // mantissa; the pad word is ignored). `cur == 0xFF` on the first call.
    if cpu.fp_movem_cur != 0xFF && !cpu.fp_movem_store {
        let reg = (7 - cpu.fp_movem_cur) as usize;
        cpu.regs.fp[reg] = FpReg::new((cpu.fp_mem_buf >> 80) as u16, cpu.fp_mem_buf as u64);
    }

    if cpu.fp_movem_list == 0 {
        // All registers done — commit the stepped pointer and finish.
        cpu.regs.set_a(cpu.fp_movem_areg as usize, cpu.fp_movem_an);
        cpu.fp_movem_active = false;
        cpu.fp_movem_cur = 0xFF;
        cpu.in_followup = false;
        cpu.followup_tag = 0;
        return;
    }

    // Take the lowest remaining register (matching Musashi's i = 0..7 loop).
    let i = cpu.fp_movem_list.trailing_zeros() as u8;
    cpu.fp_movem_list &= !(1 << i);
    cpu.fp_movem_cur = i;
    cpu.fp_mem_bytes_total = 12;

    if cpu.fp_movem_store {
        // Predecrement: step the pointer back, then write REG_FP[i].
        cpu.fp_movem_an = cpu.fp_movem_an.wrapping_sub(12);
        cpu.addr = cpu.fp_movem_an;
        let v = cpu.regs.fp[i as usize];
        cpu.fp_mem_buf = (u128::from(v.high) << 80) | u128::from(v.low);
        start_fp_write(cpu);
    } else {
        // Postincrement: read at the pointer, then step it forward. The
        // value is unpacked into REG_FP[7-i] on the next step.
        cpu.addr = cpu.fp_movem_an;
        cpu.fp_movem_an = cpu.fp_movem_an.wrapping_add(12);
        start_fp_read(cpu);
    }
}

// ─── FSAVE / FRESTORE (cpID-1 op-classes 4/5) ──────────────────────────
//
// FSAVE/FRESTORE move only the FPU's *internal* state to / from a memory
// frame — the FP data registers, FPCR/FPSR/FPIAR are saved separately by
// FMOVEM. For our synchronous core (no mid-instruction exception or busy
// state) the frame is purely formal: the frame id, then for an idle frame
// a fixed run of zeroed control/operand longwords plus the BIU flags. Two
// models are supported (WinUAE `fpuop_save` / `fpuop_restore`, 6888x
// branch): the MC68881 (28-byte idle frame) and the MC68882 (60-byte idle
// frame). Both report `fpu_version` $1F; only the frame size differs.

/// Build the FSAVE internal-state frame into `cpu.fp_frame`, returning the
/// total byte count. A null frame (FPU never used since reset) is just the
/// 4-byte frame id with a zero version byte; an idle frame carries the
/// version byte plus zeroed condition/operand fields and the BIU flags.
fn build_fsave_frame(cpu: &mut Cpu68000) -> u8 {
    // Frame size byte (size − 4): $18 (68881) / $38 (68882).
    let size_byte: u32 = if cpu.variant_fpu_is_68882 { 0x38 } else { 0x18 };
    let mut lw: [u32; 15] = [0; 15];
    let total: u8;

    if cpu.variant_fpu_state == 0 {
        // Null frame: version byte 0, only the 4-byte frame id is written.
        lw[0] = size_byte << 16;
        total = 4;
    } else {
        // Idle frame: version $1F. The BIU flags base is $540EFFFF; with no
        // pending exception (our core never has one) bit 27 is set, giving
        // $5C0EFFFF. The command/condition register, the three exceptional-
        // operand longwords, the operand register, and (68882 only) the 8
        // unused internal longwords are all zero for us.
        lw[0] = (0x1F << 24) | (size_byte << 16);
        if cpu.variant_fpu_is_68882 {
            // 15 longwords: id, ccr, 8×unused, eo0, eo1, eo2, operand, biu.
            lw[14] = 0x5C0E_FFFF;
            total = 60;
        } else {
            // 7 longwords: id, ccr, eo0, eo1, eo2, operand, biu.
            lw[6] = 0x5C0E_FFFF;
            total = 28;
        }
    }

    // Pack the longwords big-endian into the byte frame.
    for (i, word) in lw.iter().enumerate() {
        let base = i * 4;
        cpu.fp_frame[base] = (word >> 24) as u8;
        cpu.fp_frame[base + 1] = (word >> 16) as u8;
        cpu.fp_frame[base + 2] = (word >> 8) as u8;
        cpu.fp_frame[base + 3] = *word as u8;
    }
    total
}

/// FSAVE (cpID-1 op-class 4): write the FPU's internal-state frame to the
/// effective address. The destination is control-alterable or `-(An)`
/// (predecrement) — never postincrement, register-direct, PC-relative, or
/// immediate. FSAVE does not change the FPU state it reports.
///
/// Decline checks run before any state mutation.
fn begin_fsave(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let ea_mode = ((opcode >> 3) & 7) as u8;
    let ea_reg = (opcode & 7) as u8;

    // Control-alterable: (An), d16(An), (d8,An,Xn), abs.W, abs.L. Plus the
    // predecrement -(An). Postincrement, Dn/An-direct, PC-relative, and
    // immediate are not valid FSAVE destinations.
    let predec = ea_mode == 4;
    let direct = ea_mode == 2;
    let static_mode = matches!(ea_mode, 5 | 6) || (ea_mode == 7 && matches!(ea_reg, 0 | 1));
    if !predec && !direct && !static_mode {
        return false;
    }

    // Committed: build the frame and stream it out. FSAVE has no FP
    // extension word, so `irc` already holds the first EA extension word
    // (or the next opcode for the register-indirect modes) — leave it.
    let total = build_fsave_frame(cpu);
    cpu.fp_frame_total = total;
    cpu.fp_frame_done = 0;
    cpu.in_followup = true;

    if static_mode {
        // Resolve the destination via the core EA machinery, then start the
        // write at TAG_FETCH_SRC_DATA (fp_frame_pending + store).
        cpu.fp_frame_pending = true;
        cpu.fp_frame_store = true;
        cpu.size = motorola_68000::alu::Size::Long;
        cpu.src_mode = motorola_68000::addressing::AddrMode::decode(ea_mode, ea_reg);
        cpu.followup_tag = motorola_68000::cpu::TAG_FETCH_SRC_EA;
        cpu.micro_ops.push(MicroOp::Execute);
        return true;
    }

    let ea_reg = ea_reg as usize;
    let base = if predec {
        // Predecrement by the whole frame, write back the new pointer.
        let a = cpu.regs.a(ea_reg).wrapping_sub(u32::from(total));
        cpu.regs.set_a(ea_reg, a);
        a
    } else {
        cpu.regs.a(ea_reg)
    };
    cpu.addr = base;
    start_fsave_write(cpu);
    true
}

/// Kick off the byte-at-a-time FSAVE frame write. `fp_frame`,
/// `fp_frame_total`, and the base address (`addr`) must already be set.
fn start_fsave_write(cpu: &mut Cpu68000) {
    cpu.fp_frame_done = 0;
    cpu.data = u32::from(cpu.fp_frame[0]);
    cpu.followup_tag = TAG_V_FSAVE_WRITE;
    cpu.micro_ops.push(MicroOp::WriteByte);
    cpu.micro_ops.push(MicroOp::Execute);
}

/// `TAG_V_FSAVE_WRITE`: a frame byte has been written. Advance to the next
/// byte (big-endian, ascending address) or finish the instruction.
fn handle_fsave_write(cpu: &mut Cpu68000) {
    cpu.fp_frame_done += 1;
    if cpu.fp_frame_done < cpu.fp_frame_total {
        cpu.addr = cpu.addr.wrapping_add(1);
        cpu.data = u32::from(cpu.fp_frame[cpu.fp_frame_done as usize]);
        cpu.micro_ops.push(MicroOp::WriteByte);
        cpu.micro_ops.push(MicroOp::Execute);
    } else {
        cpu.in_followup = false;
        cpu.followup_tag = 0;
    }
}

/// FBcc.W (cpID-1 op-class 2): branch on an FPU condition with a 16-bit
/// displacement. The 6-bit condition is in the opcode's low bits and is
/// evaluated against the FPSR condition codes; a taken branch targets
/// `instr_start + 2 + disp` (the displacement-word address), matching
/// the integer Bcc.W and Musashi's `fbcc16`. FNOP is the `FBF.W` (never)
/// special case with a zero displacement.
fn execute_fbcc_w(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let condition = (opcode & 0x3F) as u8;
    let disp = i32::from(cpu.irc as i16) as u32;
    if motorola_68k_common::fpu::predicate_raises_bsun(condition, cpu.regs.fpsr) {
        cpu.regs.set_fpsr_bsun();
    }
    if motorola_68k_common::fpu::test_condition(cpu.regs.fpsr, condition) {
        let target = cpu.instr_start_pc.wrapping_add(2).wrapping_add(disp);
        cpu.regs.pc = target;
        cpu.next_fetch_addr = target;
        cpu.micro_ops.clear();
        cpu.micro_ops.push(MicroOp::FetchIRC);
        cpu.micro_ops.push(MicroOp::PromoteIRC);
    } else {
        // Not taken: advance the prefetch past the displacement word to
        // the next instruction.
        cpu.micro_ops.push(MicroOp::FetchIRC);
    }
    true
}

/// FBcc.L (cpID-1 op-class 3): branch on an FPU condition with a 32-bit
/// displacement. The 6-bit condition is in the opcode's low bits; the
/// high displacement word is already prefetched in `irc`, the low word is
/// fetched next and combined at `TAG_V_FBCC_L` (mirrors the integer
/// `Bcc.L` gather).
fn execute_fbcc_l(cpu: &mut Cpu68000, opcode: u16) -> bool {
    cpu.variant_ext_word = opcode & 0x3F; // stash the condition
    cpu.src_val = u32::from(cpu.irc) << 16; // high displacement word
    cpu.in_followup = true;
    cpu.followup_tag = motorola_68000::cpu::TAG_V_FBCC_L;
    cpu.micro_ops.push(MicroOp::FetchIRC);
    cpu.micro_ops.push(MicroOp::Execute);
    true
}

/// `TAG_V_FBCC_L`: the low displacement word is now in `irc`. Combine it
/// with the stashed high word and take the branch if the FP condition
/// holds. The displacement is relative to `instr_start + 2` (the first
/// displacement word), matching FBcc.W and the integer long branch.
fn handle_fbcc_l(cpu: &mut Cpu68000) {
    let disp = (cpu.src_val | u32::from(cpu.irc)) as i32;
    let condition = (cpu.variant_ext_word & 0x3F) as u8;
    if motorola_68k_common::fpu::predicate_raises_bsun(condition, cpu.regs.fpsr) {
        cpu.regs.set_fpsr_bsun();
    }
    if motorola_68k_common::fpu::test_condition(cpu.regs.fpsr, condition) {
        let target = cpu.instr_start_pc.wrapping_add(2).wrapping_add(disp as u32);
        cpu.regs.pc = target;
        cpu.next_fetch_addr = target;
        cpu.micro_ops.clear();
        cpu.micro_ops.push(MicroOp::FetchIRC);
        cpu.micro_ops.push(MicroOp::PromoteIRC);
    } else {
        cpu.micro_ops.push(MicroOp::FetchIRC);
    }
    cpu.in_followup = false;
}

/// FScc (cpID-1 op-class 1): set a byte integer to all-ones if the FP
/// condition holds, else all-zeros. The condition is the low 6 bits of
/// the extension word. The destination is a byte-alterable EA: `Dn`
/// (mode 0), `(An)`/`(An)+`/`-(An)`, `d16(An)`/indexed, or absolute —
/// the memory forms reuse the store byte-pipeline. The same op-class
/// encodes FDBcc (EA mode 1) and FTRAPcc (EA mode 7, regs 2-4); those and
/// the non-alterable EAs decline (vector-11 trap).
fn execute_fscc(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let ea_mode = ((opcode >> 3) & 7) as u8;
    let ea_reg = (opcode & 7) as u8;
    // The op-class-1 encoding multiplexes FScc / FDBcc / FTRAPcc on the EA
    // mode: mode 1 (the An field as a counter Dn) is FDBcc; mode 7 regs 2-4
    // are FTRAPcc; otherwise it is FScc (Dn or a byte-alterable memory EA).
    if ea_mode == 1 {
        return execute_fdbcc(cpu, ea_reg);
    }
    if ea_mode == 7 && matches!(ea_reg, 2..=4) {
        return execute_ftrapcc(cpu, ea_reg);
    }
    let static_mode = match ea_mode {
        5 | 6 => true,
        7 => matches!(ea_reg, 0 | 1),
        _ => false,
    };
    let memory = matches!(ea_mode, 2..=4) || static_mode;
    if ea_mode != 0 && !memory {
        return false; // Dn or a byte-alterable memory EA only
    }

    let condition = (cpu.irc & 0x3F) as u8;
    let _ = cpu.consume_irc();
    if motorola_68k_common::fpu::predicate_raises_bsun(condition, cpu.regs.fpsr) {
        cpu.regs.set_fpsr_bsun();
    }
    let byte = if motorola_68k_common::fpu::test_condition(cpu.regs.fpsr, condition) {
        0xFF_u32
    } else {
        0x00
    };

    if ea_mode == 0 {
        let reg = ea_reg as usize;
        cpu.regs.d[reg] = (cpu.regs.d[reg] & 0xFFFF_FF00) | byte;
        return true;
    }

    // Memory destination: a single-byte store of the condition byte,
    // reusing the FP store pipeline (fp_mem_buf / start_fp_write).
    cpu.fp_mem_buf = u128::from(byte);
    cpu.fp_mem_bytes_total = 1;
    cpu.in_followup = true;
    if static_mode {
        cpu.fp_mem_pending = true;
        cpu.fp_mem_store = true;
        cpu.size = motorola_68000::alu::Size::Byte;
        cpu.src_mode = motorola_68000::addressing::AddrMode::decode(ea_mode, ea_reg);
        cpu.followup_tag = motorola_68000::cpu::TAG_FETCH_SRC_EA;
        cpu.micro_ops.push(MicroOp::Execute);
        return true;
    }
    let reg = ea_reg as usize;
    let base = match ea_mode {
        2 => cpu.regs.a(reg),
        3 => {
            let a = cpu.regs.a(reg);
            cpu.regs.set_a(reg, a.wrapping_add(1));
            a
        }
        _ => {
            let a = cpu.regs.a(reg).wrapping_sub(1);
            cpu.regs.set_a(reg, a);
            a
        }
    };
    cpu.addr = base;
    start_fp_write(cpu);
    true
}

/// FDBcc (op-class 1, EA mode 1): test the FP condition; if false, decrement
/// the low word of the counter `Dn` and branch unless it underflows to −1.
/// The 16-bit displacement follows the condition word, so it is fetched
/// asynchronously and the branch resolved at [`handle_fdbcc`].
fn execute_fdbcc(cpu: &mut Cpu68000, counter_reg: u8) -> bool {
    let condition = (cpu.irc & 0x3F) as u8;
    // Stash the counter register and condition for the follow-up.
    cpu.variant_ext_word = (u16::from(counter_reg) << 8) | u16::from(condition);
    let _ = cpu.consume_irc(); // queue the displacement-word fetch into IRC
    cpu.in_followup = true;
    cpu.followup_tag = motorola_68000::cpu::TAG_V_FDBCC;
    cpu.micro_ops.push(MicroOp::Execute);
    true
}

/// `TAG_V_FDBCC`: the displacement word is now in `irc`. Per the MC68881UM the
/// branch PC is the address of the displacement word (`instr_start + 4`).
fn handle_fdbcc(cpu: &mut Cpu68000) {
    let condition = (cpu.variant_ext_word & 0x3F) as u8;
    let reg = ((cpu.variant_ext_word >> 8) & 7) as usize;
    if motorola_68k_common::fpu::predicate_raises_bsun(condition, cpu.regs.fpsr) {
        cpu.regs.set_fpsr_bsun();
    }
    let disp = i32::from(cpu.irc as i16) as u32;
    if motorola_68k_common::fpu::test_condition(cpu.regs.fpsr, condition) {
        // Condition true → loop terminates; fall through past the displacement.
        cpu.micro_ops.push(MicroOp::FetchIRC);
    } else {
        let dn = cpu.regs.d[reg];
        let count = (dn as u16).wrapping_sub(1);
        cpu.regs.d[reg] = (dn & 0xFFFF_0000) | u32::from(count);
        if count != 0xFFFF {
            let target = cpu.instr_start_pc.wrapping_add(4).wrapping_add(disp);
            cpu.regs.pc = target;
            cpu.next_fetch_addr = target;
            cpu.micro_ops.clear();
            cpu.micro_ops.push(MicroOp::FetchIRC);
            cpu.micro_ops.push(MicroOp::PromoteIRC);
        } else {
            cpu.micro_ops.push(MicroOp::FetchIRC);
        }
    }
    cpu.in_followup = false;
}

/// FTRAPcc (op-class 1, EA mode 7, regs 2-4): trap (vector 7) if the FP
/// condition holds. `reg` selects the optional operand: 2 = word, 3 = long,
/// 4 = none. The operand is discarded (like the integer TRAPcc); the
/// condition is already in `irc`, so this resolves synchronously.
fn execute_ftrapcc(cpu: &mut Cpu68000, reg: u8) -> bool {
    let condition = (cpu.irc & 0x3F) as u8;
    let operand_words: u32 = match reg {
        2 => 1,
        3 => 2,
        _ => 0,
    };
    let _ = cpu.consume_irc(); // step past the condition word
    for _ in 0..operand_words {
        cpu.consume_irc(); // step past the discarded operand words
    }
    if motorola_68k_common::fpu::predicate_raises_bsun(condition, cpu.regs.fpsr) {
        cpu.regs.set_fpsr_bsun();
    }
    if motorola_68k_common::fpu::test_condition(cpu.regs.fpsr, condition) {
        // Stacked PC points past the whole instruction (opcode + condition +
        // operand). begin_group1_exception clears the prefetch queue.
        let next_pc = cpu.instr_start_pc.wrapping_add(4 + operand_words * 2);
        cpu.begin_group1_exception(7, next_pc);
    }
    true
}

/// FMOVE FPCR/FPSR/FPIAR ↔ `<ea>` (cpGEN sub-op 4/5, Musashi
/// `fmove_fpcr`). The extension word's register mask (bits 12-10) selects
/// the control register(s): bit 2 = FPCR, bit 1 = FPSR, bit 0 = FPIAR;
/// bit 13 (the sub-op's low bit) gives the direction (0 = ea → register,
/// 1 = register → ea). Each transfer is a 32-bit longword.
///
/// Only the register-direct EA modes (Dn / An) are wired so far — the
/// common `FMOVE.L D0,FPCR` / `FMOVE.L FPSR,D0` idioms, which need no
/// memory access and carry exactly one control register. Memory and
/// immediate EAs decline (vector-11 trap) until the EA chain is wired.
fn execute_fmove_control(cpu: &mut Cpu68000, w2: u16) -> bool {
    let dir = (w2 >> 13) & 1;
    let reg_mask = ((w2 >> 10) & 7) as u8;
    let ea_mode = ((cpu.ir >> 3) & 7) as u8;
    let ea_reg = (cpu.ir & 7) as u8;

    // Register-direct (Dn / An): the transfer is synchronous and carries
    // whichever control register(s) the mask selects.
    if matches!(ea_mode, 0 | 1) {
        let _ = cpu.consume_irc();
        let r = ea_reg as usize;
        if dir == 0 {
            // ea → control register(s). For a Dn/An source the same value
            // feeds every selected register (only one is set in practice).
            let value = if ea_mode == 0 {
                cpu.regs.d[r]
            } else {
                cpu.regs.a(r)
            };
            if reg_mask & 4 != 0 {
                cpu.regs.fpcr = value;
            }
            if reg_mask & 2 != 0 {
                cpu.regs.fpsr = value;
            }
            if reg_mask & 1 != 0 {
                cpu.regs.fpiar = value;
            }
        } else {
            // control register → ea (last selected wins for a register
            // destination, per Musashi's FPCR/FPSR/FPIAR write order).
            let value = control_reg_value(cpu, reg_mask);
            if let Some(value) = value {
                if ea_mode == 0 {
                    cpu.regs.d[r] = value;
                } else {
                    cpu.regs.set_a(r, value);
                }
            }
        }
        return true;
    }

    // Memory: a single control register transferred as one 32-bit
    // longword, instant addressing modes only ((An)/(An)+/-(An)). The
    // multi-register and static-mode forms decline for now.
    if reg_mask.count_ones() != 1 || !matches!(ea_mode, 2..=4) {
        return false;
    }
    let _ = cpu.consume_irc();
    let r = ea_reg as usize;
    let step = 4u32;
    let base = match ea_mode {
        2 => cpu.regs.a(r),
        3 => {
            let a = cpu.regs.a(r);
            cpu.regs.set_a(r, a.wrapping_add(step));
            a
        }
        _ => {
            let a = cpu.regs.a(r).wrapping_sub(step);
            cpu.regs.set_a(r, a);
            a
        }
    };
    cpu.fp_mem_bytes_total = 4;
    cpu.addr = base;
    cpu.in_followup = true;
    if dir == 1 {
        // control register → memory: a plain 4-byte store.
        cpu.fp_mem_buf = u128::from(control_reg_value(cpu, reg_mask).unwrap_or(0));
        start_fp_write(cpu);
    } else {
        // memory → control register: read 4 bytes, then write the control
        // register at TAG_V_FP_MEM_EXEC. The 0xFF format is a sentinel for
        // "control move" so the exec writes a control register instead of
        // an Fpn; the register mask rides in fp_mem_dst.
        cpu.fp_mem_format = 0xFF;
        cpu.fp_mem_dst = reg_mask;
        start_fp_read(cpu);
    }
    true
}

/// The value of the (single) control register selected by `mask`, in
/// Musashi's FPCR → FPSR → FPIAR fold order (last selected wins).
fn control_reg_value(cpu: &Cpu68000, mask: u8) -> Option<u32> {
    let mut value = None;
    if mask & 4 != 0 {
        value = Some(cpu.regs.fpcr);
    }
    if mask & 2 != 0 {
        value = Some(cpu.regs.fpsr);
    }
    if mask & 1 != 0 {
        value = Some(cpu.regs.fpiar);
    }
    value
}

/// Finish CAS once the destination has been read into `self.data`.
/// Computes the comparison flags, then either writes Du back (equal) or
/// loads the read value into Dc (not equal). Returns `true` when a write
/// was queued (the caller must not end the instruction yet).
fn finish_cas(cpu: &mut Cpu68000) -> bool {
    let ext = cpu.variant_ext_word;
    let size = cpu.size;
    let dc = (ext & 7) as usize;
    let du = ((ext >> 6) & 7) as usize;
    let dest = cpu.data;
    let compare = cpu.regs.d[dc];

    // Flags = dest - Dc, with X preserved (CMP semantics).
    cpu.exec_alu(motorola_68000::cpu::AluOp::Cmp, compare, dest, size);

    if cpu.regs.sr & Z != 0 {
        // Equal: write Du back to [EA]. `cpu.addr` still holds the EA;
        // the write micro-ops mask `cpu.data` to size.
        cpu.data = cpu.regs.d[du];
        cpu.followup_tag = TAG_V_CAS_WRITE_DONE;
        cpu.queue_write_ops(size);
        cpu.micro_ops.push(MicroOp::Execute);
        true
    } else {
        // Not equal: load the read value's low `size` bits into Dc.
        cpu.regs.d[dc] = match size {
            motorola_68000::alu::Size::Byte => (compare & 0xFFFF_FF00) | (dest & 0xFF),
            motorola_68000::alu::Size::Word => (compare & 0xFFFF_0000) | (dest & 0xFFFF),
            motorola_68000::alu::Size::Long => dest,
        };
        false
    }
}

/// MULU.L / MULS.L. 32×32 multiply, with 32-bit or 64-bit result
/// depending on bit 10 of the extension word.
fn execute_mull(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let ext = cpu.consume_irc();
    if (opcode >> 3) & 7 == 0 {
        // Register source — compute immediately.
        compute_mull(cpu, ext, cpu.regs.d[(opcode & 7) as usize]);
        return true;
    }
    // Memory source — fetch the operand, then compute at the
    // continuation (TAG_V_MULDIV_MEM_EXEC).
    begin_muldiv_mem_source(cpu, opcode, ext)
}

/// 64-bit MUL.L core. `ext` is the spec word (Dl / Dh / signed / size);
/// `src` is the 32-bit multiplier operand (register or memory).
fn compute_mull(cpu: &mut Cpu68000, ext: u16, src: u32) {
    let dl = ((ext >> 12) & 7) as usize;
    let dh = (ext & 7) as usize;
    let signed = (ext & 0x0800) != 0;
    let wide = (ext & 0x0400) != 0;

    let dl_val = cpu.regs.d[dl];

    let (result_lo, result_hi) = if signed {
        let product = i64::from(src as i32) * i64::from(dl_val as i32);
        (product as u64 as u32, ((product as u64) >> 32) as u32)
    } else {
        let product = u64::from(src) * u64::from(dl_val);
        (product as u32, (product >> 32) as u32)
    };

    let mut sr = cpu.regs.sr & !(N | Z | V | C);

    if wide {
        // 64-bit form: Dh:Dl holds the full product, V is always 0
        // because a 32×32 product fits in 64 bits. Musashi writes
        // Dh *first* then Dl — when `Dl == Dh` the Dl write wins
        // and the low half lands. Matching that write order matters
        // for the ~6% of random fixtures where the corpus picked
        // the same register for both.
        cpu.regs.d[dh] = result_hi;
        cpu.regs.d[dl] = result_lo;
        let zero = result_lo == 0 && result_hi == 0;
        if zero {
            sr |= Z;
        }
        if (result_hi & 0x8000_0000) != 0 {
            sr |= N;
        }
    } else {
        cpu.regs.d[dl] = result_lo;
        // 32-bit form: only Dl is written; V signals that the result
        // didn't fit in 32 bits.
        if result_lo == 0 {
            sr |= Z;
        }
        if (result_lo & 0x8000_0000) != 0 {
            sr |= N;
        }
        let overflow = if signed {
            // Signed: upper 32 bits must equal sign-extension of bit 31.
            let expected = if (result_lo & 0x8000_0000) != 0 {
                0xFFFF_FFFF
            } else {
                0
            };
            result_hi != expected
        } else {
            // Unsigned: upper 32 bits must be zero.
            result_hi != 0
        };
        if overflow {
            sr |= V;
        }
    }

    cpu.regs.sr = sr;
}

/// DIVU.L / DIVS.L. Three forms (M68000PRM § 6.2.7):
///
/// - `Sz=0, Dq=Dr`: 32-bit dividend in Dq, quotient → Dq (no
///   remainder).
/// - `Sz=0, Dq≠Dr`: 32-bit dividend in Dq, quotient → Dq, remainder
///   → Dr (this is the `DIVUL.L` / `DIVSL.L` assembler syntax).
/// - `Sz=1`: 64-bit dividend in Dr:Dq, quotient → Dq, remainder → Dr.
///
/// Divide-by-zero traps vector 5 with the saved PC pointing at the
/// next instruction (the standard 68000 group-1 behaviour). Overflow
/// (quotient doesn't fit in 32 bits) sets V and leaves Dq / Dr
/// untouched.
fn execute_divl(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let ext = cpu.consume_irc();
    if (opcode >> 3) & 7 == 0 {
        // Register source — the instruction is 4 bytes, so the
        // divide-by-zero trap stacks instr_start + 4.
        compute_divl(
            cpu,
            ext,
            cpu.regs.d[(opcode & 7) as usize],
            cpu.instr_start_pc.wrapping_add(4),
        );
        return true;
    }
    begin_muldiv_mem_source(cpu, opcode, ext)
}

/// 64-bit DIV.L core. `ext` is the spec word; `src` the divisor;
/// `next_pc` the address past the whole instruction (stacked on a
/// divide-by-zero trap — it varies with the source mode's extension
/// words, so the caller computes it).
fn compute_divl(cpu: &mut Cpu68000, ext: u16, src: u32, next_pc: u32) {
    let dq = ((ext >> 12) & 7) as usize;
    let dr = (ext & 7) as usize;
    let signed = (ext & 0x0800) != 0;
    let wide = (ext & 0x0400) != 0;

    // Divide-by-zero: stack the address past the whole instruction.
    if src == 0 {
        cpu.begin_group1_exception(5, next_pc);
        return;
    }

    let dq_val = cpu.regs.d[dq];
    let dr_val = cpu.regs.d[dr];

    let dividend = if wide {
        if signed {
            ((dr_val as u64) << 32) | u64::from(dq_val)
        } else {
            ((u64::from(dr_val)) << 32) | u64::from(dq_val)
        }
    } else if signed {
        i64::from(dq_val as i32) as u64
    } else {
        u64::from(dq_val)
    };

    // Overflow only applies to the 64-bit dividend form — 32-bit
    // dividends always fit a 32-bit quotient when the divisor is
    // non-zero. Compute overflow and the unchecked quotient /
    // remainder; Musashi-style.
    let (quotient, remainder, overflow) = if signed {
        let divisor = i64::from(src as i32);
        let dividend_signed = dividend as i64;
        let q = dividend_signed.wrapping_div(divisor);
        let r = dividend_signed.wrapping_rem(divisor);
        let overflow = wide && (q < i64::from(i32::MIN) || q > i64::from(i32::MAX));
        (q as u32, r as u32, overflow)
    } else {
        let divisor = u64::from(src);
        let q = dividend / divisor;
        let r = dividend % divisor;
        let overflow = wide && q > u64::from(u32::MAX);
        (q as u32, r as u32, overflow)
    };

    if overflow {
        // Per Musashi: on overflow set V and return without touching
        // any other flag (N / Z / C / X stay as they were before the
        // instruction). PRM § 6.2.7 says "N and Z undefined, C
        // cleared", but the hardware (and Musashi) preserve all
        // three. The destination registers are also unchanged.
        cpu.regs.sr |= V;
        return;
    }

    // Write remainder to Dr first, then quotient to Dq. If Dq == Dr
    // (the 32-bit "no remainder" form), the second write overwrites
    // and only the quotient lands — same as Musashi.
    cpu.regs.d[dr] = remainder;
    cpu.regs.d[dq] = quotient;

    let mut sr = cpu.regs.sr & !(N | Z | V | C);
    if quotient == 0 {
        sr |= Z;
    }
    if (quotient & 0x8000_0000) != 0 {
        sr |= N;
    }
    cpu.regs.sr = sr;
}

// ─── MOVEC — 68020 control-register extensions ────────────────────
//
// The 68020 adds four control registers reachable via MOVEC, on
// top of the 68010-basic four (SFC / DFC / USP / VBR):
//
//   $002 CACR — Cache Control (mask varies per variant; on 68020
//                bits 0-3 are writable, on 68030 bits 0-12 are,
//                and on 68040 all bits are. Musashi gates the
//                mask on `CPU_TYPE`; we keep the 68020-conservative
//                mask of 0xf because that's what the corpus
//                expects).
//   $802 CAAR — Cache Address Register.
//   $803 MSP  — Master Stack Pointer.
//   $804 ISP  — Interrupt Stack Pointer (when M-flag is clear,
//                this is the active supervisor stack — i.e. SSP).
//
// For unknown CRs we fall through to the 68010-basic four; the
// 68010 hook raises ILLEGAL for everything outside that set.

fn execute_movec_68020_cr_to_rn(cpu: &mut Cpu68000) -> bool {
    if !cpu.regs.is_supervisor() {
        cpu.begin_group1_exception(8, cpu.instr_start_pc);
        return true;
    }
    let ext = cpu.consume_irc();
    let value =
        read_68020_cr(cpu, ext).or_else(|| motorola_68010::cpu::read_control_register(cpu, ext));
    let Some(value) = value else {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    };
    write_movec_reg(cpu, ext, value);
    true
}

fn execute_movec_68020_rn_to_cr(cpu: &mut Cpu68000) -> bool {
    if !cpu.regs.is_supervisor() {
        cpu.begin_group1_exception(8, cpu.instr_start_pc);
        return true;
    }
    let ext = cpu.consume_irc();
    let value = read_movec_reg(cpu, ext);
    if write_68020_cr(cpu, ext, value) {
        return true;
    }
    if motorola_68010::cpu::write_control_register(cpu, ext, value) {
        return true;
    }
    cpu.begin_group1_exception(4, cpu.instr_start_pc);
    true
}

fn read_68020_cr(cpu: &Cpu68000, ext: u16) -> Option<u32> {
    match ext & 0x0FFF {
        0x002 => Some(cpu.regs.cacr),
        0x802 => Some(cpu.regs.caar),
        0x803 => Some(cpu.regs.msp),
        // ISP: when the M-flag is clear (always, for the current
        // corpus), the active SSP *is* the ISP — read it back so
        // round-trip MOVEC writes match.
        0x804 => Some(cpu.regs.ssp),
        _ => None,
    }
}

fn write_68020_cr(cpu: &mut Cpu68000, ext: u16, value: u32) -> bool {
    match ext & 0x0FFF {
        0x002 => {
            // 68020 CACR mask: bits 0-3 (E / F / CE / C).
            //   bit 0 E  — enable instruction cache
            //   bit 1 F  — freeze (serve hits, suppress fills)
            //   bit 2 CE — clear entry (the CAAR-indexed line)
            //   bit 3 C  — clear cache (all entries)
            // Higher variants (68030, 68040) widen this; matching
            // Musashi exactly would mean per-variant masks, but
            // the m68k-test-gen corpus generates random values and
            // the 68020 / 68030 mask differs only in upper bits
            // that the corpus doesn't exercise meaningfully.
            cpu.regs.cacr = value & 0x0f;
            // C and CE are momentary actions that fire on the write.
            // We keep them in the stored value (the corpus round-trips
            // `value & 0x0f`) but act on them here. CE selects the line
            // by CAAR.
            let caar = cpu.regs.caar;
            if let Some(cache) = cpu.variant_icache.as_mut() {
                if value & 0x08 != 0 {
                    cache.clear();
                }
                if value & 0x04 != 0 {
                    cache.clear_entry(caar);
                }
            }
            true
        }
        0x802 => {
            cpu.regs.caar = value;
            true
        }
        0x803 => {
            cpu.regs.msp = value;
            true
        }
        0x804 => {
            // ISP write with M=0 lands on the active SSP. M=1
            // routing is deferred (no fixture exercises it).
            cpu.regs.ssp = value;
            true
        }
        _ => false,
    }
}

/// Decode the data/address register field of a MOVEC extension
/// word and read the named GP register. Mirrors the 68010 helper.
fn read_movec_reg(cpu: &Cpu68000, ext: u16) -> u32 {
    let reg = ((ext >> 12) & 7) as usize;
    let is_address = (ext & 0x8000) != 0;
    if is_address {
        cpu.regs.a(reg)
    } else {
        cpu.regs.d[reg]
    }
}

/// Decode the data/address register field of a MOVEC extension
/// word and write the named GP register. Mirrors the 68010 helper.
fn write_movec_reg(cpu: &mut Cpu68000, ext: u16, value: u32) {
    let reg = ((ext >> 12) & 7) as usize;
    let is_address = (ext & 0x8000) != 0;
    if is_address {
        cpu.regs.set_a(reg, value);
    } else {
        cpu.regs.d[reg] = value;
    }
}

// ─── Bit-field family ──────────────────────────────────────────────
//
// Extension word format (M68000PRM § 6.2.2-6.2.4):
//
//   bit 15:  0 (reserved)
//   bits 14-12: destination / source register (BFEXTU / BFEXTS /
//             BFFFO / BFINS); ignored by BFTST / BFCHG / BFCLR /
//             BFSET.
//   bit 11:  Do — 0 = offset is the 5-bit immediate in bits 10-6,
//             1 = offset is the full 32-bit signed value in
//             D[bits 8-6].
//   bits 10-6: offset (immediate 0-31, or Dn number when Do = 1).
//   bit 5:   Dw — 0 = width is the 5-bit immediate in bits 4-0,
//             1 = width is in D[bits 2-0].
//   bits 4-0: width (encoded 0 = 32, 1-31 = 1-31; same encoding
//             modulo 32 for the Dn case).
//
// For a Dn destination/source the offset wraps modulo 32 and the
// field wraps around bit 0. Musashi's implementation uses a
// position-mask in the original register (built by rotating
// `0xFFFFFFFF << (32 - width)` right by offset) for in-place
// modification, and rotate-left + shift-right for extraction.

/// Hook entry: dispatch the 8 BF opcodes by their sub-op field.
/// Dn-destination runs synchronously here; memory operands kick off
/// the multi-step memory pipeline via [`begin_bf_memory`].
fn execute_bf(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let ea_mode = (opcode >> 3) & 7;
    let ea_reg = (opcode & 7) as usize;
    if ea_mode != 0 {
        return begin_bf_memory(cpu, opcode, ea_mode as u8, ea_reg as u8);
    }

    let op = (opcode >> 8) & 7;
    let ext = cpu.consume_irc();
    let dr = ((ext >> 12) & 7) as usize;

    // Decode offset (immediate or Dn). Dn is signed → use rem_euclid
    // so negative offsets wrap correctly to 0..=31.
    let offset = if ext & 0x0800 != 0 {
        let reg = ((ext >> 6) & 7) as usize;
        (cpu.regs.d[reg] as i32).rem_euclid(32) as u32
    } else {
        u32::from((ext >> 6) & 0x1F)
    };

    // Width: ((raw - 1) & 31) + 1 maps 0→32, 1→1, …, 31→31, and for
    // Dn-width handles arbitrary 32-bit values via the same trick.
    let raw_width = if ext & 0x0020 != 0 {
        let reg = (ext & 7) as usize;
        cpu.regs.d[reg]
    } else {
        u32::from(ext & 0x1F)
    };
    let width = (raw_width.wrapping_sub(1) & 31) + 1;

    // Field-position mask in the source register's bit layout.
    let mask_base: u32 = if width == 32 {
        0xFFFF_FFFF
    } else {
        0xFFFF_FFFFu32 << (32 - width)
    };
    let mask = mask_base.rotate_right(offset);

    let dn_val = cpu.regs.d[ea_reg];

    // The pre-modification field flags (used by BFTST / BFCHG /
    // BFCLR / BFSET). N = bit 31 of (Dn shifted left by offset) =
    // MSB of the field. Z = field-bits are all zero in Dn.
    let n_set_pre = (dn_val.wrapping_shl(offset) & 0x8000_0000) != 0;
    let z_set_pre = (dn_val & mask) == 0;

    let mut sr = cpu.regs.sr & !(N | Z | V | C);

    match op {
        0 => {
            // BFTST: flags only.
            if n_set_pre {
                sr |= N;
            }
            if z_set_pre {
                sr |= Z;
            }
        }
        1 => {
            // BFEXTU: rotate Dn left so field sits at the top, then
            // right-shift by (32-width) to right-align. Write to Dr.
            let rotated = dn_val.rotate_left(offset);
            let result = if width == 32 {
                rotated
            } else {
                rotated >> (32 - width)
            };
            cpu.regs.d[dr] = result;
            if n_set_pre {
                sr |= N;
            }
            if result == 0 {
                sr |= Z;
            }
        }
        2 => {
            // BFCHG: toggle the field bits in Dn.
            cpu.regs.d[ea_reg] = dn_val ^ mask;
            if n_set_pre {
                sr |= N;
            }
            if z_set_pre {
                sr |= Z;
            }
        }
        3 => {
            // BFEXTS: same as BFEXTU but arithmetic-shift the rotated
            // value so the MSB of the field sign-extends through the
            // upper bits.
            let rotated = dn_val.rotate_left(offset);
            let result = if width == 32 {
                rotated
            } else {
                ((rotated as i32) >> (32 - width)) as u32
            };
            cpu.regs.d[dr] = result;
            if n_set_pre {
                sr |= N;
            }
            if result == 0 {
                sr |= Z;
            }
        }
        4 => {
            // BFCLR: clear the field bits.
            cpu.regs.d[ea_reg] = dn_val & !mask;
            if n_set_pre {
                sr |= N;
            }
            if z_set_pre {
                sr |= Z;
            }
        }
        5 => {
            // BFFFO: find first one bit MSB-first within the field.
            // Result Dr = offset + (position of first '1'), or
            // offset + width if no bit is set.
            let rotated = dn_val.rotate_left(offset);
            let field = if width == 32 {
                rotated
            } else {
                rotated >> (32 - width)
            };
            let mut bit_idx = 0u32;
            let mut bit_mask = 1u32 << (width - 1);
            while bit_mask != 0 && (field & bit_mask) == 0 {
                bit_idx += 1;
                bit_mask >>= 1;
            }
            cpu.regs.d[dr] = offset + bit_idx;
            if n_set_pre {
                sr |= N;
            }
            if field == 0 {
                sr |= Z;
            }
        }
        6 => {
            // BFSET: set the field bits.
            cpu.regs.d[ea_reg] = dn_val | mask;
            if n_set_pre {
                sr |= N;
            }
            if z_set_pre {
                sr |= Z;
            }
        }
        7 => {
            // BFINS: write Dr (truncated/positioned to width) into
            // the field. Flags come from the source register's
            // width-bit value (after shifting up so its MSB sits at
            // bit 31) — N = MSB of shifted source, Z = source==0.
            let insert_value = cpu.regs.d[dr];
            let insert_shifted = if width == 32 {
                insert_value
            } else {
                insert_value.wrapping_shl(32 - width)
            };
            let n_ins = (insert_shifted & 0x8000_0000) != 0;
            let z_ins = insert_shifted == 0;
            // Place the shifted source at the field's location by
            // rotating right by offset, then merge with Dn.
            let insert_placed = insert_shifted.rotate_right(offset);
            cpu.regs.d[ea_reg] = (dn_val & !mask) | (insert_placed & mask);
            if n_ins {
                sr |= N;
            }
            if z_ins {
                sr |= Z;
            }
        }
        _ => unreachable!("BF sub-op masked to 3 bits"),
    }

    cpu.regs.sr = sr;
    true
}

// ─── Bit-field memory pipeline (Stage M / Phase 5) ─────────────────
//
// `execute_bf` dispatches memory operands here. We decode the BF
// extension word, resolve the EA (only the no-extension-word modes
// — `(An)`, `(An)+`, `-(An)` — synchronously for now; the rest
// trap until `TAG_BF_MEM_EA_RESOLVE` is implemented), stash the
// pipeline state on the CPU, and queue the first `ReadByte`. The
// `TAG_BF_MEM_READ` continuation in `motorola-68000::decode` chains
// the remaining byte reads and hands off to `TAG_BF_MEM_EXEC` /
// `TAG_BF_MEM_WRITE` for the field math and any writeback.

use motorola_68000::cpu::{
    TAG_BF_MEM_EA_ABSLONG_LO, TAG_BF_MEM_EA_RESOLVE, TAG_BF_MEM_EXEC, TAG_BF_MEM_READ,
    TAG_BF_MEM_WRITE, TAG_FETCH_SRC_DATA, TAG_V_CAS_COMPARE, TAG_V_CAS_WRITE_DONE,
    TAG_V_CAS2_COMPUTE, TAG_V_CAS2_GATHER, TAG_V_CAS2_READ2, TAG_V_CAS2_WRITE_DONE,
    TAG_V_CAS2_WRITE2, TAG_V_CHK2_LOWER, TAG_V_CHK2_UPPER, TAG_V_FBCC_L, TAG_V_FDBCC,
    TAG_V_FMOVEM_STEP, TAG_V_FP_IMM_READ, TAG_V_FP_MEM_EXEC, TAG_V_FP_MEM_READ, TAG_V_FP_MEM_WRITE,
    TAG_V_FSAVE_WRITE, TAG_V_MULDIV_MEM_EXEC,
};
use motorola_68000::microcode::MicroOp;
use motorola_68010::continue_68010_opcode;

/// Start the memory-EA bit-field pipeline. Returns `true` once the
/// pipeline is in flight or the instruction has been replaced by a
/// group-1 exception (illegal instruction for unsupported modes).
fn begin_bf_memory(cpu: &mut Cpu68000, opcode: u16, ea_mode: u8, ea_reg: u8) -> bool {
    let sub_op = ((opcode >> 8) & 7) as u8;
    let ext = cpu.consume_irc();
    let dr = ((ext >> 12) & 7) as u8;

    // Offset: bit 11 selects Dn-source (signed 32-bit) vs 5-bit
    // immediate. Memory mode treats the offset as a signed bit
    // displacement from the base byte address — it can be arbitrarily
    // large in either direction.
    let offset_raw: i32 = if ext & 0x0800 != 0 {
        let reg = ((ext >> 6) & 7) as usize;
        cpu.regs.d[reg] as i32
    } else {
        i32::from((ext >> 6) & 0x1F)
    };

    // Width: bit 5 selects Dn-source vs 5-bit immediate. Both encode
    // 0 → 32, otherwise 1..=31 maps to itself (and Dn's full 32 bits
    // are reduced mod 32 via the same `(raw - 1) & 31 + 1` trick).
    let raw_width: u32 = if ext & 0x0020 != 0 {
        let reg = (ext & 7) as usize;
        cpu.regs.d[reg]
    } else {
        u32::from(ext & 0x1F)
    };
    let width = (raw_width.wrapping_sub(1) & 31) + 1;

    // Decompose offset into a signed byte displacement and a 0..=7
    // bit offset within the first byte (MSB-numbered, so bit 0 is
    // the byte's MSB). Rust's arithmetic `>> 3` is floor for signed
    // i32; `& 7` then takes the positive-mod-8 remainder thanks to
    // two's-complement.
    let byte_disp: i32 = offset_raw >> 3;
    let bit_offset: u8 = (offset_raw & 7) as u8;
    let bytes_total: u8 = (u32::from(bit_offset) + width).div_ceil(8) as u8;

    // Common pipeline state. Stash the per-instruction params so
    // both the instant-EA path below and the deferred resolve handler
    // can act on them.
    cpu.bf_buf = 0;
    cpu.bf_sub_op = sub_op;
    cpu.bf_dr = dr;
    cpu.bf_width = width as u8;
    cpu.bf_bit_offset = bit_offset;
    cpu.bf_bytes_total = bytes_total;
    cpu.bf_bytes_done = 0;
    cpu.bf_byte_disp = byte_disp;
    cpu.bf_ea_mode = ea_mode;
    cpu.bf_ea_reg = ea_reg;
    cpu.bf_source_val = match sub_op {
        7 => cpu.regs.d[dr as usize], // BFINS: snapshot Dr before any writes.
        5 => offset_raw as u32,       // BFFFO: stash the full signed offset
        //         so the result (offset + first-one
        //         position) sees its original
        //         32-bit width, not the wrapped
        //         5-bit value.
        _ => 0,
    };

    // Resolve EA. Instant modes ((An), (An)+, -(An)) compute the
    // base byte address synchronously and kick the read chain off
    // right here. Modes that need extension words defer to
    // `TAG_BF_MEM_EA_RESOLVE`, which runs after the queued FetchIRC
    // has refilled IRC with the EA's first extension word.
    cpu.in_followup = true;
    match ea_mode {
        // (An): plain indirect.
        2 => start_bf_read(cpu, cpu.regs.a(ea_reg as usize)),
        // (An)+: byte-step post-increment per M68000PRM § 4.3.5.
        3 => {
            let a = cpu.regs.a(ea_reg as usize);
            cpu.regs.set_a(ea_reg as usize, a.wrapping_add(1));
            start_bf_read(cpu, a);
        }
        // -(An): byte-step pre-decrement per M68000PRM § 4.3.5.
        4 => {
            let a = cpu.regs.a(ea_reg as usize).wrapping_sub(1);
            cpu.regs.set_a(ea_reg as usize, a);
            start_bf_read(cpu, a);
        }
        // d16(An), (d8,An,Xn), AbsShort/Long, PcDisp, PcIndex —
        // defer EA resolution until the FetchIRC has refilled IRC.
        5..=7 => {
            cpu.followup_tag = TAG_BF_MEM_EA_RESOLVE;
            cpu.micro_ops.push(MicroOp::Execute);
        }
        // Modes 0 (Dn) and 1 (An direct) shouldn't reach here:
        // `execute_bf` routes Dn elsewhere, and direct-An isn't a
        // valid memory EA. Treat anything that does as illegal.
        _ => {
            cpu.begin_group1_exception(4, cpu.instr_start_pc);
        }
    }
    true
}

/// Finalise the BF base-byte address and kick off the byte-read
/// chain. Shared by `begin_bf_memory`'s instant-EA modes and
/// `TAG_BF_MEM_EA_RESOLVE` after extension words land. The caller
/// is responsible for everything *except* applying the BF byte
/// displacement, queueing the first `ReadByte`, and setting
/// `TAG_BF_MEM_READ`.
fn start_bf_read(cpu: &mut Cpu68000, ea_addr: u32) {
    let base_byte = ea_addr.wrapping_add_signed(cpu.bf_byte_disp);
    cpu.bf_base_addr = base_byte;
    cpu.addr = base_byte;
    cpu.followup_tag = TAG_BF_MEM_READ;
    cpu.micro_ops.push(MicroOp::ReadByte);
    cpu.micro_ops.push(MicroOp::Execute);
}

/// 68020 continuation hook. Dispatches the BF memory pipeline tags
/// and chains to the 68010 hook for any tag the 68020 doesn't claim
/// (notably `TAG_RTD_*`). Installed via `variant_continue_hook`.
pub fn continue_68020_opcode(cpu: &mut Cpu68000) -> bool {
    match cpu.followup_tag {
        TAG_BF_MEM_EA_RESOLVE => {
            handle_bf_mem_ea_resolve(cpu);
            true
        }
        TAG_BF_MEM_EA_ABSLONG_LO => {
            handle_bf_mem_ea_abslong_lo(cpu);
            true
        }
        TAG_BF_MEM_READ => {
            handle_bf_mem_read(cpu);
            true
        }
        TAG_BF_MEM_EXEC => {
            handle_bf_mem_exec(cpu);
            true
        }
        TAG_BF_MEM_WRITE => {
            handle_bf_mem_write(cpu);
            true
        }
        TAG_V_FP_MEM_READ => {
            handle_fp_mem_read(cpu);
            true
        }
        TAG_V_FP_IMM_READ => {
            handle_fp_imm_read(cpu);
            true
        }
        TAG_V_FP_MEM_EXEC => {
            handle_fp_mem_exec(cpu);
            true
        }
        TAG_V_FP_MEM_WRITE => {
            handle_fp_mem_write(cpu);
            true
        }
        TAG_V_FMOVEM_STEP => {
            handle_fmovem_step(cpu);
            true
        }
        TAG_V_FBCC_L => {
            handle_fbcc_l(cpu);
            true
        }
        TAG_V_FDBCC => {
            handle_fdbcc(cpu);
            true
        }
        TAG_V_FSAVE_WRITE => {
            handle_fsave_write(cpu);
            true
        }
        // An FPU memory operand using a static addressing mode: the core's
        // EA machinery has resolved the address into `addr`. Take over and
        // read/write the operand bytes ourselves (the core's data fetch
        // only knows B/W/L, but we need 1/2/4/8/12).
        TAG_FETCH_SRC_DATA if cpu.fp_mem_pending => {
            cpu.fp_mem_pending = false;
            if cpu.fp_mem_store {
                cpu.fp_mem_store = false;
                start_fp_write(cpu);
            } else {
                start_fp_read(cpu);
            }
            true
        }
        // FSAVE with a control addressing mode: the core's EA machinery has
        // resolved the frame address into `addr`. Start the frame write.
        TAG_FETCH_SRC_DATA if cpu.fp_frame_pending => {
            cpu.fp_frame_pending = false;
            cpu.fp_frame_store = false;
            start_fsave_write(cpu);
            true
        }
        // Memory-source MUL.L / DIV.L: the core's EA pipeline has
        // resolved the source EA and reached TAG_FETCH_SRC_DATA. Only
        // MUL.L / DIV.L ($4C00 / $4C40) reach this tag via the variant
        // path (register-source forms compute synchronously at decode),
        // so the opcode guard uniquely identifies the case. Queue the
        // long operand read, then finish at TAG_V_MULDIV_MEM_EXEC.
        TAG_FETCH_SRC_DATA if (cpu.ir & 0xFF80) == 0x4C00 => {
            cpu.followup_tag = TAG_V_MULDIV_MEM_EXEC;
            cpu.queue_read_ops(motorola_68000::alu::Size::Long);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        // CHK2 / CMP2: the core's EA pipeline has resolved the
        // bounds-tuple base (`cpu.addr`) and reached TAG_FETCH_SRC_DATA.
        // Only CHK2/CMP2 ($00C0-style, bit 11 = 0) reach this tag with
        // this opcode shape. Read the lower bound, chain to the upper.
        TAG_FETCH_SRC_DATA if (cpu.ir & 0xF9C0) == 0x00C0 => {
            cpu.followup_tag = TAG_V_CHK2_LOWER;
            cpu.queue_read_ops(cpu.size);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        TAG_V_CHK2_LOWER => {
            // Lower bound is in `cpu.data`; stash it, then read the
            // upper bound at EA + size.
            cpu.src_val = cpu.data;
            cpu.addr = cpu.addr.wrapping_add(cpu.size.bytes());
            cpu.followup_tag = TAG_V_CHK2_UPPER;
            cpu.queue_read_ops(cpu.size);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        TAG_V_CHK2_UPPER => {
            compute_chk2_cmp2(cpu);
            // End the instruction — unless CHK2 raised a vector-6 trap,
            // in which case `begin_group1_exception` owns in_followup.
            if cpu.exc_vector.is_none() {
                cpu.in_followup = false;
            }
            true
        }
        // CAS: the EA pipeline has resolved the destination address and
        // reached TAG_FETCH_SRC_DATA. CAS opcodes are bit 11 = 1 of the
        // size-3 immediate group ($08C0 mask); the guard separates them
        // from CHK2/CMP2 (bit 11 = 0). Read the destination operand, then
        // compare-and-swap at TAG_V_CAS_COMPARE.
        TAG_FETCH_SRC_DATA if (cpu.ir & 0xF9C0) == 0x08C0 => {
            cpu.followup_tag = TAG_V_CAS_COMPARE;
            cpu.queue_read_ops(cpu.size);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        TAG_V_CAS_COMPARE => {
            // `finish_cas` returns true when it queued the Du write-back
            // (instruction continues to TAG_V_CAS_WRITE_DONE); false when
            // the compare failed and Dc was loaded (instruction ends).
            if !finish_cas(cpu) {
                cpu.in_followup = false;
            }
            true
        }
        TAG_V_CAS_WRITE_DONE => {
            cpu.in_followup = false;
            cpu.followup_tag = 0;
            true
        }
        // CAS2: extension word 2 is now in `irc`. Complete the 32-bit
        // spec, advance the prefetch past it, then read the first
        // destination at [Rn1].
        TAG_V_CAS2_GATHER => {
            cpu.src_val |= u32::from(cpu.irc);
            cpu.micro_ops.push(MicroOp::FetchIRC);
            let word2 = cpu.src_val;
            cpu.addr = reg_da(cpu, word2 >> 28);
            cpu.followup_tag = TAG_V_CAS2_READ2;
            cpu.queue_read_ops(cpu.size);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        TAG_V_CAS2_READ2 => {
            // dest1 read; stash it and read the second destination at
            // [Rn2].
            cpu.dst_val = cpu.data;
            let word2 = cpu.src_val;
            cpu.addr = reg_da(cpu, word2 >> 12);
            cpu.followup_tag = TAG_V_CAS2_COMPUTE;
            cpu.queue_read_ops(cpu.size);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        TAG_V_CAS2_COMPUTE => {
            let word2 = cpu.src_val;
            let dest1 = cpu.dst_val;
            let dest2 = cpu.data;
            if finish_cas2(cpu, word2, dest1, dest2) {
                // Both matched: write Du1 to [Rn1], then Du2 to [Rn2].
                let du1 = ((word2 >> 22) & 7) as usize;
                cpu.addr = reg_da(cpu, word2 >> 28);
                cpu.data = cpu.regs.d[du1];
                cpu.followup_tag = TAG_V_CAS2_WRITE2;
                cpu.queue_write_ops(cpu.size);
                cpu.micro_ops.push(MicroOp::Execute);
            } else {
                // Mismatch: compare registers already loaded; end.
                cpu.in_followup = false;
            }
            true
        }
        TAG_V_CAS2_WRITE2 => {
            let word2 = cpu.src_val;
            let du2 = ((word2 >> 6) & 7) as usize;
            cpu.addr = reg_da(cpu, word2 >> 12);
            cpu.data = cpu.regs.d[du2];
            cpu.followup_tag = TAG_V_CAS2_WRITE_DONE;
            cpu.queue_write_ops(cpu.size);
            cpu.micro_ops.push(MicroOp::Execute);
            true
        }
        TAG_V_CAS2_WRITE_DONE => {
            cpu.in_followup = false;
            cpu.followup_tag = 0;
            true
        }
        TAG_V_MULDIV_MEM_EXEC => {
            let src = cpu.data;
            let ext = cpu.variant_ext_word;
            if (cpu.ir & 0xFFC0) == 0x4C00 {
                compute_mull(cpu, ext, src);
            } else {
                // DIV.L: the instruction length depends on the source
                // mode's extension words; next_pc = opcode + spec word +
                // EA extension words.
                let ea_ext = cpu.src_mode.map_or(0, |m| u32::from(m.ext_word_count()));
                let next_pc = cpu.instr_start_pc.wrapping_add(4 + ea_ext * 2);
                compute_divl(cpu, ext, src, next_pc);
            }
            // End the instruction — unless a divide-by-zero trap was
            // raised, in which case `begin_group1_exception` owns
            // in_followup and the queued exception sequence.
            if cpu.exc_vector.is_none() {
                cpu.in_followup = false;
            }
            true
        }
        _ => continue_68010_opcode(cpu),
    }
}

/// BF memory EA resolve. Runs after the FetchIRC queued during
/// `begin_bf_memory`'s `consume_irc` has refilled IRC with the
/// EA's first extension word. Decodes the EA using the stashed
/// `bf_ea_mode` / `bf_ea_reg` (`(d16,An)`, `(d8,An,Xn)`,
/// AbsShort, PcDisp, PcIndex complete here; AbsLong stages its
/// high word and hands off to `TAG_BF_MEM_EA_ABSLONG_LO`).
fn handle_bf_mem_ea_resolve(cpu: &mut Cpu68000) {
    let ea_mode = cpu.bf_ea_mode;
    let ea_reg = cpu.bf_ea_reg;

    match ea_mode {
        // d16(An): sign-extended 16-bit displacement added to An.
        5 => {
            let disp = i32::from(cpu.consume_irc() as i16) as u32;
            let ea = cpu.regs.a(ea_reg as usize).wrapping_add(disp);
            start_bf_read(cpu, ea);
        }
        // (d8,An,Xn): brief extension word — base + sign-extended
        // d8 + (D/A indexed Xn × scale).
        6 => {
            let ext = cpu.consume_irc();
            let ea = compute_brief_index_ea(cpu, ext, cpu.regs.a(ea_reg as usize));
            start_bf_read(cpu, ea);
        }
        // Mode 7: sub-mode selected by ea_reg.
        7 => match ea_reg {
            // AbsShort: sign-extended 16-bit absolute address.
            0 => {
                let ea = i32::from(cpu.consume_irc() as i16) as u32;
                start_bf_read(cpu, ea);
            }
            // AbsLong: stash the high word and chain to ABSLONG_LO.
            1 => {
                cpu.bf_base_addr = u32::from(cpu.consume_irc()) << 16;
                cpu.followup_tag = TAG_BF_MEM_EA_ABSLONG_LO;
                cpu.micro_ops.push(MicroOp::Execute);
            }
            // PcDisp: PC at the extension word + sign-extended d16.
            // `irc_addr` is the address the extension word was
            // fetched from — exactly the PC value the EA spec
            // references.
            2 => {
                let pc_ext = cpu.irc_addr;
                let disp = i32::from(cpu.consume_irc() as i16) as u32;
                let ea = pc_ext.wrapping_add(disp);
                start_bf_read(cpu, ea);
            }
            // PcIndex: brief extension word with PC-at-ext as base.
            3 => {
                let pc_ext = cpu.irc_addr;
                let ext = cpu.consume_irc();
                let ea = compute_brief_index_ea(cpu, ext, pc_ext);
                start_bf_read(cpu, ea);
            }
            // Sub-modes 4 (immediate) and 5..=7 are reserved /
            // illegal for bit-field memory ops. Treat as illegal.
            _ => {
                cpu.begin_group1_exception(4, cpu.instr_start_pc);
            }
        },
        _ => {
            // Shouldn't be reachable — begin_bf_memory only routes
            // modes 5/6/7 here.
            cpu.begin_group1_exception(4, cpu.instr_start_pc);
        }
    }
}

/// AbsLong second extension word. The high word is in
/// `bf_base_addr` (set by `handle_bf_mem_ea_resolve`); OR in the
/// just-fetched low word and start the read chain.
fn handle_bf_mem_ea_abslong_lo(cpu: &mut Cpu68000) {
    let lo = u32::from(cpu.consume_irc());
    let ea = cpu.bf_base_addr | lo;
    start_bf_read(cpu, ea);
}

/// Compute an EA from a brief-extension-word indexed mode
/// (`(d8,An,Xn)` or `(d8,PC,Xn)`). The base is whatever the caller
/// passes (An value or PC-at-extension). 68020 honours the scale
/// factor (bits 10-9) when `variant_scaled_index` is set; 68000 /
/// 68010 treat those bits as "don't care" and always use ×1.
///
/// Mirrors the corresponding arm of `Cpu68000::calc_ea_start` —
/// kept local here so the BF resolve handler can compute the EA
/// inline without dragging in the calc_ea_start state machine.
fn compute_brief_index_ea(cpu: &Cpu68000, ext: u16, base: u32) -> u32 {
    let disp = (ext & 0xFF) as i8 as i32;
    let idx_reg = ((ext >> 12) & 7) as usize;
    let idx_val = if ext & 0x8000 != 0 {
        cpu.regs.a(idx_reg)
    } else {
        cpu.regs.d[idx_reg]
    };
    let idx = if ext & 0x0800 != 0 {
        idx_val
    } else {
        i32::from(idx_val as i16) as u32
    };
    let scale = if cpu.variant_scaled_index {
        1u32 << ((ext >> 9) & 0x3)
    } else {
        1
    };
    base.wrapping_add(disp as u32)
        .wrapping_add(idx.wrapping_mul(scale))
}

/// BF read-chain step. One `ReadByte` has just completed and the
/// byte sits in the low 8 bits of `self.data`. Pack it MSB-first
/// into `bf_buf` (byte 0 → bits 63-56, byte 1 → 55-48, …) so that
/// the field-extraction math in `TAG_BF_MEM_EXEC` works against a
/// uniform MSB-aligned buffer.
fn handle_bf_mem_read(cpu: &mut Cpu68000) {
    let byte = u64::from(cpu.data & 0xFF);
    let shift = 56 - 8 * u32::from(cpu.bf_bytes_done);
    cpu.bf_buf |= byte << shift;
    cpu.bf_bytes_done += 1;
    if cpu.bf_bytes_done < cpu.bf_bytes_total {
        cpu.addr = cpu.addr.wrapping_add(1);
        cpu.micro_ops.push(MicroOp::ReadByte);
        cpu.micro_ops.push(MicroOp::Execute);
    } else {
        cpu.followup_tag = TAG_BF_MEM_EXEC;
        cpu.micro_ops.push(MicroOp::Execute);
    }
}

/// BF field math. The MSB-aligned `bf_buf` is now fully populated;
/// extract the `width`-bit field that starts at `bit_offset` (bit
/// position counted from the MSB end of byte 0). Set N/Z from the
/// pre-modification field for every op except BFINS, then dispatch:
///
///   - BFTST → flags only; finish.
///   - BFEXTU → zero-extend into Dr; finish.
///   - BFEXTS → sign-extend into Dr; finish.
///   - BFFFO  → Dr = (original signed offset) + (bit position of
///     the first '1' MSB-first, or width if none); finish.
///   - BFCHG / BFCLR / BFSET / BFINS → modify the field bits in
///     `bf_buf` and hand off to `TAG_BF_MEM_WRITE` for the
///     R-M-W byte chain.
fn handle_bf_mem_exec(cpu: &mut Cpu68000) {
    let width = u32::from(cpu.bf_width);
    let bit_offset = u32::from(cpu.bf_bit_offset);
    let field_shift = 64 - bit_offset - width;
    // Width 1..=32, so `(1u64 << width) - 1` is well-defined for the
    // full range — `1u64 << 32 = 0x1_0000_0000` fits in u64.
    let mask_u64: u64 = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    let mask_u32 = mask_u64 as u32;

    let field_u32 = ((cpu.bf_buf >> field_shift) & mask_u64) as u32;
    let field_msb = (field_u32 >> (width - 1)) & 1 != 0;
    let field_zero = field_u32 == 0;

    let mut sr = cpu.regs.sr & !(N | Z | V | C);
    if field_msb {
        sr |= N;
    }
    if field_zero {
        sr |= Z;
    }

    let sub_op = cpu.bf_sub_op;
    let dr = cpu.bf_dr as usize;

    match sub_op {
        // BFTST — flags only.
        0 => {
            cpu.regs.sr = sr;
            cpu.in_followup = false;
            cpu.followup_tag = 0;
        }
        // BFEXTU — zero-extended field into Dr.
        1 => {
            cpu.regs.d[dr] = field_u32;
            cpu.regs.sr = sr;
            cpu.in_followup = false;
            cpu.followup_tag = 0;
        }
        // BFEXTS — sign-extended field into Dr.
        3 => {
            let signed = if width == 32 {
                field_u32
            } else {
                let extend = 32 - width;
                (((field_u32 << extend) as i32) >> extend) as u32
            };
            cpu.regs.d[dr] = signed;
            cpu.regs.sr = sr;
            cpu.in_followup = false;
            cpu.followup_tag = 0;
        }
        // BFFFO — scan for first '1' MSB-first within the field.
        5 => {
            let mut position = 0u32;
            let mut m = 1u32 << (width - 1);
            while m != 0 && (field_u32 & m) == 0 {
                position += 1;
                m >>= 1;
            }
            // bf_source_val holds the original signed offset
            // (stashed in `begin_bf_memory`). Result = offset + N
            // (or offset + width when no '1' bit is set).
            cpu.regs.d[dr] = cpu.bf_source_val.wrapping_add(position);
            cpu.regs.sr = sr;
            cpu.in_followup = false;
            cpu.followup_tag = 0;
        }
        // BFCHG / BFCLR / BFSET / BFINS — read-modify-write.
        2 | 4 | 6 | 7 => {
            let mask_in_buf = mask_u64 << field_shift;
            cpu.bf_buf = match sub_op {
                2 => cpu.bf_buf ^ mask_in_buf,
                4 => cpu.bf_buf & !mask_in_buf,
                6 => cpu.bf_buf | mask_in_buf,
                7 => {
                    let insert = u64::from(cpu.bf_source_val & mask_u32);
                    (cpu.bf_buf & !mask_in_buf) | (insert << field_shift)
                }
                _ => unreachable!(),
            };
            // BFINS gets flags from the source operand, not the
            // pre-modification field. PRM § 4.3.4.
            if sub_op == 7 {
                let src = cpu.bf_source_val & mask_u32;
                let src_msb = (src >> (width - 1)) & 1 != 0;
                let src_zero = src == 0;
                sr = cpu.regs.sr & !(N | Z | V | C);
                if src_msb {
                    sr |= N;
                }
                if src_zero {
                    sr |= Z;
                }
            }
            cpu.regs.sr = sr;
            // Set up the writeback chain. Byte 0 sits at bits 63-56
            // of bf_buf; subsequent bytes step right by 8 bits.
            cpu.bf_bytes_done = 0;
            cpu.addr = cpu.bf_base_addr;
            cpu.data = ((cpu.bf_buf >> 56) & 0xFF) as u32;
            cpu.followup_tag = TAG_BF_MEM_WRITE;
            cpu.micro_ops.push(MicroOp::WriteByte);
            cpu.micro_ops.push(MicroOp::Execute);
        }
        _ => unreachable!("BF sub-op must be 0..=7"),
    }
}

/// BF write-chain step. A `WriteByte` just completed; pick up the
/// next byte from `bf_buf` and queue another `WriteByte`, or finish
/// the instruction when the field's span is fully written.
fn handle_bf_mem_write(cpu: &mut Cpu68000) {
    cpu.bf_bytes_done += 1;
    if cpu.bf_bytes_done < cpu.bf_bytes_total {
        cpu.addr = cpu.addr.wrapping_add(1);
        let shift = 56 - 8 * u32::from(cpu.bf_bytes_done);
        cpu.data = ((cpu.bf_buf >> shift) & 0xFF) as u32;
        cpu.micro_ops.push(MicroOp::WriteByte);
        cpu.micro_ops.push(MicroOp::Execute);
    } else {
        cpu.in_followup = false;
        cpu.followup_tag = 0;
        // Empty queue → tick's "start next instruction" path will
        // auto-push `PromoteIRC` on the following tick.
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu68020;

    #[test]
    fn deserialize_restores_variant_hooks() {
        let mut cpu = Cpu68020::new();
        cpu.regs.pc = 0xDEAD_BEEF;
        cpu.regs.d[0] = 0x1234_5678;

        let bytes = postcard::to_allocvec(&cpu).expect("serialize");
        let restored: Cpu68020 = postcard::from_bytes(&bytes).expect("deserialize");

        // State preserved.
        assert_eq!(restored.regs.pc, 0xDEAD_BEEF);
        assert_eq!(restored.regs.d[0], 0x1234_5678);

        // Variant hooks reinstalled (Cpu68000's #[serde(skip)] would
        // otherwise default these to None / false).
        assert!(restored.variant_decode_hook.is_some());
        assert!(restored.variant_continue_hook.is_some());
        assert!(restored.variant_scaled_index);
        assert!(restored.variant_extended_sr_writes);
        assert!(restored.variant_format2_vectors);
        assert!(restored.variant_six_word_frame);
        assert!(restored.variant_musashi_bcd_v);
        assert!(restored.variant_musashi_div_overflow);
        assert!(restored.variant_long_branch);
    }

    #[test]
    fn clone_preserves_variant_hooks() {
        let cpu = Cpu68020::new();
        let cloned = cpu.clone();
        assert!(cloned.variant_decode_hook.is_some());
        assert!(cloned.variant_continue_hook.is_some());
        assert!(cloned.variant_scaled_index);
        assert!(cloned.variant_extended_sr_writes);
        assert!(cloned.variant_format2_vectors);
    }

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
