//! Shared infrastructure for the Motorola 680x0 family.
//!
//! This crate carries the pieces that every variant in the family
//! agrees on: addressing modes, the ALU with flag bookkeeping, bus
//! pin types, the prefetch micro-op queue, the register file, the
//! status-register flag bits, and the family-wide [`CpuModel`] /
//! [`TimingClass`] / [`CpuCapabilities`] metadata.
//!
//! # Shape
//!
//! The CPU implementation lives in the per-variant crates. This
//! crate intentionally carries **no** decode tables, **no** execute
//! logic, and **no** `Cpu*` struct. The split mirrors the family
//! layering rule from `knowledge/decisions/within-family-layering.md` —
//! a `common-{family}` crate plus per-silicon-part variant crates.
//!
//! # Per-variant generic vs. concrete shape
//!
//! The 68k family adds genuinely new opcodes (bitfield, MUL.L,
//! cas/cas2, MOVE16, FPU, MMU table walk) at each variant step, not
//! just timing differences. A single `Cpu68k<M: M68kVariant>` would
//! either need many associated types and dispatch methods, or it
//! would force a single concrete state machine that ignores variant
//! information. We use the **per-variant concrete struct** fallback
//! the architectural plan permits: each variant has its own `Cpu*`
//! type alias, and higher variants depend on lower ones to inherit
//! base behaviour.
//!
//! Today every variant alias resolves to [`motorola_68000::Cpu68000`]
//! because the M68000 core still hosts every variant's instruction
//! paths internally (gated on [`CpuModel`] capabilities). Reducing
//! `motorola-68000` to the M68000-only paths and giving each higher
//! variant its own state machine is the deferred follow-up work
//! tracked in the knowledge/log.md as "Cov-5".
//!
//! # MMU and FPU placement
//!
//! The 68030's MMU lives in [`motorola_68030::mmu`] and the 68040's
//! FPU lives in [`motorola_68040::fpu`] — the conceptually-correct
//! homes for those features (on-die MMU first appears in the 68030,
//! on-die FPU in the 68040). Neither lives here, because neither is
//! shared family-wide.

pub mod addressing;
pub mod alu;
pub mod bus;
pub mod flags;
pub mod microcode;
pub mod model;
pub mod registers;

pub use model::{CpuCapabilities, CpuModel, TimingClass};
