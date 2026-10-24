//! Sega Mega Drive / Genesis machine profiles and runtime.
//!
//! One machine, three models: the Japanese Mega Drive, the American
//! Genesis and the European Mega Drive. They differ in the version
//! register a game reads and in the television they drive. With one machine
//! there is no class to share, so the runtime, input, queries and snapshot
//! the Master System splits across two crates all live here.

pub mod cartridge;
mod debug;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_sega_mega_drive::{Buttons, Device, MegaDrive, Region};
pub use profiles::{Model, blank, profile_for, profiles, with_cartridge};
pub use queries::MegaDriveSessionQueryProvider;
pub use runtime::{MASTER_SYSTEM, MEGA_DRIVE, MegaDriveRuntime};
