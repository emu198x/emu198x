//! Pentagon 128-specific runtime extras: firmware constructors and
//! blank-runtime helpers.
//!
//! Pentagon takes two 16 KiB ROMs in the same layout as the 128K:
//!
//! - `pentagon-128-rom-0` — Pentagon 128 BASIC editor (with 1993 banner)
//! - `pentagon-128-rom-1` — 48 BASIC sub-ROM

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_pentagon_128::Pentagon128;

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

const ROM_0_ID: &str = "pentagon-rom-0";
const ROM_1_ID: &str = "pentagon-rom-1";
const ROM_BYTES: usize = 16 * 1024;

impl SpectrumRuntime<Pentagon128> {
    #[must_use]
    pub fn new_pentagon(rom0: [u8; ROM_BYTES], rom1: [u8; ROM_BYTES]) -> Self {
        let mut machine = Pentagon128::new();
        machine.memory.load_roms(&rom0, &rom1);
        SpectrumRuntime::new(Model::Pentagon128, machine)
    }

    /// Builds a Pentagon 128 runtime from borrowed 16 KiB ROM byte slices.
    ///
    /// # Errors
    ///
    /// Returns an error if either slice is not exactly 16 KiB.
    pub fn from_rom_bytes(rom0: &[u8], rom1: &[u8]) -> Result<Self, RomImageError> {
        let rom0: [u8; ROM_BYTES] = rom0
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom0.len() })?;
        let rom1: [u8; ROM_BYTES] = rom1
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom1.len() })?;
        Ok(Self::new_pentagon(rom0, rom1))
    }

    /// Builds a Pentagon 128 runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if either ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::Pentagon128);
        firmware.validate_for_profile(&profile)?;
        let rom0 = firmware
            .bytes(ROM_0_ID)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: ROM_0_ID.to_owned(),
            })?;
        let rom1 = firmware
            .bytes(ROM_1_ID)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: ROM_1_ID.to_owned(),
            })?;
        Self::from_rom_bytes(rom0, rom1).map_err(|reason| MachineError::InvalidFirmware {
            id: if rom0.len() != ROM_BYTES {
                ROM_0_ID.to_owned()
            } else {
                ROM_1_ID.to_owned()
            },
            reason: reason.to_string(),
        })
    }

    #[must_use]
    pub fn blank() -> Self {
        Self::new_pentagon([0; ROM_BYTES], [0; ROM_BYTES])
    }
}
