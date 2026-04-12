//! ZX Spectrum 48K machine-local state.
//!
//! This crate owns the first machine-layer slice for the fresh workspace:
//! 48K memory, keyboard matrix state, tape EAR input state, and the `$FE`
//! port behaviour that ties those together.

pub mod keyboard;
pub mod machine;
pub mod port;

pub use keyboard::{KeyboardMatrix, SpectrumKey};
pub use machine::{BoardIssue, Spectrum48k};
pub use port::{PortFeState, TapeInput};
