//! Core traits and types for cycle-accurate emulation.
//!
//! Everything ticks at the master crystal frequency. All component timing
//! derives from this. No exceptions.

mod bus;
mod clock;
mod cpu;
mod machine;
#[cfg(feature = "mcp")]
pub mod mcp;
mod observable;
mod tickable;
mod ticks;
#[cfg(feature = "renderer")]
pub mod renderer;
#[cfg(feature = "renderer")]
pub mod runner;
#[cfg(feature = "video")]
pub mod video;

pub use bus::{Bus, ReadResult, SimpleBus, WordBus};
pub use clock::MasterClock;
pub use cpu::Cpu;
pub use machine::{AudioFrame, Machine};
pub use observable::{Observable, Value};
pub use tickable::Tickable;
pub use ticks::Ticks;
