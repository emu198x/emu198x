//! Motorola 68010 CPU.
//!
//! [`Cpu68010`] wraps the shared [`motorola_68000::Cpu68000`] engine
//! and installs the 68010 decode and continuation hooks. Later family
//! wrappers build on this type, so the active A1200's `Cpu68020`
//! inherits the implemented 68010 exception behavior.
//!
//! # Implemented surface
//!
//! - `MOVEC` for VBR, SFC, DFC and USP.
//! - `RTD #d16`.
//! - `MOVE from CCR` with a data-register destination.
//! - VBR-relative exception-vector fetches.
//! - Four-word Format `$0` exception frames and matching `RTE`
//!   handling.
//! - Device, autovector, uninitialized and spurious interrupt
//!   responses with one consistent stacked vector offset.
//!
//! The control-register storage and exception continuations live on
//! the shared core. The wrapper re-installs its skipped function hooks
//! and behavior flags after deserialization.
//!
//! # Deferred surface
//!
//! `MOVES`, memory destinations for `MOVE from CCR`, privileged
//! 68010 `MOVE from SR`, loop-mode timing, external `BKPT`
//! acknowledge, restartable Format `$8` bus-error frames and
//! cycle-exact exception stack ordering remain unimplemented. The
//! current short-frame interrupt result is architectural; the shared
//! compatibility bus is not a claim of variant-accurate pins.

pub mod cpu;

pub use cpu::{Cpu68010, continue_68010_opcode, decode_68010_opcode};
pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};

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
