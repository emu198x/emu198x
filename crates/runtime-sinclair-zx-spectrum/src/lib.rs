//! Sinclair ZX Spectrum family metadata and runtime layer.
//!
//! This crate owns the Spectrum family's metadata catalogue plus the
//! generic `SpectrumRuntime<M>` that translates `MediaSet`, host input
//! events, and frame/audio sinks into concrete machine operations. Each
//! variant — 48K through Timex TS2068 — exposes a `Spectrum…Runtime`
//! type alias backed by the same generic implementation. The
//! `SpectrumSessionQueryProvider` is generic over `M: SpectrumMachine`,
//! so every variant exposes the shared screen-text / keyboard / tape /
//! timing query surface; variant-specific paths (boot banner, AY
//! state, board issue, SCLD high-res) come from the `SpectrumMachine`
//! trait's variant-query hooks.

mod autoload;
mod basic_loader;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;
mod spectrum_16k;
mod spectrum_48k;
mod spectrum_128k;
mod spectrum_plus;
mod spectrum_plus2;
mod spectrum_plus2a;
mod spectrum_plus2b;
mod spectrum_plus3;
mod variants;

pub use autoload::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, SpectrumAutoloadError,
    SpectrumTapeAutoloadResult, autoload_basic_tape,
};
pub use basic_loader::{
    DEFAULT_BASIC_LOADER_BOOT_FRAMES, LoadBasicError, LoadBasicResult, load_basic_program,
};
pub use common_sinclair_zx_spectrum::{AudioControls, SpeakerChannel};
pub use profiles::{Model, profile_for, profiles};
pub use queries::{SpectrumBootStatus, SpectrumSessionQueryProvider};
pub use runtime::{SpectrumMachine, SpectrumRuntime};
pub use variants::{
    Pentagon128Runtime, ScorpionZS256Runtime, Spectrum16kRuntime, Spectrum48kRuntime,
    Spectrum128kRuntime, SpectrumPlus2ARuntime, SpectrumPlus2BRuntime, SpectrumPlus2Runtime,
    SpectrumPlus3Runtime, SpectrumPlusRuntime, TimexTC2048Runtime, TimexTS2068Runtime,
};
