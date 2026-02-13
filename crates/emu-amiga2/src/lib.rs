//! Cycle-accurate Amiga emulator with variant support.
//!
//! Uses emu-m68k's word-level M68kBus trait for proper bus contention.
//! CPU always ticks — chip RAM DMA contention flows through wait_cycles.
//!
//! Timing (PAL):
//! - Crystal: 28,375,160 Hz
//! - CPU: crystal / 4 = 7,093,790 Hz
//! - Colour clock (CCK): crystal / 8 = 3,546,895 Hz
//! - CIA E-clock: crystal / 40 = 709,379 Hz
//! - One frame: 312 lines x 227 CCKs x 8 = 566,208 crystal ticks

pub mod agnus;
mod amiga;
mod blitter;
mod bus;
pub mod capture;
mod cia;
pub mod config;
mod copper;
mod custom_regs;
pub mod denise;
pub mod input;
mod keyboard;
mod memory;
mod paula;

pub use amiga::Amiga;
pub use bus::AmigaBus;
pub use config::AmigaConfig;
