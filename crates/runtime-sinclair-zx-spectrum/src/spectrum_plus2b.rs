//! +2B-specific runtime extras: firmware constructors and blank-runtime
//! helpers. Same 4-ROM Amstrad gate-array layout as the +2A and +3,
//! sharing the `sinclair-zx-spectrum-plus3-rom-{0..3}` firmware ids.
//! ROM 2 carries a dummy on +2A/+2B (no built-in disk drive); the +3
//! has the actual +3 DOS ROM in that slot.

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_sinclair_zx_spectrum_plus2b::SpectrumPlus2B;

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

const ROM_IDS: [&str; 4] = [
    "sinclair-zx-spectrum-plus3-rom-0",
    "sinclair-zx-spectrum-plus3-rom-1",
    "sinclair-zx-spectrum-plus3-rom-2",
    "sinclair-zx-spectrum-plus3-rom-3",
];
const ROM_BYTES: usize = 16 * 1024;

impl SpectrumRuntime<SpectrumPlus2B> {
    /// Builds a +2B runtime around the supplied 4-ROM image set.
    #[must_use]
    pub fn new_plus2b(roms: [[u8; ROM_BYTES]; 4]) -> Self {
        let mut machine = SpectrumPlus2B::new();
        machine
            .memory
            .load_roms(&roms[0], &roms[1], &roms[2], &roms[3]);
        SpectrumRuntime::new(Model::SpectrumPlus2B, machine)
    }

    /// Builds a +2B runtime from borrowed 16 KiB ROM byte slices.
    ///
    /// # Errors
    ///
    /// Returns an error if any slice is not exactly 16 KiB.
    pub fn from_rom_bytes(
        rom0: &[u8],
        rom1: &[u8],
        rom2: &[u8],
        rom3: &[u8],
    ) -> Result<Self, RomImageError> {
        let roms = [
            rom0.try_into()
                .map_err(|_| RomImageError::WrongSize { actual: rom0.len() })?,
            rom1.try_into()
                .map_err(|_| RomImageError::WrongSize { actual: rom1.len() })?,
            rom2.try_into()
                .map_err(|_| RomImageError::WrongSize { actual: rom2.len() })?,
            rom3.try_into()
                .map_err(|_| RomImageError::WrongSize { actual: rom3.len() })?,
        ];
        Ok(Self::new_plus2b(roms))
    }

    /// Builds a +2B runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if any required ROM is missing, if unknown
    /// firmware is supplied, or if any ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::SpectrumPlus2B);
        firmware.validate_for_profile(&profile)?;
        let mut roms: [&[u8]; 4] = [&[]; 4];
        for (slot, id) in roms.iter_mut().zip(ROM_IDS.iter()) {
            *slot = firmware.bytes(id).ok_or_else(|| MachineError::MissingFirmware {
                id: (*id).to_owned(),
            })?;
        }

        Self::from_rom_bytes(roms[0], roms[1], roms[2], roms[3]).map_err(|reason| {
            MachineError::InvalidFirmware {
                id: ROM_IDS
                    .iter()
                    .zip(roms.iter())
                    .find(|(_, bytes)| bytes.len() != ROM_BYTES)
                    .map(|(id, _)| (*id).to_owned())
                    .unwrap_or_else(|| ROM_IDS[0].to_owned()),
                reason: reason.to_string(),
            }
        })
    }

    /// Builds a runtime backed by zero-filled ROM images.
    #[must_use]
    pub fn blank() -> Self {
        Self::new_plus2b([[0; ROM_BYTES]; 4])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variants::SpectrumPlus2BRuntime;
    use emu198x_shell::FirmwareImage;

    #[test]
    fn from_firmware_rejects_missing_rom() {
        let firmware = FirmwareSet::new();
        match SpectrumRuntime::<SpectrumPlus2B>::from_firmware(&firmware) {
            Err(MachineError::MissingFirmware { .. }) => {}
            Err(other) => panic!("expected MissingFirmware, got {other:?}"),
            Ok(_) => panic!("empty firmware set should fail to boot the +2B runtime"),
        }
    }

    #[test]
    fn from_firmware_reports_invalid_rom_when_size_mismatches() {
        let mut firmware = FirmwareSet::new();
        let too_small = [0u8; 1024];
        let full = [0u8; ROM_BYTES];
        firmware.push(FirmwareImage::new(ROM_IDS[0], &too_small));
        firmware.push(FirmwareImage::new(ROM_IDS[1], &full));
        firmware.push(FirmwareImage::new(ROM_IDS[2], &full));
        firmware.push(FirmwareImage::new(ROM_IDS[3], &full));
        match SpectrumRuntime::<SpectrumPlus2B>::from_firmware(&firmware) {
            Err(MachineError::InvalidFirmware { .. }) => {}
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("wrong-size +2B ROM 0 must be rejected"),
        }
    }

    #[test]
    fn from_rom_bytes_rejects_wrong_size_slice() {
        let full = [0u8; ROM_BYTES];
        match SpectrumRuntime::<SpectrumPlus2B>::from_rom_bytes(
            &[0u8; 1024],
            &full,
            &full,
            &full,
        ) {
            Err(RomImageError::WrongSize { actual }) => assert_eq!(actual, 1024),
            Ok(_) => panic!("wrong-size slice must be rejected at construction time"),
        }
    }

    #[test]
    fn blank_runtime_carries_the_plus2b_profile_id() {
        use emu198x_shell::MachineCore;
        let runtime = SpectrumPlus2BRuntime::blank();
        assert_eq!(
            runtime.profile().profile_id.as_str(),
            "sinclair-zx-spectrum-plus2b-pal"
        );
    }
}
