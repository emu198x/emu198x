//! ZX Spectrum 48K machine wrapper.
//!
//! The 48K hardware composition (Z80 + Ferranti ULA + beeper + tape) is
//! shared with the 16K and Spectrum+ via
//! [`common_sinclair_zx_spectrum_48k_class::SpectrumMachineCore`]. This
//! crate exposes the 48K alias plus the host-boundary
//! [`machine::ApplyInputEvent`] extension trait — the only part of the
//! machine surface that depends on `emu198x-shell`.

pub mod machine;

pub use common_sinclair_zx_spectrum::{
    AudioControls, SpeakerChannel,
    keyboard::{KeyboardMatrix, SpectrumKey},
};
pub use common_sinclair_zx_spectrum_48k_class::{Spectrum48kMarker, TapeInput};
pub use ferranti_ula_6c001e::BoardIssue;
pub use machine::{ApplyInputEvent, Spectrum48k};
