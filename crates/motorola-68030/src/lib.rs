//! Motorola 68030 CPU — skeleton crate.
//!
//! The 68030 adds the on-chip MMU (PMMU + ATC + TT registers) and a
//! 256-byte data cache with burst-fill on top of the 68020's feature
//! set. The 68EC030 omits the MMU; the 68LC030 omits the FPU
//! coprocessor interface.
//!
//! # MMU
//!
//! The on-die MMU first appears in the 68030, so the
//! [`mmu`] module lives here — table-walk descriptor processing,
//! ATC + TT-register matching, and the translation fast path. The
//! 68040's MMU is a fixed-3-level superset and reuses the same
//! module via [`mmu::MmuMode::M68040`]; that's a within-family
//! re-export concern, not a reason to host this code in
//! `motorola-68k-common`.
//!
//! # Today
//!
//! No active machine in the workspace runs a 68030-class part. The
//! decode arms and capability gates for 68030-specific opcodes (the
//! PMMU / PFLUSH / PTEST instruction family) live inside
//! [`motorola_68000`] today, gated on
//! [`motorola_68k_common::CpuModel`] capabilities, with stub bodies
//! that `unimplemented!` on entry — they're dead-code-gated for the
//! M68000 builds the Amiga depends on.
//!
//! # Type aliases
//!
//! [`Cpu68030`], [`Cpu68EC030`], and [`Cpu68LC030`] all resolve to
//! [`motorola_68000::Cpu68000`] today — distinguish them via the
//! corresponding [`motorola_68k_common::CpuModel`] variant at
//! construction time.

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
