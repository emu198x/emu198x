//! Acorn Atom family metadata and runtime surface.

mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{BIOS_FIRMWARE_ID, Model, profile_for, profiles};
pub use queries::AtomSessionQueryProvider;
pub use runtime::AtomRuntime;
