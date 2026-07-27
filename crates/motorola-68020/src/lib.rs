//! Motorola 68020 CPU.
//!
//! The 68020 is the first 32-bit-everywhere member of the family:
//! 32-bit external data and address buses, a 32-bit ALU, a barrel
//! shifter, an on-die 256-byte instruction cache, and the
//! coprocessor interface used to bolt on the 68881 / 68882 FPU and
//! the 68851 PMMU. The 68EC020 omits the coprocessor interface (no
//! FPU, no MMU); it shows up in the Amiga CD32 and A1200.
//!
//! # Current implementation
//!
//! [`Cpu68020`] wraps the MC68010 layer and installs the MC68020 ISA,
//! addressing, timing, cache, coprocessor and exception capabilities on
//! the shared reactive core. The Amiga A1200 uses this type as its active
//! CPU. The external bus remains the shared compatibility surface; full
//! 32-bit dynamic bus sizing is separate work.
//!
//! # Architectural delta
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
//!   stack used by tasks the OS schedules. An interrupt accepted with
//!   M set creates an ordinary frame on MSP and a four-word Format `$1`
//!   throwaway frame on ISP.
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
//!   misalignment handling; no exception fires. The current abstract
//!   bus preserves logical RAM values but does not yet model
//!   alignment-dependent split cycles or odd MMIO side effects.
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
//! - **Format `$1`**: 4-word throwaway interrupt frame on ISP, paired
//!   with the ordinary frame retained on MSP.
//! - **Format `$2`**: 6-word instruction-error frame — adds an
//!   `instruction_address` field.
//! - **Format `$9`**: coprocessor mid-instruction (10 words).
//! - **Format `$A`**: short bus / address error (16 words). The current
//!   entry path implements the documented frame extent and field offsets.
//!   Its special-status, pipeline, data-buffer and internal words do not
//!   yet contain the precise state required to rerun a faulted access.
//! - **Format `$B`**: long bus / address error (46 words). Captures
//!   enough internal state for the OS to retry the failing access.
//!
//! `RTE` currently handles Formats `$0`, `$1` and `$2`. Format `$1`
//! restores its saved SR and restarts frame processing on the newly
//! selected stack. Format `$A` is recognised and its complete 16-word
//! footprint is consumed, but exact pipeline restoration and fault rerun
//! remain incomplete. Return handling for Formats `$9` and `$B` remains
//! separate work.
//!
//! # Today's wrapper
//!
//! [`Cpu68020`] wraps [`motorola_68010::Cpu68010`], which in turn
//! wraps [`motorola_68000::Cpu68000`]. Each variant installs a decode
//! hook on the inner 68000's `variant_decode_hook` slot; the 68020
//! hook handles the MC68020 ISA and addressing delta and falls through
//! to the 68010 hook for `MOVEC`, `MOVE-from-CCR` and other inherited
//! opcodes. `Deref` / `DerefMut` chain through both wrappers to the
//! inner 68000 so existing call sites that touch `cpu.regs`,
//! `cpu.state`, `cpu.tick()`, etc. continue to work. The wrapper also
//! enables MSP/ISP selection in the shared register file.
//!
//! All 68020 control registers (`MSP`, `VBR`, `CACR`, `CAAR`, `SFC`,
//! `DFC`) live on the shared
//! [`motorola_68k_common::registers::Registers`] struct, not on the
//! wrapper itself — there's only one source of truth for each
//! register.
//!
//! [`Cpu68EC020`] is currently a type alias to [`Cpu68020`]. The two
//! diverge only when Phase 8 routes F-line opcodes (the EC020 takes
//! `LINE 1111 EMULATOR`; the full 68020 performs the coprocessor
//! handshake).

pub mod cpu;

pub use cpu::Cpu68020;
pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};

/// 68EC020 — the embedded-controller variant of the 68020. No FPU
/// coprocessor socket, no MMU. Used by the Amiga A1200 and CD32.
/// Currently identical to [`Cpu68020`]; Phase 8 forks the F-line path.
pub type Cpu68EC020 = Cpu68020;

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
