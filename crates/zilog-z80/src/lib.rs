//! Zilog Z80 CPU core.
//!
//! Source references:
//! - `knowledge/chips/zilog-z80.md`
//! - `knowledge/decisions/half-cycle-signals.md`
//! - Adapted from `../Emu198x-Older/crates/zilog-z80/`
//!
//! This port keeps the fresh-start architecture intact: half-cycle ticks,
//! public pin fields, no bus trait, and static M-step sequences.

pub mod alu;
mod execute;
pub mod mcycle;
pub mod registers;
pub mod walker;
pub mod z80;

pub use registers::Registers;
pub use z80::{BusOp, Z80};
