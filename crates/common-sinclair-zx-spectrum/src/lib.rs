//! Shared Sinclair ZX Spectrum family building blocks.
//!
//! This crate starts with the pieces that are stable across the first Spectrum
//! implementation pass and do not require a fake CPU or ULA shell: timing
//! constants and the 48K memory map.

pub mod error;
pub mod memory;
pub mod timing;

pub use error::RomImageError;
pub use memory::{MemoryBus, Spectrum48kMemory};
