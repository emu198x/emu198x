//! Zilog Z80 CPU core.
//!
//! Source references:
//! - `wiki/chips/zilog-z80.md`
//! - `wiki/decisions/half-cycle-signals.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/zilog-z80/`
//!
//! This port keeps the fresh-start architecture intact: half-cycle ticks,
//! public pin fields, no bus trait, and static M-step sequences.

pub mod alu;
mod execute;
pub mod mcycle;
pub mod registers;
pub(crate) mod walker;
pub mod z80;

pub use z80::Z80;
