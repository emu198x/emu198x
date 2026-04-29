//! Nintendo NES family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_nintendo_nes::{ApuChannel, AudioControls};
pub use profiles::{Model, profile_for, profiles};
pub use queries::NesSessionQueryProvider;
pub use runtime::NesRuntime;
