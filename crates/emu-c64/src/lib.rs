//! Cycle-accurate Commodore 64 emulator.
//!
//! The C64 master clock ticks at CPU cycle rate (985,248 Hz PAL). All
//! components (VIC-II, CIA, SID) tick at this rate. One frame is 312
//! raster lines x 63 cycles = 19,656 CPU cycles (~50.12 Hz).

mod bus;
mod c64;
pub mod capture;
pub mod cartridge;
mod cia;
pub mod config;
pub mod d64;
pub mod drive1541;
mod drive1541_bus;
pub mod gcr;
pub mod iec;
pub mod input;
mod keyboard;
pub mod keyboard_map;
pub mod mcp;
mod memory;
pub mod palette;
pub mod prg;
pub mod reu;
pub mod tap;
pub mod tape;
pub mod vic;

pub use bus::C64Bus;
pub use c64::C64;
pub use config::{C64Config, C64Model};
pub use d64::D64;
pub use drive1541::Drive1541;
pub use input::{C64Key, InputQueue};
pub use keyboard::KeyboardMatrix;
pub use memory::C64Memory;
pub use reu::Reu;
pub use vic::Vic;
