//! Tatung Einstein family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_tatung_einstein::EinsteinRegion;
pub use profiles::{Model, ROM_FIRMWARE_ID, profile_for, profiles};
pub use queries::EinsteinSessionQueryProvider;
pub use runtime::EinsteinRuntime;
