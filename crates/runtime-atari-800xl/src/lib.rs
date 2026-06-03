//! Atari 800XL family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_atari_800xl::Atari800xlRegion;
pub use profiles::{BASIC_FIRMWARE_ID, Model, OS_FIRMWARE_ID, profile_for, profiles};
pub use queries::Atari800xlSessionQueryProvider;
pub use runtime::Atari800xlRuntime;
