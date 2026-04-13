//! Shared Sinclair ZX Spectrum family building blocks.
//!
//! This crate holds the shared Spectrum-family hardware pieces that are stable
//! across concrete variants: memory contracts, timing data, palette handling,
//! beeper/tape audio, pulse-driven tape playback, and the shared ULA rendering
//! engine.

pub mod audio;
pub mod error;
pub mod memory;
pub mod palette;
pub mod tape;
pub mod timing;
pub mod ula;
pub mod ula_engine;

pub use audio::BeeperAudio;
pub use error::RomImageError;
pub use memory::{MemoryBus, Spectrum48kMemory};
pub use palette::SPECTRUM_PALETTE;
pub use tape::{TapeBlock, TapePlayer, TapeSpan};
