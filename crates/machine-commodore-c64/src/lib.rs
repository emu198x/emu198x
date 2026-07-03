//! Commodore 64 machine substrate for the fresh workspace.
//!
//! This crate deliberately stops short of a runnable C64. It owns the durable
//! board-level behaviours that future chip models will need:
//! - 6510 `$00`/`$01` port semantics and memory banking
//! - colour RAM and VIC-visible character ROM access rules
//! - live CIA keyboard scan and VIC bank selection
//! - live VIC-II raster, BA, IRQ, and framebuffer ownership
//! - first datasette transport and pulse-to-FLAG integration

pub mod config;
mod datasette;
mod easyflash;
mod flash040;
pub mod keyboard;
pub mod machine;
pub mod memory;
mod serde_skip_audit;

pub use config::{C64Config, C64Model};
pub use keyboard::KeyboardMatrix;
pub use machine::{C64, C64Snapshot};
pub use memory::{C64Memory, MemoryInitError};
pub use mos_sid_6581::{AudioControls, SidChannel};
