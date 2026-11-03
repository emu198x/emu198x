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
mod board_runtime;
mod debug;
mod family_runtime;
mod input;
mod pentagon_128;
mod profiles;
mod profiling;
mod queries;
mod runtime;
mod scorpion_zs256;
mod snapshot;
mod spectrum_128k;
mod spectrum_plus2;
mod spectrum_plus2a;
mod spectrum_plus2b;
mod spectrum_plus3;
mod timex_tc2048;
mod timex_ts2068;
mod variants;

pub use autoload::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, SpectrumAutoloadError,
    SpectrumTapeAutoloadResult, autoload_basic_tape, tap_key, tap_symbol_combo,
};
pub use basic_loader::{
    DEFAULT_BASIC_LOADER_BOOT_FRAMES, LoadBasicError, LoadBasicResult, load_basic_program,
    load_basic_program_with_writer,
};
pub use common_sinclair_zx_spectrum::BlockSeek;
pub use common_sinclair_zx_spectrum::{AudioControls, SpeakerChannel};
pub use family_runtime::{SpectrumLiveAccess, SpectrumRuntimeKind};
pub use input::{ApplyInputEvent, ApplyKempstonEvent, NMI_KEY};
pub use profiles::{Model, profile_for, profiles};
pub use queries::{SpectrumBootStatus, SpectrumSessionQueryProvider};
pub use runtime::{SD_CARD_SLOT, SpectrumMachine, SpectrumRuntime};
pub use variants::{
    Pentagon128Runtime, ScorpionZS256Runtime, Spectrum128kRuntime, SpectrumIssue1Ram16kRuntime,
    SpectrumIssue1Ram48kRuntime, SpectrumIssue2Ram16kRuntime, SpectrumIssue2Ram48kRuntime,
    SpectrumIssue3BRam16kRuntime, SpectrumIssue3BRam48kRuntime, SpectrumIssue3Ram16kRuntime,
    SpectrumIssue3Ram48kRuntime, SpectrumIssue4ARam16kRuntime, SpectrumIssue4ARam48kRuntime,
    SpectrumIssue4BRam16kRuntime, SpectrumIssue4BRam48kRuntime,
    SpectrumIssue5Ula6c001e6Ram16kRuntime, SpectrumIssue5Ula6c001e6Ram48kRuntime,
    SpectrumIssue5Ula6c001e7Ram16kRuntime, SpectrumIssue5Ula6c001e7Ram48kRuntime,
    SpectrumIssue6ARam16kRuntime, SpectrumIssue6ARam48kRuntime, SpectrumPlus2ARuntime,
    SpectrumPlus2BRuntime, SpectrumPlus2Runtime, SpectrumPlus3Runtime, TimexTC2048Runtime,
    TimexTC2068Runtime, TimexTS2068Runtime,
};

mod signal;
