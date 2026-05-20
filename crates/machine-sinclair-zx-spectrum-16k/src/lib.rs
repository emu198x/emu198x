//! ZX Spectrum 16K machine wrapper.
//!
//! The 16K is electrically identical to the 48K up to the 32 KiB RAM
//! that's missing — same Ferranti 6C001E ULA, same Z80, same ROM, same
//! keyboard matrix, same timing. The hardware composition lives in
//! [`common_sinclair_zx_spectrum_48k_class::SpectrumMachineCore`]; this
//! crate exposes only the 16K type alias and re-exports the surface
//! callers need.

pub mod machine;

pub use common_sinclair_zx_spectrum::{
    AudioControls, SpeakerChannel,
    keyboard::{KeyboardMatrix, SpectrumKey},
};
pub use common_sinclair_zx_spectrum_48k_class::{Spectrum16kMarker, TapeInput};
pub use ferranti_ula_6c001e::UlaRevision;
pub use machine::Spectrum16K;
