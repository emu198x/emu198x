//! Cycle-accurate Amiga 500 PAL emulator.
//!
//! The Amiga master clock ticks at 28,375,160 Hz (PAL crystal). Components
//! derive their timing from this:
//! - CPU: crystal / 4 = 7,093,790 Hz (68000, gated by DMA)
//! - Colour clock (CCK): crystal / 8 = 3,546,895 Hz (DMA slot boundary)
//! - CIA E-clock: crystal / 40 = 709,379 Hz
//!
//! One frame = 312 lines × 227 CCKs × 8 = 566,208 crystal ticks.

mod agnus;
mod amiga;
mod blitter;
mod bus;
pub mod capture;
mod cia;
mod config;
mod copper;
mod custom_regs;
pub mod denise;
pub mod input;
pub mod mcp;
mod memory;
mod paula;

pub use amiga::Amiga;
pub use bus::AmigaBus;
pub use config::AmigaConfig;
