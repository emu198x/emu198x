//! Cycle-accurate Z80 CPU emulator.
//!
//! Each call to `tick()` advances exactly one T-state.

mod alu;
mod cpu;
mod flags;
mod microcode;
mod registers;

pub use cpu::Z80;
pub use flags::{CF, HF, NF, PF, SF, XF, YF, ZF};
pub use registers::Registers;

#[cfg(feature = "test-utils")]
pub use microcode::MicroOp;
