//! Dragon family runtime.
//!
//! This crate is the first host-boundary wrapper for the fresh Dragon 32 path.
//! It owns profile metadata and bridges `machine-dragon-32` to the shared
//! `emu198x-shell` `MachineCore` trait. It runs bounded MC6809 cycles and emits
//! the current MC6847 framebuffer as RGBA8888.

mod media_serde;
mod profiles;
mod runtime;

pub use profiles::{
    COCO_COLOR_BASIC_ROM_ID, COCO_EXTENDED_BASIC_ROM_ID, Model, profile_for, profiles,
};
pub use runtime::{DragonRuntime, DragonSessionQueryProvider, coco_rom_image};
