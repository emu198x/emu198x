//! Commodore Amiga family metadata and runtime surface.

pub mod amiga_model;
mod cpu_trace;
mod debug;
mod input;
mod live_access;
mod profiles;
mod queries;
mod runtime;
mod snapshot;
mod variants;

pub use amiga_model::{
    Accelerator, ChipsetKind, CpuKind, ECS_AGA_CHIP_RAM_BYTES, FAT_AGNUS_CHIP_RAM_BYTES,
    FATTER_AGNUS_CHIP_RAM_BYTES, KIB, MIB, OCS_AGNUS_CHIP_RAM_BYTES,
};
pub use cpu_trace::CpuTraceEntry;
pub use live_access::{
    AmigaLiveAccess, Bplcon0LogEntry, CpuSnapshot, CustomWriteEntry, DskLogEntry, PaletteLogEntry,
    RegReadLogEntry, WatchLogEntry,
};
pub use machine_commodore_amiga_ocs::{AudioControls, PaulaChannel, RamConfig};
pub use profiles::{
    A500_NTSC_CCK_HZ, A500_NTSC_FRAME_CCKS, A500_NTSC_FRAME_TICKS, A500_PAL_CCK_HZ,
    A500_PAL_FRAME_CCKS, A500_PAL_FRAME_TICKS, Model, profile_for, profiles,
};
pub use queries::AmigaSessionQueryProvider;
pub use runtime::{AmigaRuntime, DISPLAY_HEIGHT, DISPLAY_WIDTH};
pub use variants::{
    AmigaA1200Runtime, AmigaEcsRuntime, AmigaMachine, AmigaOcsRuntime, AmigaRuntimeKind,
};
