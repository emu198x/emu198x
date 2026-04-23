//! Commodore 64 family metadata and runtime surface.

mod autoload;
pub mod file_loader;
mod profiles;
mod runtime;

pub use autoload::{
    C64AutoloadError, C64DiskAutoloadResult, C64TapeAutoloadResult, DEFAULT_DISK_AUTOLOAD_SLOT,
    DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, autoload_basic_disk,
    autoload_basic_disk_with_trace_sink, autoload_basic_tape, autoload_basic_tape_with_trace_sink,
};
pub use profiles::{Model, profile_for, profiles};
pub use runtime::{C64Runtime, C64SessionQueryProvider};
