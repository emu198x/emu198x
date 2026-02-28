//! NES configuration.

/// Video region — determines frame timing and APU rates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NesRegion {
    /// NTSC: 60 Hz, 262 scanlines, 1,789,773 Hz CPU.
    #[default]
    Ntsc,
    /// PAL: 50 Hz, 312 scanlines, 1,662,607 Hz CPU.
    Pal,
}

impl NesRegion {
    /// Master crystal frequency in Hz.
    #[must_use]
    pub const fn crystal_hz(self) -> u64 {
        match self {
            Self::Ntsc => 21_477_272,
            Self::Pal => 26_601_712,
        }
    }

    /// Total scanlines per frame (including pre-render and VBlank).
    #[must_use]
    pub const fn scanlines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }

    /// Pre-render scanline number (last scanline of the frame).
    #[must_use]
    pub const fn pre_render_line(self) -> u16 {
        self.scanlines_per_frame() - 1
    }

    /// CPU frequency in Hz.
    #[must_use]
    pub const fn cpu_hz(self) -> u32 {
        match self {
            Self::Ntsc => 1_789_773,
            Self::Pal => 1_662_607,
        }
    }
}

/// NES configuration.
pub struct NesConfig {
    /// iNES file contents.
    pub rom_data: Vec<u8>,
    /// Video region (NTSC or PAL). Defaults to NTSC.
    pub region: NesRegion,
}
