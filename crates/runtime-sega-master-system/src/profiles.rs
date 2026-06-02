//! Sega Master System / Game Gear family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Master System NTSC.
    SmsNtsc,
    /// Master System PAL.
    SmsPal,
    /// Sega Game Gear (NTSC handheld variant).
    GameGear,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::SmsNtsc => "sega-master-system-ntsc",
            Self::SmsPal => "sega-master-system-pal",
            Self::GameGear => "sega-game-gear",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::SmsNtsc => "Sega Master System (NTSC)",
            Self::SmsPal => "Sega Master System (PAL)",
            Self::GameGear => "Sega Game Gear",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::SmsPal => Region::Pal,
            _ => Region::Ntsc,
        }
    }
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::SmsNtsc),
        profile_for(Model::SmsPal),
        profile_for(Model::GameGear),
    ]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let (machine_id, summary) = match model {
        Model::SmsNtsc | Model::SmsPal => (
            "sega-master-system",
            "Sega Master System — Z80A + Sega VDP + SN76489, 8 KB RAM, Sega mapper cartridge boot.",
        ),
        Model::GameGear => (
            "sega-game-gear",
            "Sega Game Gear — Z80A + Sega VDP (160×144 LCD crop) + SN76489 stereo, 8 KB RAM.",
        ),
    };
    MachineProfile {
        machine_id: MachineId::from(machine_id),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1985,
        summary: summary.into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware: vec![],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            true,
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
            .map(|p| p.profile_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn pal_profile_uses_pal_region() {
        let p = profile_for(Model::SmsPal);
        assert_eq!(p.region, Region::Pal);
    }
}
