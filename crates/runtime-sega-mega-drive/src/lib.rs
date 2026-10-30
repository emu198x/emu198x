//! Sega Mega Drive / Genesis machine profiles and runtime.
//!
//! Three fixed regional boards share common wiring and this family runtime.
//! Profiles, version-2 saves and Power Base Converter configurations retain
//! their existing behaviour.

pub mod cartridge;
mod debug;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_sega_mega_drive::{Buttons, Device, Region};
pub use machine::MegaDrive;
mod machine;
pub use profiles::{Model, blank, profile_for, profiles, with_cartridge};
pub use queries::MegaDriveSessionQueryProvider;
pub use runtime::{MASTER_SYSTEM, MEGA_DRIVE, MegaDriveRuntime};

#[cfg(test)]
mod board_baseline_tests;

#[cfg(test)]
mod board_identity_tests;
