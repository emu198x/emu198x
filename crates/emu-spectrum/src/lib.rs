//! Cycle-accurate ZX Spectrum emulator.
//!
//! Architectured for the full Spectrum family (48K through Next), but v1
//! implements the 48K model only. The system ticks at 14 MHz (master crystal);
//! the ULA runs at 7 MHz and the CPU at 3.5 MHz, both derived by integer
//! division.

mod beeper;
mod bus;
#[cfg(feature = "native")]
pub mod capture;
mod config;
pub mod input;
mod keyboard;
#[cfg(feature = "native")]
pub mod keyboard_map;
#[cfg(feature = "native")]
pub mod mcp;
mod memory;
pub use format_sna as sna;
mod spectrum;
pub use format_spectrum_tap as tap;
pub mod tape;
pub use format_tzx as tzx;
pub mod tzx_signal;
pub use format_z80 as z80;

pub use beeper::BeeperState;
pub use bus::SpectrumBus;
pub use config::{SpectrumConfig, SpectrumModel};
pub use input::{InputQueue, SpectrumKey};
pub use keyboard::KeyboardState;
pub use memory::{Memory48K, Memory128K, MemoryPlus3, SpectrumMemory};
pub use sinclair_ula::Ula;
pub use sna::load_sna;
pub use spectrum::Spectrum;
pub use tap::TapFile;
pub use tape::TapeDeck;
pub use tzx::TzxFile;
pub use tzx_signal::TzxSignal;
pub use z80::load_z80;
