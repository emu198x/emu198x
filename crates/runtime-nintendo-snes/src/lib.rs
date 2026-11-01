//! SNES family runtime with two fixed regional owners and three stable profiles.

pub mod cartridge;
mod debug;
mod machine;
pub use machine::SnesMachine;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_nintendo_snes::{Buttons, Cartridge, Region, Snes};
pub use profiles::{Model, blank, profile_for, profiles, with_cartridge};
pub use queries::SnesSessionQueryProvider;
pub use runtime::SnesRuntime;

#[cfg(test)]
mod restore_regressions;

#[cfg(test)]
mod board_baseline_tests;

#[cfg(test)]
#[allow(
    dead_code,
    reason = "The integration fixture also exports address constants."
)]
#[path = "../tests/common/mod.rs"]
mod test_cartridge;
