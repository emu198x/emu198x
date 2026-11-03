//! Fixed board runtime construction; RAM is a profile, not a physical owner.
use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};
use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_sinclair_zx_spectrum_issue1::{SpectrumIssue1With16k, SpectrumIssue1With48k};
use machine_sinclair_zx_spectrum_issue2::{SpectrumIssue2With16k, SpectrumIssue2With48k};
use machine_sinclair_zx_spectrum_issue3::{SpectrumIssue3With16k, SpectrumIssue3With48k};
use machine_sinclair_zx_spectrum_issue3b::{SpectrumIssue3BWith16k, SpectrumIssue3BWith48k};
use machine_sinclair_zx_spectrum_issue4a::{SpectrumIssue4AWith16k, SpectrumIssue4AWith48k};
use machine_sinclair_zx_spectrum_issue4b::{SpectrumIssue4BWith16k, SpectrumIssue4BWith48k};
use machine_sinclair_zx_spectrum_issue5_6c001e6::{
    SpectrumIssue5Ula6c001e6With16k, SpectrumIssue5Ula6c001e6With48k,
};
use machine_sinclair_zx_spectrum_issue5_6c001e7::{
    SpectrumIssue5Ula6c001e7With16k, SpectrumIssue5Ula6c001e7With48k,
};
use machine_sinclair_zx_spectrum_issue6a::{SpectrumIssue6AWith16k, SpectrumIssue6AWith48k};

macro_rules! board_runtime {
    ($machine:ty, $model:expr) => {
        impl SpectrumRuntime<$machine> {
            /// Construct the fixed board and RAM profile with its 16 KiB ROM.
            #[must_use]
            pub fn new_board(rom: [u8; 16384]) -> Self {
                Self::new($model, <$machine>::with_board_rom(rom))
            }
            /// Construct from a complete ROM image.
            /// # Errors
            /// Rejects images whose size is not exactly 16 KiB.
            pub fn from_rom_bytes(rom: &[u8]) -> Result<Self, RomImageError> {
                let rom = rom
                    .try_into()
                    .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
                Ok(Self::new_board(rom))
            }
            /// Construct from exactly the firmware declared by this profile.
            /// # Errors
            /// Rejects missing, unknown or wrong-sized firmware.
            pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
                let profile = profile_for($model);
                firmware.validate_for_profile(&profile)?;
                let id = "sinclair-zx-spectrum-48k-rom";
                let rom = firmware
                    .bytes(id)
                    .ok_or_else(|| MachineError::MissingFirmware { id: id.into() })?;
                Self::from_rom_bytes(rom).map_err(|reason| MachineError::InvalidFirmware {
                    id: id.into(),
                    reason: reason.to_string(),
                })
            }
            /// Construct a fixed board with a blank ROM for fixtures and tools.
            #[must_use]
            pub fn blank() -> Self {
                Self::new_board([0; 16384])
            }
        }
    };
}
board_runtime!(SpectrumIssue1With16k, Model::SpectrumIssue1Ram16kPal);
board_runtime!(SpectrumIssue1With48k, Model::SpectrumIssue1Ram48kPal);
board_runtime!(SpectrumIssue2With16k, Model::SpectrumIssue2Ram16kPal);
board_runtime!(SpectrumIssue2With48k, Model::SpectrumIssue2Ram48kPal);
board_runtime!(SpectrumIssue3With16k, Model::SpectrumIssue3Ram16kPal);
board_runtime!(SpectrumIssue3With48k, Model::SpectrumIssue3Ram48kPal);
board_runtime!(SpectrumIssue3BWith16k, Model::SpectrumIssue3BRam16kPal);
board_runtime!(SpectrumIssue3BWith48k, Model::SpectrumIssue3BRam48kPal);
board_runtime!(SpectrumIssue4AWith16k, Model::SpectrumIssue4ARam16kPal);
board_runtime!(SpectrumIssue4AWith48k, Model::SpectrumIssue4ARam48kPal);
board_runtime!(SpectrumIssue4BWith16k, Model::SpectrumIssue4BRam16kPal);
board_runtime!(SpectrumIssue4BWith48k, Model::SpectrumIssue4BRam48kPal);
board_runtime!(
    SpectrumIssue5Ula6c001e6With16k,
    Model::SpectrumIssue5Ula6c001e6Ram16kPal
);
board_runtime!(
    SpectrumIssue5Ula6c001e6With48k,
    Model::SpectrumIssue5Ula6c001e6Ram48kPal
);
board_runtime!(
    SpectrumIssue5Ula6c001e7With16k,
    Model::SpectrumIssue5Ula6c001e7Ram16kPal
);
board_runtime!(
    SpectrumIssue5Ula6c001e7With48k,
    Model::SpectrumIssue5Ula6c001e7Ram48kPal
);
board_runtime!(SpectrumIssue6AWith16k, Model::SpectrumIssue6ARam16kPal);
board_runtime!(SpectrumIssue6AWith48k, Model::SpectrumIssue6ARam48kPal);
