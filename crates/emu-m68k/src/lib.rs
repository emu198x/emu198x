//! Unified Motorola 680x0 CPU family emulator.
//!
//! This crate provides cycle-accurate emulation of the Motorola 68000 series CPUs.
//! It is designed from the ground up to support bus wait states, DMA cycle stealing,
//! and multi-variant architectures (68000 through 68060).
//!
//! # Architecture
//!
//! All CPU variants share common infrastructure (registers, flags, ALU, addressing
//! modes) via the `common` module. Each variant lives in its own sub-module and
//! implements the `M68k` trait.
//!
//! The `M68kBus` trait provides word-level access with function codes and wait
//! states, enabling proper Amiga-style DMA cycle stealing. A `CoreBusAdapter`
//! bridges the `emu-core::Bus` trait for backward compatibility with existing
//! test harnesses.
//!
//! # Currently Implemented
//!
//! - **68000**: Full instruction set, cycle-accurate. Passes 317,500 single-step tests.

#![warn(missing_docs)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::struct_excessive_bools)]

pub mod bus;
pub mod common;
pub mod m68000;

pub use bus::{BusResult, CoreBusAdapter, FunctionCode, M68kBus};
pub use common::flags::{self, Status, C, N, V, X, Z};
pub use common::registers::Registers;
pub use m68000::{Cpu68000, Size, AddrMode};
