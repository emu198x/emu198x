//! Motorola 68000 CPU.
//!
//! This crate hosts the M68000 CPU core: prefetch pipeline, micro-op
//! state machine, decode tables, and execute logic. It implements the
//! 68000 ISA *only* — no 68010+ instructions, no caches, no FPU, no
//! MMU. Higher-variant cores (68010 / 68020 / 68030 / 68040) live in
//! their own crates and currently re-export this type as a stand-in
//! until each variant's state machine is built out.
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
pub mod icache;

pub use cpu::Cpu68000;
pub use icache::ICache;
pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};
