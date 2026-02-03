//! Cycle-accurate 6502 CPU emulator.
//!
//! The 6502 executes one bus access per cycle. Each `tick()` advances
//! exactly one cycle. Instructions take multiple cycles, and the CPU
//! tracks its internal state between cycles.

mod cpu;
mod flags;
mod registers;

pub use cpu::Mos6502;
pub use flags::Status;
pub use registers::Registers;
