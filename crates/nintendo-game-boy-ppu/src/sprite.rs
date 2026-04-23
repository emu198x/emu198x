//! Sprite entry — one of up to 10 visible on a scanline after OAM
//! scan. Pixel rows are pre-decoded so the per-dot composite step
//! only does priority + palette work.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sprite {
    /// Screen Y (OAM Y - 16). Mostly informational; the dot loop
    /// uses LY directly.
    pub y: u8,
    /// OAM X (screen X + 8).
    pub x: u8,
    /// Tile id (with the lower bit cleared in 8x16 mode).
    pub tile: u8,
    /// OAM attribute byte: priority (bit 7), Y flip (bit 6), X flip
    /// (bit 5), palette select (bit 4), CGB-only bits 0-3.
    pub attr: u8,
    /// Decoded 2-bit pixel row for the current scanline.
    pub pixels: [u8; 8],
}

impl Sprite {
    pub(crate) const EMPTY: Self = Self {
        y: 0,
        x: 0,
        tile: 0,
        attr: 0,
        pixels: [0; 8],
    };
}
