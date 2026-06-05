//! Atari 5200 family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_atari_5200::Atari5200Region;
pub use profiles::{BIOS_FIRMWARE_ID, Model, profile_for, profiles};
pub use queries::Atari5200SessionQueryProvider;
pub use runtime::Atari5200Runtime;
