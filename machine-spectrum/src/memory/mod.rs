//! Memory subsystem for ZX Spectrum emulation.
//!
//! This module provides:
//! - `Ula` - ULA chip emulation (border, keyboard, beeper, contention, floating bus)
//! - `MemoryModel` - Trait for different memory configurations
//! - `Memory16K` - 16K Spectrum memory model
//! - `Memory48K` - 48K Spectrum memory model

mod memory_16k;
mod memory_48k;
mod model;
mod ula;

pub use memory_16k::Memory16K;
pub use memory_48k::Memory48K;
pub use model::MemoryModel;
pub use ula::{T_STATES_PER_FRAME_48K, Ula};
