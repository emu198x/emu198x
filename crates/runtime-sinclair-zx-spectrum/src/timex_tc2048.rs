//! Timex TC2048-specific runtime extras: firmware constructors.
//!
//! TC2048 ships a single 16 KiB ROM — `timex-tc2048-rom`.

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_timex_tc2048::TimexTC2048;

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

const ROM_ID: &str = "timex-tc2048-rom";
const ROM_BYTES: usize = 16 * 1024;

impl SpectrumRuntime<TimexTC2048> {
    #[must_use]
    pub fn new_tc2048(rom: [u8; ROM_BYTES]) -> Self {
        let mut machine = TimexTC2048::new();
        machine.memory.load_rom_data(&rom);
        SpectrumRuntime::new(Model::TimexTC2048, machine)
    }

    /// Builds a TC2048 runtime from a borrowed 16 KiB ROM slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 16 KiB.
    pub fn from_rom_bytes(rom: &[u8]) -> Result<Self, RomImageError> {
        let rom: [u8; ROM_BYTES] = rom
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        Ok(Self::new_tc2048(rom))
    }

    /// Builds a TC2048 runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if the ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::TimexTC2048);
        firmware.validate_for_profile(&profile)?;
        let rom = firmware
            .bytes(ROM_ID)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: ROM_ID.to_owned(),
            })?;
        Self::from_rom_bytes(rom).map_err(|reason| MachineError::InvalidFirmware {
            id: ROM_ID.to_owned(),
            reason: reason.to_string(),
        })
    }

    #[must_use]
    pub fn blank() -> Self {
        Self::new_tc2048([0; ROM_BYTES])
    }
}
