//! Shared C64 engine, peripherals, timing and fixed-board binding.
//!
//! Concrete owners live in the four regional breadbin/C64C machine crates.
//! The original regression suite is family evidence, not board-exclusive evidence.

mod action_replay;
pub mod config;
mod datasette;
mod easyflash;
mod final_cartridge3;
mod flash040;
mod freeze_cart;
pub mod keyboard;
pub mod machine;
pub mod memory;
mod prg_loader;
mod serde_skip_audit;

pub use config::{C64Config, C64Model};
pub use emu198x_mos_sid_6581::{AudioControls, SidChannel};
pub use keyboard::KeyboardMatrix;
pub use machine::{C64, C64Snapshot};
pub use memory::{C64Memory, MemoryInitError};

extern crate self as common_commodore_c64;
pub mod timing;
pub use timing::{C64Timing, FRAMEBUFFER_WIDTH, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
pub mod board;
