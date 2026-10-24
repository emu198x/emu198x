//! BBC Micro family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Model {
    /// BBC Micro Model B.
    #[default]
    BbcModelB,
    /// BBC Micro Model B+: 64 KB, a shadow screen and a WD1770 disc
    /// controller.
    BbcBPlus,
}

impl Model {
    pub const ALL: [Self; 2] = [Self::BbcModelB, Self::BbcBPlus];
    pub const VARIANT_IDS: [&'static str; 2] =
        [Self::BbcModelB.variant_id(), Self::BbcBPlus.variant_id()];
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }
    /// Whether the model has a disc controller, and so a DFS and drives.
    #[must_use]
    pub const fn has_disc(self) -> bool {
        matches!(self, Self::BbcBPlus)
    }

    /// The id of the model's 16 KB MOS.
    #[must_use]
    pub const fn mos_firmware_id(self) -> &'static str {
        match self {
            Self::BbcModelB => MOS_FIRMWARE_ID,
            Self::BbcBPlus => BPLUS_MOS_FIRMWARE_ID,
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        use emu198x_shell::FirmwareSource;
        let mos = match self {
            Self::BbcModelB => FirmwareSource::required(MOS_FIRMWARE_ID, &["os.rom"])
                .with_env_var("EMU198X_BBC_MOS"),
            Self::BbcBPlus => FirmwareSource::required(BPLUS_MOS_FIRMWARE_ID, &["bplus-os.rom"])
                .with_env_var("EMU198X_BBC_BPLUS_OS"),
        };
        let mut sources = vec![
            mos,
            FirmwareSource::optional(FONT_FIRMWARE_ID, &["saa5050.rom"])
                .with_env_var("EMU198X_BBC_SAA5050"),
            FirmwareSource::optional(BASIC_FIRMWARE_ID, &["basic.rom"])
                .with_env_var("EMU198X_BBC_BASIC"),
        ];
        if self.has_disc() {
            sources.push(
                FirmwareSource::optional(DFS_FIRMWARE_ID, &["dfs-2.26.rom"])
                    .with_env_var("EMU198X_BBC_DFS"),
            );
        }
        sources
    }
    /// Existing host frame budget; do not cross the frame-granular runtime boundary.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        39_936
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::BbcModelB => "acorn-bbc-micro-b",
            Self::BbcBPlus => "acorn-bbc-micro-b-plus",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::BbcModelB => "Acorn BBC Micro Model B",
            Self::BbcBPlus => "Acorn BBC Micro Model B+",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const FONT_FIRMWARE_ID: &str = "acorn-bbc-saa5050";
pub const BASIC_FIRMWARE_ID: &str = "acorn-bbc-basic";

pub const MOS_FIRMWARE_ID: &str = "acorn-bbc-mos";
/// The B+'s own MOS, OS 2.00.
pub const BPLUS_MOS_FIRMWARE_ID: &str = "acorn-bbc-bplus-mos";
/// The Acorn 1770 DFS, fitted in sideways slot 14 on a B+.
pub const DFS_FIRMWARE_ID: &str = "acorn-bbc-1770-dfs";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let (release_year, summary, mos) = match model {
        Model::BbcModelB => (
            1981,
            "Acorn BBC Micro Model B — 6502 + 6845 CRTC + Video ULA + 2× 6522 VIA + SN76489, 16 KB MOS ROM, 16 KB sideways ROM slots.",
            "BBC MOS ROM (16 KB)",
        ),
        Model::BbcBPlus => (
            1985,
            "Acorn BBC Micro Model B+ — the Model B with 64 KB: a 20 KB shadow screen, 12 KB of paged RAM, and a WD1770 disc controller with the Acorn 1770 DFS.",
            "B+ MOS ROM, OS 2.00 (16 KB)",
        ),
    };
    let mut firmware = vec![
        FirmwareRequirement::new(model.mos_firmware_id(), mos, false),
        FirmwareRequirement::new(FONT_FIRMWARE_ID, "SAA5050 teletext character ROM", true),
        FirmwareRequirement::new(
            BASIC_FIRMWARE_ID,
            "BASIC language ROM (window default in bank 15)",
            true,
        ),
    ];
    let mut media_slots = vec![MediaSlot::new(
        "tape-1",
        "Cassette Tape",
        MediaKind::Tape,
        false,
        WritebackPolicy::SidecarOnly,
    )];
    let mut capabilities = CapabilitySet::with_all([
        known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
        known_capability("keyboard-input"),
        known_capability("scripted-input"),
    ]);
    if model.has_disc() {
        firmware.push(FirmwareRequirement::new(
            DFS_FIRMWARE_ID,
            "Acorn 1770 DFS ROM (16 KB, bank 14)",
            true,
        ));
        for (slot, label) in DISK_SLOTS.into_iter().zip(["Disc Drive 0", "Disc Drive 1"]) {
            media_slots.push(MediaSlot::new(
                slot,
                label,
                MediaKind::Disk,
                false,
                WritebackPolicy::InMemoryOnly,
            ));
        }
        capabilities.insert(known_capability("disk-input"));
        capabilities.insert(known_capability(
            emu198x_shell::capability::ids::MEDIA_EXPORT,
        ));
    }
    MachineProfile {
        machine_id: MachineId::from("acorn-bbc-micro"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year,
        summary: summary.into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(2_000_000)),
        firmware,
        media_slots,
        capabilities,
    }
}

/// The disc slots, by drive number.
pub const DISK_SLOTS: [&str; 2] = ["drive-0", "drive-1"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_b_plus_adds_its_mos_the_dfs_and_two_drives() {
        let p = profile_for(Model::BbcBPlus);
        let firmware: Vec<_> = p.firmware.iter().map(|f| f.id.as_ref()).collect();
        assert_eq!(
            firmware,
            [
                BPLUS_MOS_FIRMWARE_ID,
                FONT_FIRMWARE_ID,
                BASIC_FIRMWARE_ID,
                DFS_FIRMWARE_ID
            ]
        );
        let slots: Vec<_> = p.media_slots.iter().map(|s| s.id.as_ref()).collect();
        assert_eq!(slots, ["tape-1", "drive-0", "drive-1"]);
        assert_eq!(
            Model::from_variant_id("acorn-bbc-micro-b-plus"),
            Some(Model::BbcBPlus)
        );
        assert_eq!(profiles().len(), 2);
        assert_eq!(profile_for(Model::BbcModelB).media_slots.len(), 1);
    }

    #[test]
    fn profile_declares_mos_firmware() {
        let p = profile_for(Model::BbcModelB);
        assert_eq!(p.firmware.len(), 3);
        assert!(!p.firmware[0].optional);
        assert!(p.firmware[1..].iter().all(|image| image.optional));
    }
}
