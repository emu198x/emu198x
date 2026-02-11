//! Motorola 68000 CPU core with cycle-accurate IR/IRC prefetch pipeline.
//!
//! This crate implements the 68000 CPU at the cycle level, with an explicit
//! two-word prefetch pipeline (IR + IRC) that matches real hardware behavior.
//!
//! The tick engine follows the Z80 crate's proven architecture — per-cycle
//! ticking, explicit micro-op queue, instant Execute — adapted for the
//! 68000's 4-cycle bus and 2-word prefetch pipeline.

pub mod addressing;
pub mod alu;
pub mod bus;
pub mod cpu;
mod decode;
mod ea;
mod exceptions;
mod execute;
pub mod flags;
mod microcode;
pub mod registers;
mod shifts;
mod timing;

pub use alu::Size;
pub use addressing::AddrMode;
pub use bus::{BusResult, FunctionCode, M68kBus};
pub use cpu::Cpu68000;
pub use flags::{Status, C, N, V, X, Z};
pub use registers::Registers;
