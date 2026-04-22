//! ZX Spectrum 48K machine-local state.
//!
//! This crate owns the first integrated 48K machine slice for the fresh
//! workspace: memory, the pin-level Z80, the Ferranti ULA, and the first
//! honest frame loop. The keyboard matrix lives in
//! `common-sinclair-zx-spectrum` since every Spectrum variant shares it.

pub mod machine;
pub mod port;

pub use common_sinclair_zx_spectrum::keyboard::{KeyboardMatrix, SpectrumKey};
pub use ferranti_ula_6c001e::BoardIssue;
pub use machine::Spectrum48k;
pub use port::TapeInput;
