//! Amstrad CPC family metadata and runtime surface.

mod debug;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use input::key_for_name;
pub use profiles::{Model, ROM_FIRMWARE_ID, profile_for, profiles};
pub use queries::AmstradCpcSessionQueryProvider;
pub use runtime::AmstradCpcRuntime;
