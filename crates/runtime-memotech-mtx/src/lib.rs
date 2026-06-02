//! Memotech MTX family metadata and runtime surface.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_memotech_mtx::MtxModel;
pub use profiles::{Model, ROM_FIRMWARE_ID, profile_for, profiles};
pub use queries::MtxSessionQueryProvider;
pub use runtime::MtxRuntime;
