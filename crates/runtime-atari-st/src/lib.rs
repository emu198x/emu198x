//! Atari ST machine profiles and runtime.
//!
//! Four fixed configurations: the 520ST and the 1040ST, each PAL or NTSC.
//! They differ in RAM and in the master clock. The monitor, colour or
//! monochrome, is a setting of the runtime rather than a model, since it is
//! what is plugged in, not what was bought: [`AtariStRuntime::set_monitor`].
//!
//! TOS is firmware the host supplies, never part of the project: Atari's own
//! TOS or EmuTOS, its free replacement, either 192 KB or 256 KB.

mod debug;
mod input;
mod pointer;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

mod machine;
pub use common_atari_st::Monitor;
pub use machine::AtariSt;
pub use profiles::{Model, TOS_FIRMWARE_ID, blank, profile_for, profiles};
pub use queries::AtariStSessionQueryProvider;
pub use runtime::AtariStRuntime;

#[cfg(test)]
extern crate self as runtime_atari_st;
#[cfg(test)]
mod board_baseline_tests;
