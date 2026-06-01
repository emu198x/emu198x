//! MSX1 family metadata and runtime surface.
//!
//! Wraps `machine_msx::Msx` behind the `MachineCore` trait so the
//! shared `emu198x-shell` runner (HeadlessSession, MCP server,
//! script runner) can drive it the same way it drives Spectrum,
//! Amiga, or NES.
//!
//! The MSX cannot run without a 32 KB BIOS image — the runtime models
//! this by holding the machine in `Option`: `MsxRuntime::blank()`
//! constructs a stub that responds to `load_media(firmware=...)` to
//! finish bootstrapping.

mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use machine_msx::{MapperType, MsxRegion};
pub use profiles::{Model, profile_for, profiles};
pub use queries::MsxSessionQueryProvider;
pub use runtime::MsxRuntime;
