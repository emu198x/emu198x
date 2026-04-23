//! Nintendo NES family metadata and runtime surface.

mod profiles;
mod runtime;

pub use profiles::{Model, profile_for, profiles};
pub use runtime::{NesRuntime, NesSessionQueryProvider};
