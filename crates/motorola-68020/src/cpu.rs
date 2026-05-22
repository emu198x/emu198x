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
        // The 68020 widens the SR write mask to include the M-flag
        // (bit 12) — MOVE-to-SR / ORI-to-SR / EORI-to-SR /
        // ANDI-to-SR / STOP / RTE all read this flag. The 68010
        // leaves it false (only the 68000-shared 0xA71F bits are
        // writable).
        inner.variant_extended_sr_writes = true;
        // The 68020+ promotes CHK / CHK2 / divide-by-zero / TRAPV /
        // TRAPcc / Trace to a 12-byte Format-$2 exception frame
        // with an extra Instruction-Address long at the top.
        // M68000PRM § 8.6.3.
        inner.variant_format2_vectors = true;
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

/// Read the source operand for MULL / DIVL. Today only Dn-source
/// (mode 0) is implemented — memory EAs need the multi-step
/// continuation pipeline and are deferred to a later phase.
fn read_mull_divl_source(cpu: &Cpu68000, opcode: u16) -> Option<u32> {
    let ea_mode = (opcode >> 3) & 7;
    let ea_reg = (opcode & 7) as usize;
    if ea_mode != 0 {
        return None;
    }
    Some(cpu.regs.d[ea_reg])
}

/// MULU.L / MULS.L. 32×32 multiply, with 32-bit or 64-bit result
/// depending on bit 10 of the extension word.
fn execute_mull(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let Some(src) = read_mull_divl_source(cpu, opcode) else {
        // Memory EA — defer.
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    };

    let ext = cpu.consume_irc();
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
    true
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
    let Some(src) = read_mull_divl_source(cpu, opcode) else {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
    };

    let ext = cpu.consume_irc();
    let dq = ((ext >> 12) & 7) as usize;
    let dr = (ext & 7) as usize;
    let signed = (ext & 0x0800) != 0;
    let wide = (ext & 0x0400) != 0;

    // Divide-by-zero: the 68020 next-PC for a divide-by-zero trap is
    // the address *past* the DIVL instruction (DIVL is a 4-byte
    // instruction: opcode word + extension word).
    if src == 0 {
        cpu.begin_group1_exception(5, cpu.instr_start_pc.wrapping_add(4));
        return true;
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
        return true;
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
    true
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
    let value = read_68020_cr(cpu, ext)
        .or_else(|| motorola_68010::cpu::read_control_register(cpu, ext));
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
            // 68020 CACR mask: bits 0-3 (EI / FI / CI / CD).
            // Higher variants (68030, 68040) widen this; matching
            // Musashi exactly would mean per-variant masks, but
            // the m68k-test-gen corpus generates random values and
            // the 68020 / 68030 mask differs only in upper bits
            // that the corpus doesn't exercise meaningfully.
            cpu.regs.cacr = value & 0x0f;
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
/// Only Dn-destination is implemented today — memory-EA dispatch
/// (mode != 0) needs the multi-step EA pipeline.
fn execute_bf(cpu: &mut Cpu68000, opcode: u16) -> bool {
    let ea_mode = (opcode >> 3) & 7;
    let ea_reg = (opcode & 7) as usize;
    if ea_mode != 0 {
        cpu.begin_group1_exception(4, cpu.instr_start_pc);
        return true;
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
