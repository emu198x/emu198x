//! Commodore 64 family metadata and runtime surface.

mod autoload;
mod basic_loader;
mod drives;
pub mod file_loader;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;
mod typing;

pub use autoload::{
    C64AutoloadError, C64DiskAutoloadResult, C64TapeAutoloadResult,
    DEFAULT_DISK_1581_AUTOLOAD_SLOT, DEFAULT_DISK_AUTOLOAD_SLOT, DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT,
    DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, autoload_basic_disk, autoload_basic_disk_1581,
    autoload_basic_disk_and_run, autoload_basic_disk_with_trace_sink, autoload_basic_tape,
    autoload_basic_tape_with_trace_sink,
};
pub use basic_loader::{
    DEFAULT_BASIC_LOADER_BOOT_FRAMES, LoadBasicError, LoadBasicResult, load_basic_program,
    load_basic_source,
};
pub use drives::{DriveKind, IecDrive};
pub use input::{key_name_is_valid, keys_for_char};
pub use machine_commodore_c64::{AudioControls, SidChannel};
pub use profiles::{Model, profile_for, profiles};
pub use queries::C64SessionQueryProvider;
pub use runtime::C64Runtime;
pub use typing::{
    DEFAULT_INTER_CHAR_FRAMES, DEFAULT_KEY_HOLD_FRAMES, DEFAULT_TYPE_SETTLE_FRAMES,
    MAX_KEY_HOLD_FRAMES, press_key, type_string,
};
