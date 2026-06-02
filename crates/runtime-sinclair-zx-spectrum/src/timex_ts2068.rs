//! Timex TS2068 / TC2068-specific runtime extras.
//!
//! Shares one machine type (`TimexTS2068`) selected by `TimexModel`. The
//! firmware bundle is two slices:
//!
//! - `timex-ts2068-rom-0` — 16 KiB system ROM
//! - `timex-ts2068-rom-1` — 8 KiB EXROM

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_timex_ts2068::{TimexModel, TimexTS2068};

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

const ROM_0_ID: &str = "timex-ts2068-rom-0";
const ROM_1_ID: &str = "timex-ts2068-rom-1";
const ROM_BYTES: usize = 16 * 1024;
const EXROM_BYTES: usize = 8 * 1024;

impl SpectrumRuntime<TimexTS2068> {
    #[must_use]
    pub fn new_ts2068(model: Model, rom: [u8; ROM_BYTES], exrom: [u8; EXROM_BYTES]) -> Self {
        let timex_model = match model {
            Model::TimexTC2068 => TimexModel::TC2068,
            _ => TimexModel::TS2068,
        };
        let mut machine = TimexTS2068::new(timex_model);
        machine.memory.load_rom_data(&rom);
        machine.memory.load_exrom_data(&exrom);
        SpectrumRuntime::new(model, machine)
    }

    /// Builds a TS2068/TC2068 runtime from borrowed ROM + EXROM slices.
    ///
    /// # Errors
    ///
    /// Returns an error if `rom` is not exactly 16 KiB or `exrom` is not
    /// exactly 8 KiB.
    pub fn from_rom_bytes(model: Model, rom: &[u8], exrom: &[u8]) -> Result<Self, RomImageError> {
        let rom: [u8; ROM_BYTES] = rom
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        let exrom: [u8; EXROM_BYTES] = exrom.try_into().map_err(|_| RomImageError::WrongSize {
            actual: exrom.len(),
        })?;
        Ok(Self::new_ts2068(model, rom, exrom))
    }

    /// Builds a TS2068/TC2068 runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if either ROM image fails validation.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let rom = firmware
            .bytes(ROM_0_ID)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: ROM_0_ID.to_owned(),
            })?;
        let exrom = firmware
            .bytes(ROM_1_ID)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: ROM_1_ID.to_owned(),
            })?;
        Self::from_rom_bytes(model, rom, exrom).map_err(|reason| MachineError::InvalidFirmware {
            id: if rom.len() != ROM_BYTES {
                ROM_0_ID.to_owned()
            } else {
                ROM_1_ID.to_owned()
            },
            reason: reason.to_string(),
        })
    }

    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self::new_ts2068(model, [0; ROM_BYTES], [0; EXROM_BYTES])
    }
}
