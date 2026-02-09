//! Cycle-accurate Commodore 64 emulator.
//!
//! The C64 master clock ticks at CPU cycle rate (985,248 Hz PAL). All
//! components (VIC-II, CIA, SID) tick at this rate. One frame is 312
//! raster lines x 63 cycles = 19,656 CPU cycles (~50.12 Hz).

mod bus;
mod c64;
pub mod capture;
mod cia;
mod config;
pub mod input;
mod keyboard;
pub mod keyboard_map;
pub mod mcp;
mod memory;
mod palette;
pub mod prg;
mod sid;
pub mod vic;

pub use bus::C64Bus;
pub use c64::C64;
pub use config::{C64Config, C64Model};
pub use input::{C64Key, InputQueue};
pub use keyboard::KeyboardMatrix;
pub use memory::C64Memory;
pub use vic::Vic;
