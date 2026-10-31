//! PET family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// 32K PET 2001N / 3032 discrete-video board.
    Pet2001n32k,
    /// PET 4032 CRTC / graphics-keyboard board.
    Pet40Col,
    /// PET 8032 with 80-column display.
    Pet80Col,
}

impl Model {
    pub const ALL: [Self; 3] = [Self::Pet2001n32k, Self::Pet40Col, Self::Pet80Col];
    pub const VARIANT_IDS: [&'static str; 3] = [
        "commodore-pet-2001n-32k",
        "commodore-pet-40col",
        "commodore-pet-80col",
    ];
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "commodore-pet-2001n-32k" | "3032" => Some(Self::Pet2001n32k),
            "commodore-pet-40col" | "40" => Some(Self::Pet40Col),
            "commodore-pet-80col" | "80" => Some(Self::Pet80Col),
            _ => None,
        }
    }
    /// Existing host frame budget in CPU cycles.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Pet2001n32k => 16_640,
            Self::Pet40Col => 20_000,
            Self::Pet80Col => 19_992,
        }
    }
    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        use emu198x_shell::FirmwareSource;
        type Names = &'static [&'static str];
        let (kernal, basic, editor): (Names, Names, Names) = match self {
            Self::Pet2001n32k => (
                &["kernal-2.901465-03.bin"],
                &["basic-2.901465-01-02.bin"],
                &["edit-2-n.901447-24.bin"],
            ),
            Self::Pet40Col => (
                &["kernal-4.901465-22.bin"],
                &["basic-4.901465-23-20-21.bin"],
                &["edit-4-40-n-50Hz.901498-01.bin"],
            ),
            Self::Pet80Col => (
                &["kernal-4.901465-22.bin"],
                &["basic-4.901465-23-20-21.bin"],
                &["edit-4-80-b-50Hz.901474-04.bin"],
            ),
        };
        vec![
            FirmwareSource::required(KERNAL_FIRMWARE_ID, kernal).with_env_var("EMU198X_PET_KERNAL"),
            FirmwareSource::required(BASIC_FIRMWARE_ID, basic).with_env_var("EMU198X_PET_BASIC"),
            FirmwareSource::required(EDITOR_FIRMWARE_ID, editor).with_env_var("EMU198X_PET_EDITOR"),
            FirmwareSource::required(CHAR_FIRMWARE_ID, &["characters-2.901447-10.bin"])
                .with_env_var("EMU198X_PET_CHAR"),
        ]
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Pet2001n32k => "commodore-pet-2001n-32k",
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
            Self::Pet2001n32k => "Commodore PET 2001N / 3032 (32K)",
            Self::Pet40Col => "Commodore PET 4032 (50 Hz, graphics keyboard)",
            Self::Pet80Col => "Commodore PET 8032 (50 Hz, business keyboard)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Other
    }
    pub const fn board(self) -> common_commodore_pet::Board {
        match self {
            Self::Pet2001n32k => common_commodore_pet::Board::Pet2001n32k,
            Self::Pet40Col => common_commodore_pet::Board::Pet4032,
            Self::Pet80Col => common_commodore_pet::Board::Pet8032,
        }
    }
    pub const fn machine_id(self) -> &'static str {
        match self {
            Self::Pet2001n32k => "commodore-pet-2001n-32k",
            Self::Pet40Col => "commodore-pet-4032",
            Self::Pet80Col => "commodore-pet-8032",
        }
    }
    pub const fn basic_size(self) -> usize {
        if matches!(self, Self::Pet2001n32k) {
            8192
        } else {
            12288
        }
    }
    #[must_use]
    pub const fn screen_chars(self) -> u32 {
        match self {
            Self::Pet2001n32k | Self::Pet40Col => 40,
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
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from(model.machine_id()),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: if model == Model::Pet2001n32k { 1979 } else { 1980 },
        summary: "32K PET board with matching BASIC/editor firmware and keyboard. Second PIA, IEEE-488 peripherals and cassette remain incomplete.".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(1_000_000)),
        firmware: vec![
            FirmwareRequirement::new(KERNAL_FIRMWARE_ID, "PET KERNAL ROM (4 KB)", false),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID, "Board-matched PET BASIC ROM (8 KB BASIC 2 / 12 KB BASIC 4)", false),
            FirmwareRequirement::new(EDITOR_FIRMWARE_ID, "PET editor ROM (2 KB)", false),
            FirmwareRequirement::new(CHAR_FIRMWARE_ID, "PET 901447-10 character ROM (2 KB)", false),
        ],
        media_slots: vec![MediaSlot::new(
            "program-1",
            "Program (.prg)",
            MediaKind::Program,
            false,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
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
