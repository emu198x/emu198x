//! Acorn Electron family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{BASIC_FIRMWARE_ID, Model, OS_FIRMWARE_ID, profile_for, profiles};
pub use queries::ElectronSessionQueryProvider;
pub use runtime::ElectronRuntime;
