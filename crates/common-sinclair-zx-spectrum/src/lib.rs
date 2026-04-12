//! Shared Sinclair ZX Spectrum family building blocks.
//!
//! This crate holds the shared Spectrum-family hardware pieces that are stable
//! across concrete variants: memory contracts, timing data, palette handling,
//! and the shared ULA rendering engine.

pub mod error;
pub mod memory;
pub mod palette;
pub mod timing;
pub mod ula;
pub mod ula_engine;

pub use error::RomImageError;
pub use memory::{MemoryBus, Spectrum48kMemory};
pub use palette::SPECTRUM_PALETTE;
