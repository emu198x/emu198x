//! Atari 800XL configuration.

/// Video region.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Atari800xlRegion {
    /// NTSC: ~60 Hz, 262 lines, 3,579,545 Hz colour clock.
    #[default]
    Ntsc,
    /// PAL: ~50 Hz, 312 lines, 3,546,894 Hz colour clock.
    Pal,
}

impl Atari800xlRegion {
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

/// Atari 800XL configuration.
pub struct Atari800xlConfig {
    /// Optional cartridge ROM data (8KB or 16KB).
    pub rom_data: Option<Vec<u8>>,
    /// OS ROM (~16KB, maps $C000-$FFFF with $D000-$D7FF gap).
    pub os_rom: Option<Vec<u8>>,
    /// BASIC ROM (8KB, maps $A000-$BFFF).
    pub basic_rom: Option<Vec<u8>>,
    /// Video region. Defaults to NTSC.
    pub region: Atari800xlRegion,
    /// Whether BASIC starts enabled (PIA PORTB bit 1 = 0).
    pub basic_enabled: bool,
}
