//! Sega SG-1000 / SC-3000 family metadata and runtime surface.
//!
//! Wraps `machine_sega_sg_1000::Sg1000` behind the `MachineCore` trait
//! so the shared `emu198x-shell` runner drives it the same way it
//! drives MSX, ColecoVision, NES, or Spectrum.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_sega_sg_1000::Sg1000Region;
pub use profiles::{Model, profile_for, profiles};
pub use queries::Sg1000SessionQueryProvider;
pub use runtime::Sg1000Runtime;
