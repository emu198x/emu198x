//! Commodore PET family metadata and runtime surface.

mod input;
pub mod machine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, EDITOR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model,
    profile_for, profiles,
};
pub use queries::PetSessionQueryProvider;
pub use runtime::PetRuntime;

#[cfg(test)]
mod save_regressions;

#[cfg(test)]
mod board_baseline_tests;
