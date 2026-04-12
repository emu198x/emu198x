//! ZX Spectrum 48K machine-local state.
//!
//! This crate owns the first integrated 48K machine slice for the fresh
//! workspace: memory, keyboard state, the pin-level Z80, the Ferranti ULA,
//! and the first honest frame loop.

pub mod keyboard;
pub mod machine;
pub mod port;

pub use ferranti_ula_6c001e::BoardIssue;
pub use keyboard::{KeyboardMatrix, SpectrumKey};
pub use machine::Spectrum48k;
pub use port::TapeInput;
