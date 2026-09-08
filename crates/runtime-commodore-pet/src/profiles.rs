//! PET family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// PET 4032 / 8032 with 40-column display.
    Pet40Col,
    /// PET 8032 with 80-column display.
    Pet80Col,
}

impl Model {
    pub const ALL: [Self; 2] = [Self::Pet40Col, Self::Pet80Col];
    pub const VARIANT_IDS: [&'static str; 2] = ["commodore-pet-40col", "commodore-pet-80col"];
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "commodore-pet-40col" | "40" => Some(Self::Pet40Col),
            "commodore-pet-80col" | "80" => Some(Self::Pet80Col),
            _ => None,
        }
    }
    /// Existing host frame budget in CPU cycles.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        20_000
    }
    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        use emu198x_shell::FirmwareSource;
        vec![
            FirmwareSource::required(KERNAL_FIRMWARE_ID, &["kernal.rom"])
                .with_env_var("EMU198X_PET_KERNAL"),
            FirmwareSource::required(BASIC_FIRMWARE_ID, &["basic.rom"])
                .with_env_var("EMU198X_PET_BASIC"),
            FirmwareSource::required(EDITOR_FIRMWARE_ID, &["editor.rom"])
                .with_env_var("EMU198X_PET_EDITOR"),
            FirmwareSource::required(CHAR_FIRMWARE_ID, &["chargen.rom"])
                .with_env_var("EMU198X_PET_CHAR"),
        ]
    }

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
        release_year: 1977,
        summary: "Commodore PET / CBM — 6502 + 6845 CRTC + PIA/VIA, KERNAL + BASIC + editor + character ROMs, 40 or 80 column display.".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(1_000_000)),
        firmware: vec![
            FirmwareRequirement::new(KERNAL_FIRMWARE_ID, "PET KERNAL ROM (4 KB)", false),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID, "PET BASIC ROM (8 KB)", false),
            FirmwareRequirement::new(EDITOR_FIRMWARE_ID, "PET editor ROM (2 KB)", false),
            FirmwareRequirement::new(CHAR_FIRMWARE_ID, "PET character ROM (4 KB)", false),
        ],
        media_slots: vec![MediaSlot::new(
            "program-1",
            "Program (.prg)",
            MediaKind::Program,
            false,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("variant-switch"),
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
