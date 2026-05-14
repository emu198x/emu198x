//! 16K-specific runtime extras: firmware constructors and blank-runtime
//! helpers. Mirrors `spectrum_48k` — the 16K shares the 48K's single-ROM
//! firmware shape (`sinclair-zx-spectrum-48k-rom`) and only differs in
//! RAM size.
//!
//! The runtime itself is the generic `SpectrumRuntime<Spectrum16K>` —
//! everything below builds on that.

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_sinclair_zx_spectrum_16k::{BoardIssue, Spectrum16K};

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

impl SpectrumRuntime<Spectrum16K> {
    /// Builds a 16K runtime around the given Issue 3 ROM image.
    #[must_use]
    pub fn new_16k(rom: [u8; 16 * 1024]) -> Self {
        SpectrumRuntime::new(
            Model::Spectrum16KPal,
            Spectrum16K::with_rom(BoardIssue::Issue3, rom),
        )
    }

    /// Builds an Issue 3 16K runtime from a borrowed 16 KiB ROM byte
    /// slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 16 KiB.
    pub fn from_rom_bytes(rom: &[u8]) -> Result<Self, RomImageError> {
        let rom: [u8; 16 * 1024] = rom
            .try_into()
            .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        Ok(Self::new_16k(rom))
    }

    /// Builds an Issue 3 16K runtime from the profile-declared firmware
    /// set. The 16K shares the 48K's ROM image (same Ferranti ULA, same
    /// 48 BASIC) under the `sinclair-zx-spectrum-48k-rom` firmware id.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if the ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::Spectrum16KPal);
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
        Self::new_16k([0; 16 * 1024])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::Spectrum16kRuntime;
    use emu198x_shell::FirmwareImage;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<Spectrum16K>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the 16K runtime"),
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
        match SpectrumRuntime::<Spectrum16K>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size 48K ROM must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        match SpectrumRuntime::<Spectrum16K>::from_rom_bytes(&[0u8; 1024]) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn blank_runtime_carries_the_16k_profile_id() {
        use emu198x_shell::MachineCore;
        let runtime = Spectrum16kRuntime::blank();
        assert_eq!(
            runtime.profile().profile_id.as_str(),
            "sinclair-zx-spectrum-16k-pal"
        );
    }
}
