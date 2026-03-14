//! Atari 2600 configuration.

/// Video region.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Atari2600Region {
    /// NTSC: ~60 Hz, 262 lines, 3,579,545 Hz crystal.
    #[default]
    Ntsc,
    /// PAL: ~50 Hz, 312 lines, 3,546,894 Hz crystal.
    Pal,
}

impl Atari2600Region {
    /// Lines per frame (nominal — actual count is software-controlled).
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }

    /// Crystal frequency in Hz.
    #[must_use]
    pub const fn crystal_hz(self) -> u32 {
        match self {
            Self::Ntsc => 3_579_545,
            Self::Pal => 3_546_894,
        }
    }

    /// CPU frequency in Hz (crystal / 3).
    #[must_use]
    pub const fn cpu_hz(self) -> u32 {
        self.crystal_hz() / 3
    }
}

/// Atari 2600 configuration.
pub struct Atari2600Config {
    /// Raw ROM file contents.
    pub rom_data: Vec<u8>,
    /// Video region. Defaults to NTSC.
    pub region: Atari2600Region,
}
