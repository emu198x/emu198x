//! Shared Amstrad-class composition for the Spectrum family.
//!
//! Holds [`SpectrumAmstradClassCore`], the Z80 + Amstrad 40077 gate
//! array + 4-ROM paged memory + AY-3-8912 + beeper + tape + FDC
//! composition that's shared between the Amstrad-built +2A, +2B, and
//! +3. The variant marker `V: AmstradVariant` is a phantom — it gives
//! the three variants distinct types and gates the FDC's `enabled`
//! flag (only the +3 ships a floppy drive).
//!
//! This crate sits one layer above `common-sinclair-zx-spectrum`: it
//! bakes in the concrete Amstrad 40077 ULA, Z80, AY-3-8912, NEC
//! µPD765A FDC, and `MemoryPlus` map, whereas common only carries
//! traits and helpers.

pub mod core;
pub mod memory;
pub mod variant;

pub use core::SpectrumAmstradClassCore;
pub use memory::MemoryPlus;
pub use variant::{AmstradVariant, Plus2AMarker, Plus2BMarker, Plus3Marker};
