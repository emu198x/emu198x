//! Commodore VIC-20 family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_commodore_vic_20::Vic20Model;
pub use profiles::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, profile_for, profiles,
};
pub use queries::Vic20SessionQueryProvider;
pub use runtime::Vic20Runtime;
// Re-exported so the VIC-20's public surface is unchanged by the move.
pub use emu198x_esp_at_modem::{BitBangSerial, EspAtModem, EspAtTcpBridge};
