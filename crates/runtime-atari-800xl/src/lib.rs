//! Atari 800XL family metadata and runtime surface.

mod input;
mod machine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_atari_800xl::{Atari800xlRegion, CartridgeKind};
pub use profiles::{BASIC_FIRMWARE_ID, Model, OS_FIRMWARE_ID, profile_for, profiles};
pub use queries::Atari800xlSessionQueryProvider;
pub use runtime::Atari800xlRuntime;

#[cfg(test)]
mod board_baseline_tests;
