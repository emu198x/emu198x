//! PET family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    ProfileId, Region, SupportTier, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// PET 4032 / 8032 with 40-column display.
    Pet40Col,
    /// PET 8032 with 80-column display.
    Pet80Col,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Pet40Col => "commodore-pet-40col",
            Self::Pet80Col => "commodore-pet-80col",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Pet40Col => "Commodore PET (40-col)",
            Self::Pet80Col => "Commodore PET (80-col)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Other
    }
    #[must_use]
    pub const fn screen_chars(self) -> u32 {
        match self {
            Self::Pet40Col => 40,
            Self::Pet80Col => 80,
        }
    }
}

pub const KERNAL_FIRMWARE_ID: &str = "commodore-pet-kernal";
pub const BASIC_FIRMWARE_ID: &str = "commodore-pet-basic";
pub const EDITOR_FIRMWARE_ID: &str = "commodore-pet-editor";
pub const CHAR_FIRMWARE_ID: &str = "commodore-pet-char";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Pet40Col), profile_for(Model::Pet80Col)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("commodore-pet"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1977,
        summary: "Commodore PET / CBM — 6502 + 6845 CRTC + PIA/VIA, KERNAL + BASIC + editor + character ROMs, 40 or 80 column display.".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(1_000_000)),
        firmware: vec![
            FirmwareRequirement::new(KERNAL_FIRMWARE_ID, "PET KERNAL ROM (4 KB)", false),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID, "PET BASIC ROM (8 KB)", false),
            FirmwareRequirement::new(EDITOR_FIRMWARE_ID, "PET editor ROM (2 KB)", false),
            FirmwareRequirement::new(CHAR_FIRMWARE_ID, "PET character ROM (4 KB)", false),
        ],
        media_slots: vec![],
        capabilities: CapabilitySet::with_all([
            known_capability("keyboard-input"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_declares_four_roms() {
        let p = profile_for(Model::Pet40Col);
        assert_eq!(p.firmware.len(), 4);
    }
}
