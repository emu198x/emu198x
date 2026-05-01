//! Commodore Amiga family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;
mod variants;

pub use machine_commodore_amiga_ocs::{AudioControls, PaulaChannel, RamConfig};
pub use profiles::{
    A500_NTSC_CCK_HZ, A500_NTSC_FRAME_CCKS, A500_NTSC_FRAME_TICKS, A500_PAL_CCK_HZ,
    A500_PAL_FRAME_CCKS, A500_PAL_FRAME_TICKS, Model, profile_for, profiles,
};
pub use queries::AmigaSessionQueryProvider;
pub use runtime::{AmigaRuntime, DISPLAY_HEIGHT, DISPLAY_WIDTH};
pub use variants::{AmigaEcsRuntime, AmigaMachine, AmigaOcsRuntime};
