//! C64 model selection and construction inputs.

use common_commodore_c64::timing::{C64Timing, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_mos_sid_6581::SidModel;
use mos_cia_6526::CiaModel;
use serde::{Deserialize, Serialize};

/// Supported fresh-workspace C64 machine variants.
///
/// The breadbin and C64C variants share timing and VIC-II; they differ in the
/// SID revision fitted — the original breadbin's MOS 6581 versus the cost-reduced
/// C64C's MOS 8580. The video region (PAL/NTSC) is orthogonal to the SID.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C64Model {
    /// Commodore 64 PAL breadbin (MOS 6581 SID).
    #[default]
    PalBreadbin,
    /// Commodore 64 NTSC breadbin (MOS 6581 SID).
    NtscBreadbin,
    /// Commodore 64C PAL (MOS 8580 SID).
    PalC64c,
    /// Commodore 64C NTSC (MOS 8580 SID).
    NtscC64c,
}

impl C64Model {
    /// Returns the timing descriptor for this machine model.
    #[must_use]
    pub const fn timing(self) -> C64Timing {
        match self {
            Self::PalBreadbin | Self::PalC64c => TIMING_PAL_BREADBIN,
            Self::NtscBreadbin | Self::NtscC64c => TIMING_NTSC_BREADBIN,
        }
    }

    /// Returns the SID revision fitted to this machine model.
    #[must_use]
    pub const fn sid_model(self) -> SidModel {
        match self {
            Self::PalBreadbin | Self::NtscBreadbin => SidModel::Mos6581,
            Self::PalC64c | Self::NtscC64c => SidModel::Mos8580,
        }
    }

    /// Returns the CIA revision fitted to this machine model. The breadbin
    /// ships the original 6526; the cost-reduced C64C board carries the
    /// 8521/6526A, whose interrupt path raises `/IRQ` a cycle earlier.
    #[must_use]
    pub const fn cia_model(self) -> CiaModel {
        match self {
            Self::PalBreadbin | Self::NtscBreadbin => CiaModel::Mos6526,
            Self::PalC64c | Self::NtscC64c => CiaModel::Mos6526A,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadbins_fit_the_6581_and_c64c_fits_the_8580() {
        assert_eq!(C64Model::PalBreadbin.sid_model(), SidModel::Mos6581);
        assert_eq!(C64Model::NtscBreadbin.sid_model(), SidModel::Mos6581);
        assert_eq!(C64Model::PalC64c.sid_model(), SidModel::Mos8580);
        assert_eq!(C64Model::NtscC64c.sid_model(), SidModel::Mos8580);
    }

    #[test]
    fn breadbins_fit_the_6526_and_c64c_fits_the_6526a() {
        assert_eq!(C64Model::PalBreadbin.cia_model(), CiaModel::Mos6526);
        assert_eq!(C64Model::NtscBreadbin.cia_model(), CiaModel::Mos6526);
        assert_eq!(C64Model::PalC64c.cia_model(), CiaModel::Mos6526A);
        assert_eq!(C64Model::NtscC64c.cia_model(), CiaModel::Mos6526A);
    }

    #[test]
    fn c64c_shares_breadbin_timing_by_region() {
        assert_eq!(C64Model::PalC64c.timing(), C64Model::PalBreadbin.timing());
        assert_eq!(C64Model::NtscC64c.timing(), C64Model::NtscBreadbin.timing());
    }
}
