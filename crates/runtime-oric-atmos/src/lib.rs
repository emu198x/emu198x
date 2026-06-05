//! Oric-1 / Atmos family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_oric_atmos::OricModel;
pub use profiles::{BIOS_FIRMWARE_ID, Model, profile_for, profiles};
pub use queries::OricSessionQueryProvider;
pub use runtime::OricRuntime;
