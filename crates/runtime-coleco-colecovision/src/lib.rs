//! Coleco ColecoVision family metadata and runtime surface.
//!
//! Wraps `machine_coleco_colecovision::ColecoVision` behind the
//! `MachineCore` trait so the shared `emu198x-shell` runner drives
//! it the same way it drives MSX, NES, or Spectrum.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_coleco_colecovision::CvRegion;
pub use profiles::{Model, profile_for, profiles};
pub use queries::CvSessionQueryProvider;
pub use runtime::CvRuntime;
