//! Motorola 68040 CPU — skeleton crate.
//!
//! The 68040 brings the FPU on-chip (alongside the MMU inherited
//! from the 68030), splits the ATC into separate instruction and
//! data ATCs, adds MOVE16 for cache-line-sized transfers, and
//! introduces the four transparent-translation registers
//! (ITT0/ITT1/DTT0/DTT1). The 68EC040 omits the FPU and MMU; the
//! 68LC040 omits the FPU but keeps the MMU.
//!
//! # FPU
//!
//! The [`fpu`] module owns the floating-point support: data-format
//! conversions (Single / Double / Extended / Packed BCD / integer),
//! FPSR condition-code management, FMOVECR ROM constants, and the
//! arithmetic primitives that back FADD / FSUB / FMUL / FDIV / FCMP
//! / FINT / FGETMAN / FGETEXP / FSCALE / FSINCOS. The 68881 / 68882
//! coprocessor uses the same code via the 68020's coprocessor
//! interface.
//!
//! # Today
//!
//! No active machine in the workspace runs a 68040-class part. The
//! M68000 core no longer contains 68040-specific decode arms or
//! capability gates — those were stripped on 2026-04-29.
//!
//! # What a real 68040 implementation needs
//!
//! All 68030 ISA inherits, but the *implementation* of caches and
//! MMU is materially different.
//!
//! ## Pipeline / bus
//!
//! - **Six-stage pipeline** (fetch / decode 1 / decode 2 / address /
//!   execute / writeback) — most instructions retire in one clock
//!   when the operand cache hits.
//! - **1-clock effective bus** for cached fetches; external bus
//!   transactions are 2-clock minimum.
//! - **Wider buses** internally — 32-bit-everywhere with separate
//!   instruction and data paths.
//!
//! ## Caches
//!
//! - **4 KB instruction cache** (4-way set-associative, 16-byte
//!   lines) — much bigger than the 68030's 256 bytes.
//! - **4 KB data cache** (4-way set-associative, 16-byte lines)
//!   with **copyback or write-through** policy per page (driven by
//!   the MMU descriptor's CM bits).
//! - **CINV** / **CPUSH** — encoded as `$F4xx` (NOT coprocessor
//!   format). Cache select bits choose data / instruction / both;
//!   scope bits choose line / page / all. M68040UM § 8.4.
//!
//! ## On-die MMU (rev of the 68030 PMMU)
//!
//! - **Fixed three-level table walk** (4 KB or 8 KB pages). The
//!   68030's flexible TC bit-field is gone; page size is fixed by
//!   TC[14] only.
//! - **Separate instruction / data ATCs** (64 entries each, fully
//!   associative).
//! - **Four transparent-translation registers**: ITT0 / ITT1 /
//!   DTT0 / DTT1 (vs the 68030's two TT0 / TT1 covering both
//!   instruction and data).
//! - **PFLUSH / PTEST** are *not* coprocessor F-line — they have
//!   their own encoding `$F500`-`$F5FF` (different from the 68030).
//!   `PFLUSHA` clears the entire ATC; `PFLUSH (An)` flushes one
//!   entry; `PTESTW (An)` / `PTESTR (An)` read the table for a
//!   specific FC and address.
//! - **No PMOVE** — MMU control registers move via `MOVEC`
//!   (cr codes $003 / $004 / $005 / $006 / $007 / $805 / $806 /
//!   $807 for TC / ITT0 / ITT1 / DTT0 / DTT1 / MMUSR / URP / SRP).
//!
//! ## On-die FPU (encoded as cpID 1 F-line opcodes)
//!
//! Routes through [`fpu`] for the actual arithmetic. The decode
//! side recognises:
//!
//! - **FADD / FSUB / FMUL / FDIV / FCMP / FNEG / FABS / FSQRT** and
//!   their precision-rounded variants (FSADD, FDADD, etc.).
//! - **FMOVE** — between FP register and EA, with format conversion
//!   (Byte / Word / Long / Single / Double / Extended / Packed BCD).
//! - **FMOVEM** — register-mask move of FP0-FP7 / FPCR / FPSR / FPIAR.
//! - **FMOVECR** — load a ROM constant (pi, e, log2, etc.).
//! - **FINT / FINTRZ / FGETMAN / FGETEXP / FSCALE** — IEEE
//!   transcendental support.
//! - **FBcc.[WL]** / **FScc** / **FDBcc** / **FTRAPcc** — FP
//!   conditional control flow.
//! - **FSAVE** / **FRESTORE** — save / restore the FPU's internal
//!   exception-handling state across context switches.
//!
//! ## New non-FP instruction
//!
//! - **MOVE16** (`$F600`-`$F6FF`) — copy one 16-byte cache line.
//!   Five forms:
//!   - `(Ax)+, (Ay)+`           — both post-increment, ext word
//!     carries `Ay` register in bits 14-12.
//!   - `(Ax)+, abs.L`           — post-inc source, 32-bit abs dest.
//!   - `abs.L, (Ax)+`           — 32-bit abs source, post-inc dest.
//!   - `(Ax),  abs.L`           — non-incrementing source.
//!   - `abs.L, (Ax)`            — non-incrementing dest.
//!
//!   Both addresses are forced 16-byte aligned (low 4 bits cleared).
//!   Used by AmigaOS 3.1+ for fast `CopyMem()` and by NetBSD/m68k
//!   for page copies. M68040UM § 4.5.
//!
//! ## Exception frames
//!
//! - **Format `$2`**: 6-word frame (instruction trap).
//! - **Format `$3`**: floating-point post-instruction (4 words after
//!   header).
//! - **Format `$7`**: 30-word access-fault frame (M68040 specific).
//!   Captures effective address, special status word (SSW), write
//!   buffer state. Different from the 68030's format `$B`.
//!
//! # Today's wrapper
//!
//! [`Cpu68040`] wraps [`motorola_68030::Cpu68030`] via the family
//! variant pattern. No 68040-specific decode hooks are installed
//! yet — MOVE16 / CINV / CPUSH aren't in the `m68k-test-gen`
//! corpus, and the FPU module (`fpu.rs`) is unused until F-line
//! dispatch is wired. [`Cpu68EC040`] / [`Cpu68LC040`] are type
//! aliases to [`Cpu68040`] until their MMU / FPU presence
//! actually diverges in behaviour.

pub mod cpu;

/// FPU support now lives in `motorola_68k_common::fpu` so the 68020 hook
/// (which the 68040 wraps) can reach it without a circular dependency.
/// Re-exported here so existing `motorola_68040::fpu::…` paths keep
/// working.
pub use motorola_68k_common::fpu;

pub use cpu::Cpu68040;
pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};

/// 68EC040 — no on-die MMU, no FPU coprocessor. Currently
/// identical to [`Cpu68040`]; diverges when MMU instructions land
/// (EC takes ILLEGAL) and when F-line cpID=1 is wired (EC takes
/// `LINE 1111 EMULATOR`).
pub type Cpu68EC040 = Cpu68040;

/// 68LC040 — MMU present, no FPU. Currently identical to
/// [`Cpu68040`]; diverges when F-line cpID=1 (FPU) dispatch lands.
pub type Cpu68LC040 = Cpu68040;

/// Marker zero-sized type identifying the 68040 variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68040Variant;

impl M68040Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68040
    }
}

/// Marker zero-sized type identifying the 68EC040 variant
/// (no FPU, no MMU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68EC040Variant;

impl M68EC040Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68EC040
    }
}

/// Marker zero-sized type identifying the 68LC040 variant
/// (MMU, no FPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68LC040Variant;

impl M68LC040Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68LC040
    }
}
