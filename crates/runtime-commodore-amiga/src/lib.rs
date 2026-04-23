//! Commodore Amiga family metadata and runtime surface.

mod profiles;
mod runtime;
pub use machine_commodore_amiga_ocs::RamConfig;
pub use profiles::{
    A500_PAL_CCK_HZ, A500_PAL_FRAME_CCKS, A500_PAL_FRAME_TICKS, Model, profile_for, profiles,
};
pub use runtime::{AmigaRuntime, AmigaSessionQueryProvider, DISPLAY_HEIGHT, DISPLAY_WIDTH};
