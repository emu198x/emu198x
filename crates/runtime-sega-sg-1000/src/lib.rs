//! Sega SG-1000 console family metadata and runtime surface.
//!
//! Wraps `common_sega_sg_1000::Sg1000` behind the `MachineCore` trait
//! so the shared `emu198x-shell` runner drives it the same way it
//! drives MSX, ColecoVision, NES, or Spectrum.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_sega_sg_1000::Sg1000Region;
pub use profiles::{Model, profile_for, profiles};
pub use queries::Sg1000SessionQueryProvider;
pub use runtime::Sg1000Runtime;

#[cfg(test)]
mod board_baseline_tests;

mod machine;
pub use machine::Sg1000Machine;
