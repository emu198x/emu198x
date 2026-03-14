//! Atari 5200 configuration.

/// Video region.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Atari5200Region {
    /// NTSC: ~60 Hz, 262 lines, 3,579,545 Hz colour clock.
    #[default]
    Ntsc,
    /// PAL: ~50 Hz, 312 lines, 3,546,894 Hz colour clock.
    Pal,
}

impl Atari5200Region {
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
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }
}

/// Atari 5200 configuration.
pub struct Atari5200Config {
    /// Raw cartridge ROM data.
    pub rom_data: Vec<u8>,
    /// Optional 2KB BIOS ROM. Without BIOS, the system boots directly
    /// from the cartridge reset vector.
    pub bios_data: Option<Vec<u8>>,
    /// Video region. Defaults to NTSC.
    pub region: Atari5200Region,
}
