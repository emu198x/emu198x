//! Dragon family runtime.
//!
//! This crate is the first host-boundary wrapper for the fresh Dragon 32 path.
//! It owns profile metadata and bridges `machine-dragon-32` to the shared
//! `emu198x-shell` `MachineCore` trait. The first slice is intentionally
//! text-framebuffer only: it runs bounded MC6809 cycles and emits the current
//! MC6847 text-mode framebuffer as RGBA8888.

mod profiles;
mod runtime;

pub use profiles::{Model, profile_for, profiles};
pub use runtime::{DragonRuntime, DragonSessionQueryProvider};
