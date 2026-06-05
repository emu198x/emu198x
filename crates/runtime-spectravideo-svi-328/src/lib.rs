//! Spectravideo SVI-328 family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_spectravideo_svi_328::SviRegion;
pub use profiles::{BIOS_FIRMWARE_ID, Model, profile_for, profiles};
pub use queries::Svi328SessionQueryProvider;
pub use runtime::Svi328Runtime;
