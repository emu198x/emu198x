//! Atari 7800 configuration.

/// Video region.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Atari7800Region {
    /// NTSC: ~60 Hz, 263 lines, 3,579,545 Hz colour clock.
    #[default]
    Ntsc,
    /// PAL: ~50 Hz, 313 lines, 3,546,894 Hz colour clock.
    Pal,
}

impl Atari7800Region {
    /// Colour clock (master crystal) frequency in Hz.
    #[must_use]
    pub const fn crystal_hz(self) -> u32 {
        match self {
            Self::Ntsc => 3_579_545,
            Self::Pal => 3_546_894,
        }
    }

    /// CPU frequency in Hz (crystal / 2).
    #[must_use]
    pub const fn cpu_hz(self) -> u32 {
        self.crystal_hz() / 2
    }

    /// Lines per frame.
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 263,
            Self::Pal => 313,
        }
    }
}

/// Atari 7800 configuration.
pub struct Atari7800Config {
    /// Raw cartridge ROM data (may include 128-byte A78 header).
    pub rom_data: Vec<u8>,
    /// Video region. Defaults to NTSC.
    pub region: Atari7800Region,
}
