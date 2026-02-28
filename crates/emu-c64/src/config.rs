//! C64 configuration: model selection and ROM images.

/// C64 model variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C64Model {
    /// PAL C64 (6569 VIC-II, 985,248 Hz CPU).
    C64Pal,
    /// NTSC C64 (6567 VIC-II, 1,022,727 Hz CPU).
    C64Ntsc,
}

impl C64Model {
    /// CPU clock frequency in Hz.
    #[must_use]
    pub fn cpu_frequency(self) -> u32 {
        match self {
            Self::C64Pal => 985_248,
            Self::C64Ntsc => 1_022_727,
        }
    }

    /// TOD divider: CPU frequency / vertical refresh rate.
    #[must_use]
    pub fn tod_divider(self) -> u32 {
        match self {
            Self::C64Pal => 985_248 / 50,   // 19,705
            Self::C64Ntsc => 1_022_727 / 60, // 17,045
        }
    }

    /// Lines per frame.
    #[must_use]
    pub fn lines_per_frame(self) -> u16 {
        match self {
            Self::C64Pal => 312,
            Self::C64Ntsc => 263,
        }
    }

    /// CPU cycles per raster line.
    #[must_use]
    pub fn cycles_per_line(self) -> u8 {
        match self {
            Self::C64Pal => 63,
            Self::C64Ntsc => 65,
        }
    }
}

/// SID chip revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidModel {
    /// MOS 6581 (original, found in early C64s).
    Sid6581,
    /// MOS 8580 (found in C64C and later models).
    Sid8580,
}

/// Configuration for constructing a C64 instance.
pub struct C64Config {
    /// Model variant.
    pub model: C64Model,
    /// SID chip revision (default: 6581).
    pub sid_model: SidModel,
    /// Kernal ROM (8,192 bytes).
    pub kernal_rom: Vec<u8>,
    /// BASIC ROM (8,192 bytes).
    pub basic_rom: Vec<u8>,
    /// Character ROM (4,096 bytes).
    pub char_rom: Vec<u8>,
    /// 1541 drive ROM (16,384 bytes). If present, enables the drive.
    pub drive_rom: Option<Vec<u8>>,
    /// REU size in KB (128, 256, or 512). None = no REU.
    pub reu_size: Option<u32>,
}
