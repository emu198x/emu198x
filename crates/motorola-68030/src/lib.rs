//! Motorola 68030 CPU — skeleton crate.
//!
//! The 68030 brings the PMMU on-die (it was a separate 68851 part on
//! the 68020) and adds a 256-byte data cache with burst-fill on top
//! of the 68020's 256-byte instruction cache. The 68EC030 omits the
//! MMU; the 68LC030 omits the FPU coprocessor interface.
//!
//! # MMU
//!
//! The on-die MMU first appears in the 68030, so the [`mmu`] module
//! lives here — table-walk descriptor processing, ATC + TT-register
//! matching, and the translation fast path. The 68040's MMU is a
//! fixed-3-level superset and reuses the same module via
//! [`mmu::MmuMode::M68040`]; that's a within-family re-export
//! concern, not a reason to host this code in `motorola-68k-common`.
//!
//! # Today
//!
//! No active machine in the workspace runs a 68030-class part yet.
//! The wrapper now owns its CACR layout and CDIS input; the remaining
//! cache and MMU datapaths stay out of the M68000 instruction core.
//!
//! # What a real 68030 implementation needs
//!
//! All 68020 features inherit unchanged. The 68030 is a strict
//! superset on the ISA side; the visible deltas are caches and the
//! on-die MMU.
//!
//! ## Caches
//!
//! - **256-byte data cache** — direct-mapped, 16 lines × four long
//!   words. Write-through; CACR.WA optionally enables aligned long-word
//!   write allocation. Toggled via CACR bits 8-13. Hit on a tag-and-FC
//!   match.
//! - **Burst fill** — four-long-word burst transactions on cache misses
//!   when the relevant CACR instruction/data burst-enable bit is set.
//!   Memory asserts /CIIN if a region is non-cacheable.
//! - **Cache lines tag function-code bits** so supervisor and user
//!   accesses don't share lines.
//!
//! ## On-die PMMU
//!
//! - **Translation Control register (TC)** — bits 31 / 25-23 / 22 /
//!   16 control enable / page-size / supervisor-root pointer /
//!   function-code lookup table.
//! - **Supervisor / CPU Root Pointers (SRP / CRP)** — 64-bit
//!   descriptors pointing at the table tree root for supervisor
//!   space and user space respectively.
//! - **Transparent Translation (TT0 / TT1)** — bypass the table walk
//!   for matching address ranges. Two registers, each defines a
//!   contiguous range with FC mask and read/write filtering.
//! - **Address Translation Cache (ATC)** — fully associative,
//!   22 entries on the 68030. Fast path for translated-address hits.
//! - **PMOVE** — privileged move to / from the MMU registers.
//!   F-line, cpID 0, type 000.
//! - **PFLUSH** — invalidate ATC entries. Variants by FC mask /
//!   address / `An` register.
//! - **PTEST** (read / write directions) — perform a table walk,
//!   set MMUSR with the result, optionally store the level reached
//!   into an `An`. Used by debuggers for "what's mapped here?"
//! - **PLOAD** — force-load an ATC entry from a manual table walk.
//!
//! All MMU instructions are encoded as F-line opcodes with cpID 0
//! (encoding range `$F000`-`$F1FF`) — distinct from the 68040's
//! direct PFLUSH / PTEST encoding (`$F500`-`$F5FF`).
//!
//! ## Exception frames
//!
//! Adds **format `$B`** — long bus / address error frame, 46 words,
//! captures enough internal state for the kernel to restart the
//! faulting access after fixing the page table. The 68040's bus
//! error frame is format `$7` (different layout).
//!
//! # Today's wrapper
//!
//! [`Cpu68030`] wraps [`motorola_68020::Cpu68020`] via the family
//! variant pattern. CACR uses the shared MOVEC path with
//! MC68030-specific masks installed by this wrapper. PMOVE / PFLUSH /
//! PTEST / PLOAD aren't in the `m68k-test-gen` corpus, so the wrapper
//! otherwise inherits the 68020 hook chain. [`Cpu68EC030`] /
//! [`Cpu68LC030`] are type aliases to [`Cpu68030`] until their FPU /
//! MMU execution paths diverge.

pub mod cpu;
pub mod mmu;

pub use cpu::Cpu68030;
pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};

/// 68EC030 — no on-die MMU. Currently identical to [`Cpu68030`];
/// the divergence appears when MMU instructions land and the EC
/// variant takes ILLEGAL on PMOVE/PFLUSH/PTEST/PLOAD instead.
pub type Cpu68EC030 = Cpu68030;

/// 68LC030 — MMU present but no FPU coprocessor interface.
/// Currently identical to [`Cpu68030`]; diverges when F-line
/// cpID=1 (FPU) dispatch is wired and the LC variant takes
/// `LINE 1111 EMULATOR` instead of completing the handshake.
pub type Cpu68LC030 = Cpu68030;

/// Marker zero-sized type identifying the 68030 variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68030Variant;

impl M68030Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68030
    }
}

/// Marker zero-sized type identifying the 68EC030 variant
/// (external FPU interface, no on-die MMU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68EC030Variant;

impl M68EC030Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68EC030
    }
}

/// Marker zero-sized type identifying the 68LC030 variant
/// (MMU, no FPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68LC030Variant;

impl M68LC030Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68LC030
    }
}
