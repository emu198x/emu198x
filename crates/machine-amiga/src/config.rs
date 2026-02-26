//! Configuration for the Amiga machine crate.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmigaModel {
    A500,
    A500Plus,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AmigaChipset {
    #[default]
    Ocs,
    Ecs,
}

/// Video region (PAL or NTSC) — determines frame timing and raster dimensions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AmigaRegion {
    #[default]
    Pal,
    Ntsc,
}

impl AmigaRegion {
    /// Lines per frame (non-interlaced).
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Pal => 312,
            Self::Ntsc => 262,
        }
    }

    /// Raster framebuffer height (double-height for interlace support).
    #[must_use]
    pub const fn raster_fb_height(self) -> u32 {
        match self {
            Self::Pal => PAL_RASTER_FB_HEIGHT,
            Self::Ntsc => NTSC_RASTER_FB_HEIGHT,
        }
    }
}

/// Raster framebuffer width: 227 CCKs x 4 hires pixels = 908.
pub const RASTER_FB_WIDTH: u32 = 908;

/// PAL raster framebuffer height: 312 lines x 2 (interlace) = 624.
pub const PAL_RASTER_FB_HEIGHT: u32 = 624;

/// NTSC raster framebuffer height: 262 lines x 2 (interlace) = 524.
pub const NTSC_RASTER_FB_HEIGHT: u32 = 524;

#[derive(Debug, Clone)]
pub struct AmigaConfig {
    pub model: AmigaModel,
    pub chipset: AmigaChipset,
    pub region: AmigaRegion,
    pub kickstart: Vec<u8>,
}
