//! Cycle-accurate ZX Spectrum emulator.
//!
//! Architectured for the full Spectrum family (48K through Next), but v1
//! implements the 48K model only. The system ticks at 14 MHz (master crystal);
//! the ULA runs at 7 MHz and the CPU at 3.5 MHz, both derived by integer
//! division.

mod beeper;
mod bus;
pub mod capture;
mod config;
pub mod input;
mod keyboard;
pub mod keyboard_map;
pub mod mcp;
mod memory;
mod palette;
pub mod sna;
mod spectrum;
pub mod tap;
pub mod tape;
mod ula;
mod video;

pub use beeper::BeeperState;
pub use bus::SpectrumBus;
pub use config::{SpectrumConfig, SpectrumModel};
pub use input::{InputQueue, SpectrumKey};
pub use keyboard::KeyboardState;
pub use memory::{Memory48K, SpectrumMemory};
pub use sna::load_sna;
pub use spectrum::Spectrum;
pub use tap::TapFile;
pub use tape::TapeDeck;
pub use ula::Ula;
pub use video::SpectrumVideo;
