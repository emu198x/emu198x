//! Coleco ColecoVision family metadata and runtime surface.
//!
//! Wraps the concrete ColecoVision boards behind the
//! `MachineCore` trait so the shared `emu198x-shell` runner drives
//! it the same way it drives MSX, NES, or Spectrum.

mod input;
mod machine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_coleco_colecovision::CvRegion;
pub use machine::CvMachine;
pub use profiles::{BIOS_FIRMWARE_ID, Model, profile_for, profiles};
pub use queries::CvSessionQueryProvider;
pub use runtime::CvRuntime;
