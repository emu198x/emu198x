//! Commodore Denise OCS — video output, bitplane shifter, and sprite engine.
//!
//! Denise receives bitplane data from Agnus DMA and shifts it out pixel by
//! pixel, combining with the colour palette to produce the final framebuffer.
//!
//! The implementation is split across three per-concern modules:
//!
//! - [`chip`] — the [`DeniseOcs`] core (struct + register dispatch +
//!   shifter + sprite engine + viewport extraction).
//! - [`debug`] — debug instrumentation types returned by inspection
//!   helpers (`last_shift_load_debug`, `output_pixel_with_beam`).
//! - [`viewport`] — region-agnostic viewport bounds, image scaling, and
//!   pixel aspect ratio helpers.
//!
//! Every previously-public symbol stays importable at this crate root via
//! the `pub use` re-exports below, so the public API is unchanged.

mod chip;
mod debug;
mod viewport;

/// Raster framebuffer width: 227 CCKs × 8 superhires pixels.
pub const RASTER_FB_WIDTH: u32 = 1816;
/// PAL raster framebuffer height: 312 lines x 2 (interlace double-height).
pub const PAL_RASTER_FB_HEIGHT: u32 = 624;
/// NTSC raster framebuffer height: 262 lines x 2 (interlace double-height).
pub const NTSC_RASTER_FB_HEIGHT: u32 = 524;

pub use chip::DeniseOcs;
pub use debug::{
    DeniseOutputPixelDebug, DeniseShiftLoadDebug, DeniseShiftLoadPlaneDebug, DeniseSourcePixelDebug,
};
pub use viewport::{pixel_aspect_ratio, ViewportBounds, ViewportImage, ViewportPreset};
