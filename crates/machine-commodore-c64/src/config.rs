//! C64 model selection and construction inputs.

use common_commodore_c64::timing::{C64Timing, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};

/// Supported fresh-workspace C64 machine variants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum C64Model {
    /// Commodore 64 PAL breadbin.
    #[default]
    PalBreadbin,
    /// Commodore 64 NTSC breadbin.
    NtscBreadbin,
}

impl C64Model {
    /// Returns the timing descriptor for this machine model.
    #[must_use]
    pub const fn timing(self) -> C64Timing {
        match self {
            Self::PalBreadbin => TIMING_PAL_BREADBIN,
            Self::NtscBreadbin => TIMING_NTSC_BREADBIN,
        }
    }
}

/// Construction inputs for the C64 machine substrate.
#[derive(Clone, Copy, Debug)]
pub struct C64Config<'a> {
    /// Hardware timing/model selection.
    pub model: C64Model,
    /// KERNAL ROM image, exactly 8 KiB.
    pub kernal_rom: &'a [u8],
    /// BASIC ROM image, exactly 8 KiB.
    pub basic_rom: &'a [u8],
    /// Character ROM image, exactly 4 KiB.
    pub character_rom: &'a [u8],
}
