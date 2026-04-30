//! Motorola 68020 CPU — skeleton crate.
//!
//! The 68020 adds 32-bit MUL/DIV, bitfield instructions, CAS/CAS2,
//! the scaled-index brief extension word, the barrel shifter, the
//! Master Stack Pointer (MSP), the cache-control / cache-address
//! registers (CACR/CAAR), and the coprocessor interface used by the
//! 68881/68882 FPU and the 68851 PMMU.
//!
//! # Today
//!
//! No active machine in the workspace runs a 68020-class part. The
//! decode arms and capability gates for 68020-specific opcodes live
//! inside [`motorola_68000`] today, gated on
//! [`motorola_68k_common::CpuModel`] capabilities. This crate is the
//! architectural seam for when a 68020-class machine appears
//! (likely an A1200 or A4000 down the line).
//!
//! # Type aliases
//!
//! [`Cpu68020`] resolves to [`motorola_68000::Cpu68000`] — construct
//! it with [`motorola_68k_common::CpuModel::M68020`]. The
//! [`Cpu68EC020`] alias is identical to `Cpu68020` today; on the
//! real silicon the EC variant differs only in the absence of the
//! coprocessor interface (no FPU, no MMU), already represented in
//! [`motorola_68k_common::CpuModel::M68EC020`].

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
