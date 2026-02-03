//! Cycle-accurate Motorola 68000 CPU emulator.
//!
//! This crate provides a cycle-accurate emulation of the Motorola 68000 CPU,
//! following the micro-op queue pattern used by the Z80 emulator.
//!
//! # Design
//!
//! The 68000 is a 16/32-bit processor with a 16-bit data bus and 24-bit
//! address bus. Unlike the Z80's 3-cycle minimum memory access, the 68000
//! has a 4-cycle minimum for bus operations.
//!
//! Instructions are broken down into micro-operations that execute one clock
//! cycle at a time. This allows for cycle-accurate emulation and proper
//! handling of Amiga-style DMA cycle stealing.
//!
//! # Usage
//!
//! ```ignore
//! use emu_68000::M68000;
//! use emu_core::{Bus, SimpleBus};
//!
//! let mut cpu = M68000::new();
//! let mut bus = SimpleBus::new();
//!
//! // Load program into memory
//! bus.load(0x1000, &[0x70, 0x42]); // MOVEQ #$42, D0
//!
//! // Set PC and run
//! cpu.reset();
//! cpu.regs.pc = 0x1000;
//!
//! // Execute one instruction
//! while !cpu.micro_ops.is_empty() {
//!     cpu.tick(&mut bus);
//! }
//! ```

#![warn(missing_docs)]

mod cpu;
mod flags;
mod microcode;
mod registers;

pub use cpu::{AddrMode, InstrPhase, M68000, Size};
pub use flags::{Status, C, N, V, X, Z};
pub use microcode::{MicroOp, MicroOpQueue};
pub use registers::Registers;
