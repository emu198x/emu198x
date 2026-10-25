//! Acorn Electron family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{
    ADFS_FIRMWARE_ID, BASIC_FIRMWARE_ID, DISK_SLOTS, Model, OS_FIRMWARE_ID, profile_for, profiles,
};
pub use queries::ElectronSessionQueryProvider;
pub use runtime::ElectronRuntime;
