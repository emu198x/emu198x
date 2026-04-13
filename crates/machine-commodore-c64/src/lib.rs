//! Commodore 64 machine substrate for the fresh workspace.
//!
//! This crate deliberately stops short of a runnable C64. It owns the durable
//! board-level behaviours that future chip models will need:
//! - 6510 `$00`/`$01` port semantics and memory banking
//! - colour RAM and VIC-visible character ROM access rules
//! - CIA-side keyboard scan and VIC bank selection latches
//! - `phi2`-driven raster/frame timing

pub mod config;
pub mod keyboard;
pub mod machine;
pub mod memory;

pub use config::{C64Config, C64Model};
pub use keyboard::KeyboardMatrix;
pub use machine::C64;
pub use memory::{C64Memory, MemoryInitError};
