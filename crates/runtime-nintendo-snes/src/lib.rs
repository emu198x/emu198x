//! Super NES / Super Famicom machine profiles and runtime.
//!
//! One machine, three models: the NTSC Super NES, the PAL Super Nintendo
//! and the Super Famicom. They differ in the television they drive and the
//! master clock that follows from it; the Super Famicom is the Japanese
//! NTSC console. The runtime, input, queries, debug surface and snapshot
//! all live in this crate.

pub mod cartridge;
mod debug;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_nintendo_snes::{Buttons, Cartridge, Region, Snes};
pub use profiles::{Model, blank, profile_for, profiles, with_cartridge};
pub use queries::SnesSessionQueryProvider;
pub use runtime::SnesRuntime;
