//! MSX family metadata and runtime surface: MSX1 and MSX2.
//!
//! Wraps named board owners behind the `MachineCore` trait so the
//! shared `emu198x-shell` runner (HeadlessSession, MCP server,
//! script runner) can drive it the same way it drives Spectrum,
//! Amiga, or NES.
//!
//! The MSX cannot run without a 32 KB BIOS image — the runtime models
//! this by holding the machine in `Option`: `MsxRuntime::blank()`
//! constructs a stub that responds to `load_media(firmware=...)` to
//! finish bootstrapping.

mod input;
mod machine;
pub use machine::MsxMachine;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use common_msx::{MapperType, MsxRegion};
pub use profiles::{BIOS_FIRMWARE_ID, Model, SUB_ROM_FIRMWARE_ID, profile_for, profiles};
pub use queries::MsxSessionQueryProvider;
pub use runtime::MsxRuntime;
