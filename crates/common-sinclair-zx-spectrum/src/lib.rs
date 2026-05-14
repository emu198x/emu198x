//! Shared Sinclair ZX Spectrum family building blocks.
//!
//! This crate holds the shared Spectrum-family hardware pieces that are stable
//! across concrete variants: memory contracts, timing data, palette handling,
//! beeper/tape audio, pulse-driven tape playback, and the shared ULA rendering
//! engine.

pub mod audio;
pub mod driver;
pub mod error;
pub mod keyboard;
pub mod memory;
pub mod palette;
pub mod peripheral;
pub mod snapshot;
pub mod tape;
pub mod timing;
pub mod ula;
pub mod ula_engine;

pub use audio::{AudioControls, BeeperAudio, SpeakerChannel, SpeakerMixer};
pub use driver::SpectrumDriver;
pub use error::RomImageError;
pub use keyboard::{KeyboardMatrix, SpectrumKey};
pub use memory::{Bank16K, MemoryBus, Spectrum16kMemory, Spectrum48kMemory};
pub use palette::SPECTRUM_PALETTE;
pub use peripheral::Peripheral;
pub use snapshot::{
    Paged128kMemory, SnapshotBankTarget, apply_48k_pages, apply_128k_bank_pages,
    apply_ay_registers, apply_z80_registers,
};
pub use tape::{TapeBlock, TapePlayer, TapeSpan};
