//! BBC Micro family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// BBC Micro Model B.
    BbcModelB,
}

impl Model {
    pub const ALL: [Self; 1] = [Self::BbcModelB];
    pub const VARIANT_IDS: [&'static str; 1] = [Self::BbcModelB.variant_id()];
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }
    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        use emu198x_shell::FirmwareSource;
        vec![
            FirmwareSource::required(MOS_FIRMWARE_ID, &["os.rom"]).with_env_var("EMU198X_BBC_MOS"),
            FirmwareSource::optional(FONT_FIRMWARE_ID, &["saa5050.rom"])
                .with_env_var("EMU198X_BBC_SAA5050"),
            FirmwareSource::optional(BASIC_FIRMWARE_ID, &["basic.rom"])
                .with_env_var("EMU198X_BBC_BASIC"),
        ]
    }
    /// Existing host frame budget; do not cross the frame-granular runtime boundary.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        39_936
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        "acorn-bbc-micro-b"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        "Acorn BBC Micro Model B"
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const FONT_FIRMWARE_ID: &str = "acorn-bbc-saa5050";
pub const BASIC_FIRMWARE_ID: &str = "acorn-bbc-basic";

pub const MOS_FIRMWARE_ID: &str = "acorn-bbc-mos";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("acorn-bbc-micro"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1981,
        summary: "Acorn BBC Micro Model B — 6502 + 6845 CRTC + Video ULA + 2× 6522 VIA + SN76489, 16 KB MOS ROM, 16 KB sideways ROM slots.".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(2_000_000)),
        firmware: vec![
            FirmwareRequirement::new(MOS_FIRMWARE_ID,"BBC MOS ROM (16 KB)",false),
            FirmwareRequirement::new(FONT_FIRMWARE_ID,"SAA5050 teletext character ROM",true),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID,"BASIC language ROM (window default in bank 15)",true),
        ],
        media_slots: vec![MediaSlot::new(
            "tape-1",
            "Cassette Tape",
            MediaKind::Tape,
            false,
            WritebackPolicy::SidecarOnly,
        )],
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
    fn profile_declares_mos_firmware() {
        let p = profile_for(Model::BbcModelB);
        assert_eq!(p.firmware.len(), 3);
        assert!(!p.firmware[0].optional);
        assert!(p.firmware[1..].iter().all(|image| image.optional));
    }
}
