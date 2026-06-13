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
    TAG_BF_MEM_WRITE, TAG_FETCH_SRC_DATA, TAG_V_MULDIV_MEM_EXEC,
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
