//! Commodore 64 family metadata and runtime surface.

mod autoload;
mod basic_loader;
mod c128;
mod drives;
mod family_runtime;
pub mod file_loader;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;
mod typing;
mod variants;

pub use autoload::{
    C64AutoloadError, C64DiskAutoloadResult, C64TapeAutoloadResult,
    DEFAULT_DISK_1581_AUTOLOAD_SLOT, DEFAULT_DISK_AUTOLOAD_SLOT, DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT,
    DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, autoload_basic_disk, autoload_basic_disk_1581,
    autoload_basic_disk_and_run, autoload_basic_disk_with_trace_sink, autoload_basic_tape,
    autoload_basic_tape_on, autoload_basic_tape_with_trace_sink,
};
pub use basic_loader::{
    DEFAULT_BASIC_LOADER_BOOT_FRAMES, LoadBasicError, LoadBasicResult, load_basic_program,
    load_basic_source,
};
pub use c128::{C128RomSet, C128Runtime};
pub use drives::{DriveKind, IecDrive};
// Re-exported so a host can drive the user port without naming the
// machine-neutral modem crate directly.
pub use common_commodore_c64::{AudioControls, SidChannel};
pub use emu198x_esp_at_modem::{BitBangSerial, EspAtModem, EspAtTcpBridge};
pub use family_runtime::{AsC64Runtime, C64RuntimeKind, C64Variant};
pub use input::{key_name_is_valid, keys_for_char};
pub use profiles::{C128Model, Model, c128_profile_for, c128_profiles, profile_for, profiles};
pub use queries::C64SessionQueryProvider;
pub use runtime::{C64FamilyRuntime, C64Runtime, SUPERCPU_FIRMWARE_ID};
pub use typing::{
    DEFAULT_INTER_CHAR_FRAMES, DEFAULT_KEY_HOLD_FRAMES, DEFAULT_TYPE_SETTLE_FRAMES,
    MAX_KEY_HOLD_FRAMES, press_key, type_string,
};
pub use variants::{C64FamilyMachine, C64FamilyModel, C64RomSet};

mod signal;

#[cfg(test)]
mod board_baseline_tests;

mod boards;
pub use boards::{C64, C128};
