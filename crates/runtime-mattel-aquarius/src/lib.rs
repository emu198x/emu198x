//! Mattel Aquarius family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{Model, profile_for, profiles};
pub use queries::AquariusSessionQueryProvider;
pub use runtime::AquariusRuntime;
