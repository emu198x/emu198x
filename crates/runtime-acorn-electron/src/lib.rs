//! Acorn Electron family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{Model, profile_for, profiles, BASIC_FIRMWARE_ID, OS_FIRMWARE_ID};
pub use queries::ElectronSessionQueryProvider;
pub use runtime::ElectronRuntime;
