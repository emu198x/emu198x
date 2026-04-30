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
//! No active machine in the workspace runs a 68030-class part. The
//! M68000 core no longer contains 68030-specific decode arms or
//! capability gates — those were stripped on 2026-04-29.
//!
//! # What a real 68030 implementation needs
//!
//! All 68020 features inherit unchanged. The 68030 is a strict
//! superset on the ISA side; the visible deltas are caches and the
//! on-die MMU.
//!
//! ## Caches
//!
//! - **256-byte data cache** — direct-mapped, 16 lines × 4 words.
//!   Write-through, write-no-allocate. Toggled via CACR bits 8-11
//!   (ED / FD / CD / CED). Hit on a tag-and-FC match.
//! - **Burst fill** — 4-word burst transactions on cache misses
//!   when CACR `WA` (write-allocate) is set. Memory must assert
//!   /CIIN if a region is non-cacheable.
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
//! # Type aliases
//!
//! [`Cpu68030`], [`Cpu68EC030`], and [`Cpu68LC030`] all resolve to
//! [`motorola_68000::Cpu68000`] today — construct via
//! [`motorola_68000::Cpu68000::new`].

pub mod mmu;

pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};
pub use motorola_68000::Cpu68000 as Cpu68030;
pub use motorola_68000::Cpu68000 as Cpu68EC030;
pub use motorola_68000::Cpu68000 as Cpu68LC030;

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
/// (no FPU, no MMU).
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
