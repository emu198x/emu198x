//! Sinclair ZX80 family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{Model, ROM_FIRMWARE_ID, profile_for, profiles};
pub use queries::Zx80SessionQueryProvider;
pub use runtime::Zx80Runtime;
