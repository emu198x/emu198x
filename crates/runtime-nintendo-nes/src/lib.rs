//! Nintendo NES family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_nintendo_nes::{ApuChannel, AudioControls, InputDevices};
pub use profiles::{FDS_BIOS_FIRMWARE_ID, FDS_DISK_SLOT, Model, profile_for, profiles};
pub use queries::NesSessionQueryProvider;
pub use runtime::{FDS_MAPPER_NUMBER, NesRuntime};

mod signal;

#[cfg(test)]
mod board_baseline_tests;
