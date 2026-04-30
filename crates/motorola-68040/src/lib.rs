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
//! decode arms and capability gates for 68040-specific opcodes
//! (FPU instructions, MOVE16, the new MMU instructions) live inside
//! [`motorola_68000`] today; the FPU register file lives in
//! [`motorola_68k_common::registers`] because every variant struct
//! shares that file. This crate is the architectural seam for when
//! a 68040-class machine appears.
//!
//! # Type aliases
//!
//! [`Cpu68040`], [`Cpu68EC040`], and [`Cpu68LC040`] resolve to
//! [`motorola_68000::Cpu68000`] today — distinguish them via the
//! corresponding [`motorola_68k_common::CpuModel`] variant at
//! construction time.

pub mod fpu;

pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};
pub use motorola_68000::Cpu68000 as Cpu68040;
pub use motorola_68000::Cpu68000 as Cpu68EC040;
pub use motorola_68000::Cpu68000 as Cpu68LC040;

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
