//! Configuration for the Amiga machine crate.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmigaModel {
    A500,
    A500Plus,
    A1200,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AmigaChipset {
    #[default]
    Ocs,
    Ecs,
    Aga,
}

impl AmigaChipset {
    /// True for ECS or AGA (both extend OCS with additional registers).
    #[must_use]
    pub const fn is_ecs_or_aga(self) -> bool {
        matches!(self, Self::Ecs | Self::Aga)
    }
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
    /// Slow RAM (A500 trapdoor expansion) size in bytes.
    /// 0 = disabled, valid sizes: 512K, 1M, 2M.
    /// Maps to $C00000-$DFFFFF.
    pub slow_ram_size: usize,
}
