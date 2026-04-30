//! Motorola 68000 CPU.
//!
//! This crate hosts the M68000 CPU core: prefetch pipeline, micro-op
//! state machine, decode tables, and execute logic. It also currently
//! hosts the higher-variant decode arms (68010 / 68020 / 68030 /
//! 68040), gated at runtime on
//! [`motorola_68k_common::CpuModel`] capabilities. Reducing the core
//! to M68000-only paths and giving each higher variant its own state
//! machine is the deferred follow-up tracked in `wiki/log.md` as
//! "Cov-5"; today the variant crates re-export this core under the
//! conceptually-correct type alias.
//!
//! # Shared infrastructure
//!
//! Addressing modes, the ALU, the bus pin types, the prefetch
//! micro-op queue, the register file, the status-register flag bits,
//! and the family-wide [`motorola_68k_common::CpuModel`] /
//! [`motorola_68k_common::TimingClass`] /
//! [`motorola_68k_common::CpuCapabilities`] metadata live in
//! `motorola-68k-common`. This crate re-exports those modules under
//! their original paths so existing internal `crate::alu::*` /
//! `crate::flags::*` etc. continue to resolve, and so external
//! consumers (the Amiga machine layer) see the unchanged public API
//! surface.

pub use motorola_68k_common::addressing;
pub use motorola_68k_common::alu;
pub use motorola_68k_common::bus;
pub use motorola_68k_common::flags;
pub use motorola_68k_common::microcode;
pub use motorola_68k_common::model;
pub use motorola_68k_common::registers;

pub mod cpu;
pub mod decode;
pub mod disasm;
pub mod ea;
pub mod execute;

pub use cpu::Cpu68000;
pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};
