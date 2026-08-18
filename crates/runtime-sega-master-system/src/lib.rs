//! Sega Master System machine profiles and runtime constructors.
//!
//! One machine, two models: NTSC and PAL. The Game Gear used to live here
//! too, which made it invisible to every crate-derived view of the
//! portfolio and left this the only crate in the workspace building a
//! `MachineId` from a variable rather than a literal (#998). It now ships
//! from `runtime-sega-game-gear`, and the runtime the two share lives in
//! `runtime-sega-master-system-class`.

mod profiles;

pub use profiles::{Model, blank, profile_for, profiles, with_cartridge};
pub use runtime_sega_master_system_class::{Sms, SmsRuntime, SmsSessionQueryProvider, SmsVariant};
