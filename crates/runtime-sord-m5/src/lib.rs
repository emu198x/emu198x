//! Sord M5 family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_sord_m5::M5Region;
pub use profiles::{Model, ROM_FIRMWARE_ID, profile_for, profiles};
pub use queries::M5SessionQueryProvider;
pub use runtime::M5Runtime;
