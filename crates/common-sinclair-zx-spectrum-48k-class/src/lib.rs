//! Shared 48K-class composition for the Spectrum family.
//!
//! Holds [`SpectrumMachineCore`], the Z80 + Ferranti ULA + beeper + tape
//! composition that's identical across the 48K-class variants (16K, 48K,
//! Spectrum+). Memory is the single point of variation, parameterised via
//! `M: MemoryBus`. Each variant crate (`machine-sinclair-zx-spectrum-{16k,
//! 48k,plus}`) is a thin alias.
//!
//! This crate sits one layer above `common-sinclair-zx-spectrum`: it
//! bakes in the concrete Ferranti 6C001E ULA and Zilog Z80 chip crates,
//! whereas common only carries traits and helpers.
//!
//! Variants outside the 48K-class — 128K-family, Pentagon, Scorpion,
//! Timex — have their own ULAs and additional state (AY, paging, FDC)
//! and keep their own machine implementations.

pub mod core;
pub mod tape_input;
pub mod variant;

pub use core::SpectrumMachineCore;
pub use tape_input::TapeInput;
pub use variant::{Spectrum16kMarker, Spectrum48kMarker, SpectrumPlusMarker, Variant48kClass};
