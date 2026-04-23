//! Nintendo Game Boy family runtime.
//!
//! Owns the Game Boy family metadata catalogue and the
//! `GameBoyRuntime` that bridges the concrete `GameBoy` machine to
//! the `emu198x-shell` `MachineCore` trait. The runtime translates
//! `MediaSet` cartridge inserts into `GameBoy::from_rom`, host input
//! events into joypad button presses, and per-frame execution into
//! `Indexed8` framebuffers + 48 kHz stereo audio packets.
//!
//! Per [within-family-layering](../../wiki/decisions/within-family-layering.md)
//! this crate adds nothing chip-specific — it is the family's seat
//! at the host boundary.

mod profiles;
mod runtime;

pub use profiles::{Model, profile_for, profiles};
pub use runtime::{GameBoyRuntime, GameBoySessionQueryProvider};
