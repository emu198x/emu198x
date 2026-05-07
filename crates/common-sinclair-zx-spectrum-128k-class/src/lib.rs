//! Shared 128K-class composition for the Spectrum family.
//!
//! Holds [`Spectrum128kClassCore`], the Z80 + Sinclair 7K010E ULA + paged
//! memory + AY + beeper + tape composition that's identical across the
//! 128K-class variants (Sinclair 128K, Sinclair-branded Amstrad-built
//! grey +2). The variant marker `V: Class128kVariant` is a phantom — it
//! gives the two variants distinct types so snapshots can't cross
//! variants and any future divergence lands as a per-marker `impl`
//! block, but it contributes no state.
//!
//! This crate sits one layer above `common-sinclair-zx-spectrum`: it
//! bakes in the concrete Sinclair 7K010E ULA, AY-3-8912 PSG, Z80, and
//! `Memory128K` map, whereas common only carries traits and helpers.
//!
//! Variants outside the 128K-class — 48K-class (Ferranti ULA), +2A/+2B/+3
//! (Amstrad 40077 gate array), Pentagon, Scorpion, Timex — have their
//! own ULAs and additional state and keep their own machine
//! implementations.

pub mod core;
pub mod memory;
pub mod variant;

pub use core::Spectrum128kClassCore;
pub use memory::Memory128K;
pub use variant::{AmstradPlus2Marker, Class128kVariant, Sinclair128KMarker};
