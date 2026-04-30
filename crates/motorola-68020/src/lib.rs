//! Motorola 68020 CPU — skeleton crate.
//!
//! The 68020 is the first 32-bit-everywhere member of the family:
//! 32-bit external data and address buses, a 32-bit ALU, a barrel
//! shifter, an on-die 256-byte instruction cache, and the
//! coprocessor interface used to bolt on the 68881 / 68882 FPU and
//! the 68851 PMMU. The 68EC020 omits the coprocessor interface (no
//! FPU, no MMU); it shows up in the Amiga CD32 and A1200.
//!
//! # Today
//!
//! No active machine in the workspace runs a 68020-class part. The
//! M68000 core no longer contains 68020-specific decode arms or
//! capability gates — those were stripped on 2026-04-29. When a
//! 68020 machine arrives, this crate gains its own core and the type
//! aliases collapse into real types.
//!
//! # What a real 68020 implementation needs
//!
//! References are to the M68000PRM and the M68020 User's Manual.
//! 68020-specific instructions and behaviours are *additive* on top
//! of every 68010 feature; the 68020 is a strict superset.
//!
//! ## Bus / pipeline
//!
//! - **3-clock minimum bus cycle** (down from 4 on the 68000 / 68010)
//!   for fast memory. Chip RAM still synchronises to the DMA slot
//!   grid via `BusStatus::Wait`.
//! - **32-bit data bus** with dynamic bus sizing (DSACK0/1) — a
//!   slave can declare itself 8 / 16 / 32 bits wide on each cycle.
//! - **Two-word instruction pipeline** with parallel decode → no
//!   internal-cycle dead time around extension words.
//! - **256-byte direct-mapped instruction cache** (16 lines × 4
//!   words). Controlled by CACR (cache enable / freeze / clear).
//!
//! ## New control registers (atop 68010's VBR / SFC / DFC)
//!
//! - **CACR** (Cache Control Register) — EI/FI/CI bits for the I-cache.
//! - **CAAR** (Cache Address Register) — specifies the cache line a
//!   CINV / CPUSH instruction targets.
//! - **MSP** (Master Stack Pointer) — second supervisor stack
//!   selected when SR M-flag (bit 12) is set. The unmasked SSP
//!   becomes the *Interrupt* Stack Pointer; the MSP is the *Master*
//!   stack used by tasks the OS schedules. Eight-word interrupt
//!   stack frames (format `$1`) get the throwaway treatment.
//!
//! ## New instructions (illegal on M68000 / M68010)
//!
//! - **32-bit MUL.L / MULS.L / MULU.L / DIVS.L / DIVU.L**
//!   (`$4C00`-`$4C7F` with extension word). Optional 64-bit dividend
//!   for `DIVx.L` (Dh:Dl → Dl:Dh quotient:remainder).
//!   M68000PRM § 6.2.5 / 6.2.7.
//! - **Bit-field family**: `BFTST` / `BFEXTU` / `BFEXTS` / `BFINS` /
//!   `BFCLR` / `BFSET` / `BFCHG` / `BFFFO`. Encoding `$E8C0`-`$EFC0`
//!   with extension word holding offset / width (literal or `Dn`).
//!   M68000PRM § 6.2.2 — § 6.2.4 (BFFFO is § 6.2.3).
//! - **CAS / CAS2** (compare-and-swap, single / dual-address).
//!   `$0AC0`-`$0EC0` and `$0EFC` with extension words. The atomic
//!   primitive used by Lattice / Aztec C runtimes for SMP-style
//!   list manipulation. M68000PRM § 6.2.6.
//! - **CHK2 / CMP2** — bound-checked array index against a memory
//!   tuple. Encoding `$00C0`-`$06C0` with extension word.
//!   M68000PRM § 6.2.6 / 6.2.10.
//! - **EXTB.L** (`$49C0`) — sign-extend byte → long. The 68000's
//!   `EXT` only does word and long; EXTB closes the gap.
//!   M68000PRM § 6.2.13.
//! - **PACK** / **UNPK** — BCD ↔ ASCII conversion in registers.
//!   `$8100`-`$8FFF` predec or postinc forms. M68000PRM § 6.2.27.
//! - **TRAPcc** (`$50F8`-`$5FFC`) — conditional TRAP based on cc
//!   field, with optional 16- or 32-bit immediate operand.
//!   M68000PRM § 6.2.40.
//! - **CALLM** / **RTM** (`$06C0` / `$06C0` family) — module call
//!   instructions for descriptor-based linkage. Removed in the 68030
//!   but legal on the 68020.
//! - **Bcc.L** — long-displacement branch (32-bit displacement word)
//!   alongside the existing 8-bit and 16-bit forms.
//!
//! ## Changed instruction behaviour
//!
//! - **Barrel shifter** — `LSL` / `LSR` / `ASL` / `ASR` / `ROL` /
//!   `ROR` / `ROXL` / `ROXR` execute in constant time, regardless of
//!   shift count, instead of the 68000's `2 + 2n` clocks.
//! - **Brief extension word with scaled index** — `(d8,An,Xn.SIZE*SCALE)`
//!   extension word adds bits 9-10 for `*1` / `*2` / `*4` / `*8`
//!   index scaling. The 68000 ignores those bits.
//! - **Full extension word format** — entirely new addressing mode
//!   `([bd,An,Xn.SIZE*SCALE],od)` with optional base displacement,
//!   pre-/post-indexed memory indirection, and outer displacement.
//!   M68000PRM § 2.2.5.4 — § 2.2.5.10. Parsing this is roughly the
//!   same effort as everything else combined.
//! - **Address Error generation** — only on instruction fetch.
//!   Data accesses to odd addresses go through hardware
//!   misalignment handling; no exception fires.
//! - **MOVE from SR** is privileged here too (matches 68010).
//!
//! ## Coprocessor interface (cpID 1 = FPU, cpID 2 = PMMU)
//!
//! F-line opcodes (`$F000`-`$FFFF`) talk to coprocessors via the
//! coprocessor interface (CIR/CSR/CCR memory-mapped at $00xxxxxx).
//! `cpGEN` / `cpScc` / `cpDBcc` / `cpBcc.[WL]` / `cpSAVE` / `cpRESTORE`
//! all live in this space. Implementing the 68881 / 68882 happens by
//! routing these opcodes through [`motorola_68040::fpu`].
//!
//! ## Exception frames
//!
//! - **Format `$0`**: short — 4-word frame (group 1/2).
//! - **Format `$1`**: throwaway interrupt (8-word frame, MSP path).
//! - **Format `$2`**: 6-word frame (instruction-error trap, address
//!   error, etc.) — adds `instruction_address` field.
//! - **Format `$9`**: coprocessor mid-instruction (10 words).
//! - **Format `$A`**: short bus / address error (16 words). The
//!   instruction is *not* restartable — the handler must skip past
//!   the failing instruction.
//! - **Format `$B`**: long bus / address error (46 words). Captures
//!   enough internal state for the OS to retry the failing access.
//!
//! `RTE` dispatches on the format word and pops the right amount.
//!
//! # Type aliases
//!
//! [`Cpu68020`] and [`Cpu68EC020`] both resolve to
//! [`motorola_68000::Cpu68000`] today — the M68000 core handles the
//! shared subset and any 68020 capability is *absent*. Construct via
//! [`motorola_68000::Cpu68000::new`].

pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};
pub use motorola_68000::Cpu68000 as Cpu68020;
pub use motorola_68000::Cpu68000 as Cpu68EC020;

/// Marker zero-sized type identifying the 68020 variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68020Variant;

impl M68020Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68020
    }
}

/// Marker zero-sized type identifying the 68EC020 variant
/// (no FPU, no MMU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68EC020Variant;

impl M68EC020Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68EC020
    }
}
