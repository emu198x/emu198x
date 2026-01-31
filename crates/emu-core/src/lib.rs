//! Core traits and types for cycle-accurate emulation.
//!
//! Everything ticks at the master crystal frequency. All component timing
//! derives from this. No exceptions.

mod bus;
mod clock;
mod cpu;
mod observable;
mod tickable;
mod ticks;

pub use bus::{Bus, ReadResult, SimpleBus};
pub use clock::MasterClock;
pub use cpu::Cpu;
pub use observable::{Observable, Value};
pub use tickable::Tickable;
pub use ticks::Ticks;
