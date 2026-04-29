//! Commodore 64 family metadata and runtime surface.

mod autoload;
pub mod file_loader;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use autoload::{
    C64AutoloadError, C64DiskAutoloadResult, C64TapeAutoloadResult, DEFAULT_DISK_AUTOLOAD_SLOT,
    DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, autoload_basic_disk,
    autoload_basic_disk_with_trace_sink, autoload_basic_tape, autoload_basic_tape_with_trace_sink,
};
pub use machine_commodore_c64::{AudioControls, SidChannel};
pub use profiles::{Model, profile_for, profiles};
pub use queries::C64SessionQueryProvider;
pub use runtime::C64Runtime;
