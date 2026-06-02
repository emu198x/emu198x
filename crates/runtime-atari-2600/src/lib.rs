//! Atari 2600 / VCS family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_atari_2600::Atari2600Region;
pub use profiles::{Model, profile_for, profiles};
pub use queries::Atari2600SessionQueryProvider;
pub use runtime::Atari2600Runtime;
