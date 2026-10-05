//! Scorpion ZS-256-specific runtime extras: firmware constructors.
//!
//! Scorpion ships 4 × 16 KiB ROMs:
//!
//! - `scorpion-rom-0` — the 128 menu and editor ("1992-94 Scorpion ZS 256")
//! - `scorpion-rom-1` — 48 BASIC (the Sinclair ROM)
//! - `scorpion-rom-2` — the service monitor
//! - `scorpion-rom-3` — TR-DOS 5.03, paged in as the Beta Disk overlay
//!
//! The four files are the four 16 KiB blocks of the Scorpion ZS-256 TURBO+
//! v2.95 64 KiB image catalogued by TOSEC (CRC32 0eb40a09, 9d513013,
//! e484eca5, d734e841). ROMs 0 and 1 are the same in v2.92, MAME's
//! `scorpio` set; its ROMs 2 and 3 are v2.92's and differ from these.

use common_sinclair_zx_spectrum::error::RomImageError;
use emu198x_shell::{FirmwareSet, MachineError};
use machine_scorpion_zs256::ScorpionZS256;

use crate::runtime::SpectrumRuntime;
use crate::{Model, profile_for};

const ROM_BYTES: usize = 16 * 1024;
const ROM_IDS: [&str; 4] = [
    "scorpion-rom-0",
    "scorpion-rom-1",
    "scorpion-rom-2",
    "scorpion-rom-3",
];

impl SpectrumRuntime<ScorpionZS256> {
    #[must_use]
    pub fn new_scorpion(roms: [[u8; ROM_BYTES]; 4]) -> Self {
        let mut machine = ScorpionZS256::new();
        machine
            .memory
            .load_roms(&roms[0], &roms[1], &roms[2], &roms[3]);
        SpectrumRuntime::new(Model::ScorpionZS256, machine)
    }

    /// Builds a Scorpion runtime from borrowed 16 KiB ROM byte slices.
    ///
    /// # Errors
    ///
    /// Returns an error if any slice is not exactly 16 KiB.
    pub fn from_rom_bytes(roms: [&[u8]; 4]) -> Result<Self, RomImageError> {
        let mut arr: [[u8; ROM_BYTES]; 4] = [[0; ROM_BYTES]; 4];
        for (i, rom) in roms.iter().enumerate() {
            arr[i] = (*rom)
                .try_into()
                .map_err(|_| RomImageError::WrongSize { actual: rom.len() })?;
        }
        Ok(Self::new_scorpion(arr))
    }

    /// Builds a Scorpion runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing, if unknown
    /// firmware is supplied, or if any ROM image fails validation.
    pub fn from_firmware(firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(Model::ScorpionZS256);
        firmware.validate_for_profile(&profile)?;
        let mut roms: [&[u8]; 4] = [&[]; 4];
        for (i, id) in ROM_IDS.iter().enumerate() {
            roms[i] = firmware
                .bytes(id)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: (*id).to_owned(),
                })?;
        }
        Self::from_rom_bytes(roms).map_err(|reason| {
            let first_bad = roms.iter().position(|r| r.len() != ROM_BYTES).unwrap_or(0);
            MachineError::InvalidFirmware {
                id: ROM_IDS[first_bad].to_owned(),
                reason: reason.to_string(),
            }
        })
    }

    #[must_use]
    pub fn blank() -> Self {
        Self::new_scorpion([[0; ROM_BYTES]; 4])
    }
}
