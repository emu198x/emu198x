//! Nintendo Game Boy family runtime.
//!
//! Owns the Game Boy family metadata catalogue and the
//! `GameBoyRuntime` that bridges the concrete `GameBoy` machine to
//! the `emu198x-shell` `MachineCore` trait. The runtime translates
//! `MediaSet` cartridge inserts into `GameBoy::from_rom`, host input
//! events into joypad button presses, and per-frame execution into
//! `Indexed8` framebuffers (`Rgba8888` on the Game Boy Color) + 48 kHz
//! stereo audio packets.
//!
//! Per [within-family-layering](../../../knowledge/decisions/within-family-layering.md)
//! this crate adds nothing chip-specific — it is the family's seat
//! at the host boundary.

mod input;
mod machine;
pub use machine::GameBoyMachine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_nintendo_game_boy::{ApuChannel, AudioControls};
pub use profiles::{
    CGB_BOOT_ROM_ID, DMG_BOOT_ROM_ID, DMG0_BOOT_ROM_ID, MGB_BOOT_ROM_ID, Model, profile_for,
    profiles,
};
pub use queries::GameBoySessionQueryProvider;
pub use runtime::GameBoyRuntime;
