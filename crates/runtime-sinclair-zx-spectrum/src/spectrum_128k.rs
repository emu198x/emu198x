//! 128K-specific runtime extras: firmware constructors and
//! blank-runtime helpers. Two 16 KiB ROMs:
//!
//! - `sinclair-zx-spectrum-128k-rom-0` — 128 BASIC editor / menu
//! - `sinclair-zx-spectrum-128k-rom-1` — 48 BASIC sub-ROM

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

const ROM_0_ID: &str = "sinclair-zx-spectrum-128k-rom-0";
const ROM_1_ID: &str = "sinclair-zx-spectrum-128k-rom-1";
const ROM_BYTES: usize = 16 * 1024;

impl SpectrumRuntime<Spectrum128K> {
    /// Builds a 128K runtime around the supplied editor + 48 BASIC ROMs.
    #[must_use]
    pub fn new_128k(rom0: [u8; ROM_BYTES], rom1: [u8; ROM_BYTES]) -> Self {
        let mut machine = Spectrum128K::new();
        machine.memory.load_roms(&rom0, &rom1);
        SpectrumRuntime::new(Model::Spectrum128KPal, machine)
    }

    /// Builds a 128K runtime from borrowed 16 KiB ROM byte slices.
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
        Ok(Self::new_128k(rom0, rom1))
    }

    /// Builds a 128K runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if either ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::Spectrum128KPal);
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

    /// Builds a runtime backed by zero-filled ROM images.
    #[must_use]
    pub fn blank() -> Self {
        Self::new_128k([0; ROM_BYTES], [0; ROM_BYTES])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::Spectrum128kRuntime;
    use emu198x_shell::FirmwareImage;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<Spectrum128K>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the 128K runtime"),
        }
    }

    #[test]
    fn from_firmware_reports_invalid_rom_when_size_mismatches() {
        let mut firmware = FirmwareSet::new();
        let too_small = [0u8; 1024];
        let full = [0u8; ROM_BYTES];
        firmware.push(FirmwareImage::new(ROM_0_ID, &too_small));
        firmware.push(FirmwareImage::new(ROM_1_ID, &full));
        match SpectrumRuntime::<Spectrum128K>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 128K ROM 0 must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<Spectrum128K>::from_rom_bytes(&[0u8; 1024], &[0u8; ROM_BYTES]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn blank_runtime_carries_the_128k_profile_id() {
        use emu198x_shell::MachineCore;
        let runtime = Spectrum128kRuntime::blank();
        assert_eq!(
            runtime.profile().profile_id.as_str(),
            "sinclair-zx-spectrum-128k-pal"
        );
    }
}
