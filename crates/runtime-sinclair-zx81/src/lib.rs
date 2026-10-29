//! Sinclair ZX81 family metadata and runtime surface.

mod input;
mod machine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_sinclair_zx81::HiResBoard;
pub use profiles::{Model, ROM_FIRMWARE_ID, profile_for, profiles};
pub use queries::Zx81SessionQueryProvider;
pub use runtime::{HIRES_FIRMWARE_ID, Zx81Runtime};
