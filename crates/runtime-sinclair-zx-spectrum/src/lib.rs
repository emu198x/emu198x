//! Sinclair ZX Spectrum family metadata and runtime layer.
//!
//! This crate owns the Spectrum family's metadata catalogue plus the
//! generic `SpectrumRuntime<M>` that translates `MediaSet`, host input
//! events, and frame/audio sinks into concrete machine operations. Each
//! variant — 48K through Timex TS2068 — exposes a `Spectrum…Runtime`
//! type alias backed by the same generic implementation. The 48K
//! additionally wears the `SpectrumSessionQueryProvider` for ROM-glyph
//! text extraction and boot detection.

mod autoload;
mod profiles;
mod spectrum_48k;
mod spectrum_runtime;
mod variants;

pub use autoload::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, SpectrumAutoloadError,
    SpectrumTapeAutoloadResult, autoload_basic_tape,
};
pub use profiles::{Model, profile_for, profiles};
pub use spectrum_48k::SpectrumSessionQueryProvider;
pub use spectrum_runtime::{SpectrumMachine, SpectrumRuntime};
pub use variants::{
    Pentagon128Runtime, ScorpionZS256Runtime, Spectrum128kRuntime, Spectrum48kRuntime,
    SpectrumPlusRuntime, TimexTC2048Runtime, TimexTS2068Runtime,
};

