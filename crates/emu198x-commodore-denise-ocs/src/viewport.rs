//! Viewport extraction helpers for the Denise raster framebuffer.
//!
//! Denise produces a full-raster framebuffer that includes blanking and
//! border regions. Tools (and the runtime's display path) typically want a
//! cropped image at one of a few standard aspect ratios — these helpers
//! turn the full raster into a `ViewportImage` plus the pixel-aspect
//! metadata needed to scale that image onto a square-pixel display.
//!
//! The actual `extract_viewport` method that ties a `ViewportPreset` to a
//! `DeniseOcs::framebuffer_raster` lives in [`crate::chip`]; everything in
//! this module is region-agnostic.

use serde::{Deserialize, Serialize};

/// Viewport presets for cropping the raster framebuffer to displayable area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportPreset {
    /// Standard visible area for 4:3 display output.
    ///
    /// PAL: 192 CCKs × 288 lines → 768×576 at hires line-doubled (exact 4:3).
    /// NTSC: 192 CCKs × 230 lines → 768×460 at hires line-doubled.
    /// Centered on the typical Amiga display window. Wide enough to
    /// show all STRAP display content including sprites at the edges.
    Standard,
    /// Full overscan area with borders.
    Overscan,
    /// Entire raster including blanking — for debug/educational use.
    Full,
}

/// Region-specific viewport bounds in CCK and line units.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ViewportBounds {
    pub h_start_cck: u16,
    pub h_end_cck: u16,
    pub v_start_line: u16,
    pub v_end_line: u16,
}

impl ViewportPreset {
    /// Viewport bounds for PAL.
    #[must_use]
    pub const fn pal_bounds(self) -> ViewportBounds {
        match self {
            Self::Standard => ViewportBounds {
                h_start_cck: 0x2C,
                h_end_cck: 0xEC, // 192 CCKs = 768 hires = 384 lores (4:3)
                v_start_line: 0x19,
                v_end_line: 0x139, // 288 lines → 576 line-doubled (768×576 = 4:3)
            },
            Self::Overscan => ViewportBounds {
                h_start_cck: 0x1C,
                h_end_cck: 0xDC,
                v_start_line: 0x1A,
                v_end_line: 0x138,
            },
            Self::Full => ViewportBounds {
                h_start_cck: 0x00,
                h_end_cck: 0xE3,
                v_start_line: 0x00,
                v_end_line: 312,
            },
        }
    }

    /// Viewport bounds for NTSC.
    #[must_use]
    pub const fn ntsc_bounds(self) -> ViewportBounds {
        match self {
            Self::Standard => ViewportBounds {
                h_start_cck: 0x3C,
                h_end_cck: 0xDC, // 160 CCKs = 640 hires = 320 lores (4:3)
                v_start_line: 0x10,
                v_end_line: 0x100, // 240 lines → 480 line-doubled (640×480 = 4:3)
            },
            Self::Overscan => ViewportBounds {
                h_start_cck: 0x1C,
                h_end_cck: 0xDC,
                v_start_line: 0x1A,
                v_end_line: 0x118,
            },
            Self::Full => ViewportBounds {
                h_start_cck: 0x00,
                h_end_cck: 0xE3,
                v_start_line: 0x00,
                v_end_line: 262,
            },
        }
    }
}

/// Extracted viewport image from the raster framebuffer.
#[derive(Serialize, Deserialize)]
pub struct ViewportImage {
    pub pixels: Vec<u32>,
    pub width: u32,
    pub height: u32,
}

impl ViewportImage {
    /// Scale to a target resolution using nearest-neighbor sampling.
    ///
    /// Preserves pixel edges — appropriate for pixel-art content.
    #[must_use]
    pub fn scale_nearest(&self, target_w: u32, target_h: u32) -> ViewportImage {
        let mut out = Vec::with_capacity((target_w * target_h) as usize);
        for y in 0..target_h {
            let src_y = (y * self.height) / target_h;
            for x in 0..target_w {
                let src_x = (x * self.width) / target_w;
                let idx = (src_y * self.width + src_x) as usize;
                out.push(self.pixels.get(idx).copied().unwrap_or(0xFF00_0000));
            }
        }
        ViewportImage {
            pixels: out,
            width: target_w,
            height: target_h,
        }
    }

    /// Scale to display-correct dimensions for a given PAL/NTSC region.
    ///
    /// Produces a 4:3 image that matches how the Amiga display appears on a
    /// real TV. PAL standard viewport (1280×256 raw) becomes 720×540.
    /// NTSC standard viewport (1280×200 raw) becomes 720×540.
    ///
    /// These dimensions match common emulator output and are suitable for
    /// visual comparison with reference emulators like FS-UAE.
    #[must_use]
    pub fn to_display(&self) -> ViewportImage {
        // Scale to hires + line-doubled resolution: halve the superhires
        // width, double the deinterlaced height. For the Standard viewport
        // this produces 768×576 (PAL, 4:3) or 640×480 (NTSC, 4:3).
        let target_w = self.width / 2;
        let target_h = self.height * 2;
        self.scale_nearest(target_w, target_h)
    }
}

/// Pixel aspect ratio for correct display on square-pixel screens.
///
/// Uses the BT.601/Amiga community convention:
/// - PAL lores: 16:15 (~1.067) — pixels slightly wider than tall
/// - NTSC lores: 8:9 (~0.889) — pixels slightly taller than wide
///
/// These match the ITU-R BT.601 values for 720×576 PAL and 720×480 NTSC
/// respectively, and are the standard values used by AmigaOS monitor drivers.
#[must_use]
pub fn pixel_aspect_ratio(pal: bool, hires: bool, interlaced: bool) -> f64 {
    let base = if pal { 16.0 / 15.0 } else { 8.0 / 9.0 };
    let h_factor = if hires { 0.5 } else { 1.0 };
    let v_factor = if interlaced { 0.5 } else { 1.0 };
    base * h_factor * v_factor
}
