//! Spectrum+ runtime extras: firmware constructors and blank-runtime
//! helpers. Electrically identical to the 48K (same Ferranti ULA, same
//! 48 BASIC ROM, same RAM); the phantom variant marker
//! [`machine_sinclair_zx_spectrum_plus::SpectrumPlusMarker`] keeps the
//! Spectrum+ as a distinct Rust type from the 48K.

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_sinclair_zx_spectrum_plus::{BoardIssue, SpectrumPlus};

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

impl SpectrumRuntime<SpectrumPlus> {
    /// Builds a Spectrum+ runtime around the given Issue 3 ROM image.
    #[must_use]
    pub fn new_plus(rom: [u8; 16 * 1024]) -> Self {
        SpectrumRuntime::new(
            Model::SpectrumPlus,
            SpectrumPlus::with_rom(BoardIssue::Issue3, rom),
        )
    }

    /// Builds an Issue 3 Spectrum+ runtime from a borrowed 16 KiB ROM
    /// byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 16 KiB.
    pub fn from_rom_bytes(rom: &[u8]) -> Result<Self, RomImageError> {
        let rom: [u8; 16 * 1024] = rom
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        Ok(Self::new_plus(rom))
    }

    /// Builds an Issue 3 Spectrum+ runtime from the profile-declared
    /// firmware set. The Spectrum+ shares the 48K's ROM image under the
    /// `sinclair-zx-spectrum-48k-rom` firmware id.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if the ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::SpectrumPlus);
        let rom_id = "sinclair-zx-spectrum-48k-rom";
        firmware.validate_for_profile(&profile)?;
        let rom = firmware
            .bytes(rom_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: rom_id.to_owned(),
            })?;

        Self::from_rom_bytes(rom).map_err(|reason| MachineError::InvalidFirmware {
            id: rom_id.to_owned(),
            reason: reason.to_string(),
        })
    }

    /// Builds a runtime backed by a zero-filled ROM image.
    #[must_use]
    pub fn blank() -> Self {
        Self::new_plus([0; 16 * 1024])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::SpectrumPlusRuntime;
    use emu198x_shell::FirmwareImage;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<SpectrumPlus>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the Spectrum+ runtime"),
        }
    }

    #[test]
    fn from_firmware_reports_invalid_rom_when_size_mismatches() {
        let mut firmware = FirmwareSet::new();
        let too_small = [0u8; 1024];
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &too_small,
        ));
        match SpectrumRuntime::<SpectrumPlus>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 48K ROM must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<SpectrumPlus>::from_rom_bytes(&[0u8; 1024]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn blank_runtime_carries_the_plus_profile_id() {
        use emu198x_shell::MachineCore;
        let runtime = SpectrumPlusRuntime::blank();
        assert_eq!(
            runtime.profile().profile_id.as_str(),
            "sinclair-zx-spectrum-plus-pal"
        );
    }
}
