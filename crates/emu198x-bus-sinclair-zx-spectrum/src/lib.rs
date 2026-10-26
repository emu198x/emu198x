//! The ZX Spectrum family's wiring: the traits and types its ULAs and
//! edge-connector peripherals are connected through.
//!
//! - [`Ula`] is how a machine drives its ULA, half-cycle by half-cycle,
//!   and [`MemoryBus`] is how the ULA reads the screen from the machine's
//!   memory.
//! - [`FrameTiming`] is the frame a ULA reports, and [`timing`] holds the
//!   frame data of each Spectrum-family video configuration.
//! - [`Peripheral`] and [`MemoryIntercept`] are the hooks an
//!   edge-connector device offers: I/O ports, M1 fetches, a clock, and
//!   memory paged over the machine's own.
//!
//! The crate models no device. ULAs, peripherals and machines implement
//! or call these traits, so any of them can be built and published
//! without the others.

pub mod memory;
pub mod peripheral;
pub mod timing;
pub mod ula;

pub use memory::MemoryBus;
pub use peripheral::{MemoryIntercept, Peripheral};
pub use timing::FrameTiming;
pub use ula::Ula;
