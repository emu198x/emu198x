//! Acorn BBC Micro family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use profiles::{MOS_FIRMWARE_ID, Model, profile_for, profiles};
pub use queries::BbcMicroSessionQueryProvider;
pub use runtime::BbcMicroRuntime;
