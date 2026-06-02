//! ColecoVision family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

/// Supported ColecoVision models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// ColecoVision NTSC (60 Hz / 262 lines).
    CvNtsc,
    /// ColecoVision PAL (50 Hz / 313 lines).
    CvPal,
}

impl Model {
    /// Stable model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::CvNtsc => "coleco-colecovision-ntsc",
            Self::CvPal => "coleco-colecovision-pal",
        }
    }

    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    /// User-facing display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::CvNtsc => "Coleco ColecoVision (NTSC)",
            Self::CvPal => "Coleco ColecoVision (PAL)",
        }
    }

    /// Region.
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::CvNtsc => Region::Ntsc,
            Self::CvPal => Region::Pal,
        }
    }
}

/// Stable BIOS firmware identifier.
pub const BIOS_FIRMWARE_ID: &str = "colecovision-bios";

/// Returns the initial ColecoVision family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::CvNtsc), profile_for(Model::CvPal)]
}

/// Returns the profile metadata for one model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("coleco-colecovision"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1982,
        summary: "ColecoVision — Z80A + TMS9918A + SN76489, 1 KB RAM, BIOS-driven boot, cartridge ROM support.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "ColecoVision BIOS ROM (8 KB)",
            false,
        )],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            false,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("controller-input"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_ids_are_unique() {
        let profiles = profiles();
        let mut ids: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn ntsc_declares_bios_firmware() {
        let profile = profile_for(Model::CvNtsc);
        assert_eq!(profile.region, Region::Ntsc);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(profile.firmware[0].id.as_ref(), BIOS_FIRMWARE_ID);
        assert_eq!(profile.media_slots.len(), 1);
    }
}
