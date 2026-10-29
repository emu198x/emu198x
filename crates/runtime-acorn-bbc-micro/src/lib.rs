//! Acorn BBC Micro family metadata and runtime surface.

mod input;
mod machine;
pub use machine::BbcMachine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{
    BASIC_FIRMWARE_ID, BPLUS_MOS_FIRMWARE_ID, DFS_FIRMWARE_ID, DISK_SLOTS, FONT_FIRMWARE_ID,
    MASTER_MOS_FIRMWARE_ID, MASTER_ROMS, MOS_FIRMWARE_ID, MasterRom, Model, profile_for, profiles,
};
pub use queries::BbcMicroSessionQueryProvider;
pub use runtime::BbcMicroRuntime;
