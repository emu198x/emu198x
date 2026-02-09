//! Cycle-accurate NES emulator.
//!
//! The NES master clock ticks at 21,477,272 Hz (NTSC crystal). The PPU
//! ticks at crystal/4 (5,369,318 Hz) and the CPU at crystal/12
//! (1,789,773 Hz), giving a 3:1 PPU:CPU ratio.
//!
//! One frame = 341 PPU dots × 262 scanlines = 89,342 PPU cycles.

mod apu;
mod bus;
pub mod capture;
mod cartridge;
mod config;
mod controller;
pub mod controller_map;
pub mod input;
pub mod mcp;
mod nes;
mod palette;
pub mod ppu;

pub use bus::NesBus;
pub use config::NesConfig;
pub use controller::Controller;
pub use input::{InputQueue, NesButton};
pub use nes::Nes;
