//! Sega Master System / Game Gear family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_sega_master_system::SmsVariant;
pub use profiles::{Model, profile_for, profiles};
pub use queries::SmsSessionQueryProvider;
pub use runtime::SmsRuntime;
