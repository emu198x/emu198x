//! Atari 8-bit computer configuration.
//!
//! Covers all models from the 400 to the 130XE. The chips (ANTIC, GTIA,
//! POKEY, PIA) are identical across all models — only RAM size and ROM
//! banking differ.

/// Computer model.
///
/// Selects RAM size, ROM banking capabilities, and minor I/O differences.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Atari8bitModel {
    /// Atari 400 — 16KB RAM, no XL banking, no BASIC ROM.
    A400,
    /// Atari 800 — 48KB RAM, no XL banking, 2 cartridge slots.
    A800,
    /// Atari 600XL — 16KB RAM, XL banking.
    A600XL,
    /// Atari 800XL — 64KB RAM, XL banking. The primary target.
    #[default]
    A800XL,
    /// Atari 65XE — 64KB RAM, XL banking (same as 800XL).
    A65XE,
    /// Atari 130XE — 128KB (64KB base + 64KB extended via PORTB bits 2-5).
    A130XE,
}

impl Atari8bitModel {
    /// Base RAM size in bytes. Extended RAM (130XE) is additional.
    #[must_use]
    pub const fn base_ram(self) -> usize {
        match self {
            Self::A400 | Self::A600XL => 16384,    // 16KB
            Self::A800 => 49152,                    // 48KB
            Self::A800XL | Self::A65XE | Self::A130XE => 65536, // 64KB
        }
    }

    /// Extended bank-switched RAM in bytes (130XE only).
    #[must_use]
    pub const fn extended_ram(self) -> usize {
        match self {
            Self::A130XE => 65536, // 64KB in 4 × 16KB banks
            _ => 0,
        }
    }

    /// Whether this model supports XL-style PORTB ROM banking.
    #[must_use]
    pub const fn has_xl_banking(self) -> bool {
        matches!(
            self,
            Self::A600XL | Self::A800XL | Self::A65XE | Self::A130XE
        )
    }

    /// Whether this model supports 130XE extended RAM banking.
    #[must_use]
    pub const fn has_extended_banking(self) -> bool {
        matches!(self, Self::A130XE)
    }

    /// Number of 16KB extended RAM banks (0 or 4).
    #[must_use]
    pub const fn extended_banks(self) -> usize {
        match self {
            Self::A130XE => 4,
            _ => 0,
        }
    }
}

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

/// Atari 8-bit computer configuration.
pub struct Atari800xlConfig {
    /// Computer model. Defaults to 800XL.
    pub model: Atari8bitModel,
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
