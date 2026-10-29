//! Atari 7800 family metadata and runtime surface.

mod input;
mod machine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_atari_7800::Atari7800Region;
pub use profiles::{Model, profile_for, profiles};
pub use queries::Atari7800SessionQueryProvider;
pub use runtime::Atari7800Runtime;

#[cfg(test)]
mod board_baseline_tests;
