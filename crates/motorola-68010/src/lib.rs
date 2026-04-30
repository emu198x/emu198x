//! Motorola 68010 CPU — skeleton crate.
//!
//! The 68010 adds the Vector Base Register (VBR), the MOVEC family of
//! control-register moves, loop mode (DBcc fast-loop optimisation),
//! and a 6-word stack frame instead of the 68000's 4-word frame.
//!
//! # Today
//!
//! No machine in this workspace exercises the 68010. The decode arms
//! and runtime capability gates for 68010-specific instructions live
//! inside [`motorola_68000`] today, fed by the
//! [`motorola_68k_common::CpuModel::M68010`] variant. This crate is
//! the architectural seam: when a 68010-class machine arrives, the
//! 68010-specific code paths peel off into a dedicated state machine
//! here.
//!
//! # Type aliases
//!
//! [`Cpu68010`] resolves to [`motorola_68000::Cpu68000`] today —
//! construct it via [`motorola_68000::Cpu68000::new_with_model`] with
//! [`motorola_68k_common::CpuModel::M68010`]. Once the M68000-only
//! reduction lands (Cov-5 in `wiki/log.md`), `Cpu68010` becomes its
//! own struct that wraps or supersedes `Cpu68000`.

pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};
pub use motorola_68000::Cpu68000 as Cpu68010;

/// Marker zero-sized type identifying the 68010 variant.
///
/// Reserved for the future per-variant generic shape: when
/// `Cpu68k<M: M68kVariant>` lands, this is the type that carries
/// `M68010`-specific associated types and methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68010Variant;

impl M68010Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68010
    }
}
