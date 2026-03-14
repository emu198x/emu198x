//! Cycle-accurate Commodore 64 emulator.
//!
//! The C64 master clock ticks at CPU cycle rate (985,248 Hz PAL). All
//! components (VIC-II, CIA, SID) tick at this rate. One frame is 312
//! raster lines x 63 cycles = 19,656 CPU cycles (~50.12 Hz).

mod bus;
mod c64;
#[cfg(feature = "native")]
pub mod capture;
pub mod cartridge;
pub mod config;
pub use format_d64 as d64;
pub mod drive1541;
mod drive1541_bus;
pub use format_gcr as gcr;
pub mod iec;
pub mod input;
mod keyboard;
#[cfg(feature = "native")]
pub mod keyboard_map;
#[cfg(feature = "native")]
pub mod mcp;
mod memory;
pub use format_prg as prg;
pub use mos_vic_ii::palette;
pub mod reu;
pub use format_c64_tap as tap;
pub mod tape;
pub use mos_vic_ii as vic;

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
