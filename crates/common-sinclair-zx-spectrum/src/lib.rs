//! Shared Sinclair ZX Spectrum family building blocks.
//!
//! This crate holds the shared Spectrum-family hardware pieces that are stable
//! across concrete variants: memory, palette handling, beeper/tape audio,
//! pulse-driven tape playback and snapshots.
//!
//! The wiring ULAs and peripherals implement — the `Ula`, `MemoryBus`,
//! `Peripheral` and `MemoryIntercept` traits and the frame timing data — is
//! `emu198x-bus-sinclair-zx-spectrum`'s, and the shared ULA rendering engine
//! is `emu198x-sinclair-zx-spectrum-ula`. Both are re-exported here under
//! their old module names (`ula`, `peripheral`, `timing`, `ula_engine`).

pub mod audio;
pub mod driver;
pub mod error;
pub mod io_trace;
pub mod keyboard;
pub mod memory;
pub mod memory_watch;
pub mod palette;
pub use emu198x_bus_sinclair_zx_spectrum::peripheral;
pub mod screen_text;
mod serde_skip_audit;
pub mod snapshot;
pub mod tape;
pub mod tape_recorder;
pub mod timing;
pub use emu198x_bus_sinclair_zx_spectrum::ula;
pub use emu198x_sinclair_zx_spectrum_ula as ula_engine;

pub use audio::{AudioControls, BeeperAudio, SpeakerChannel, SpeakerMixer};
// Relocated into the shared AY chip crate so the whole AY fleet captures
// register writes through one tracer; re-exported here for the Spectrum
// code that already names `common_sinclair_zx_spectrum::AyWriteWatch`.
pub use driver::SpectrumDriver;
pub use emu198x_gi_ay_3_8912::{AyWriteRecord, AyWriteWatch, DEFAULT_AY_WATCH_CAP};
pub use error::RomImageError;
pub use keyboard::{KeyboardMatrix, SpectrumKey};
pub use memory::{Bank16K, MemoryBus, Spectrum16kMemory, Spectrum48kMemory};
pub use memory_watch::{DEFAULT_WATCH_CAP, MemoryWriteRecord, MemoryWriteWatch};
pub use palette::SPECTRUM_PALETTE;
pub use peripheral::Peripheral;
pub use snapshot::{
    Paged128kMemory, SnapshotBankTarget, apply_48k_pages, apply_128k_bank_pages,
    apply_ay_registers, apply_z80_registers,
};
pub use tape::{BlockSeek, SpectrumTapePlayer, TapeBlock, TapePlayer, TapeSpan};
pub use tape_recorder::TapeRecorder;
