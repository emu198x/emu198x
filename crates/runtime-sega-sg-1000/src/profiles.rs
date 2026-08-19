//! SG-1000 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, WritebackPolicy, known_capability,
};

/// Supported SG-1000 / SC-3000 family models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// SG-1000 NTSC (60 Hz / 262 lines).
    Sg1000Ntsc,
    /// SG-1000 PAL (50 Hz / 313 lines).
    Sg1000Pal,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Sg1000Ntsc => "sega-sg-1000-ntsc",
            Self::Sg1000Pal => "sega-sg-1000-pal",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Sg1000Ntsc => "Sega SG-1000 (NTSC)",
            Self::Sg1000Pal => "Sega SG-1000 (PAL)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Sg1000Ntsc => Region::Ntsc,
            Self::Sg1000Pal => Region::Pal,
        }
    }
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Sg1000Ntsc),
        profile_for(Model::Sg1000Pal),
    ]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sega-sg-1000"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1983,
        summary: "Sega SG-1000 — Z80A + TMS9918A + SN76489, 1 KB RAM, BIOS-less cartridge boot. Predecessor to the Master System.".into(),
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
        let mut ids: Vec<&str> = profiles.iter().map(|p| p.profile_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn ntsc_profile_uses_ntsc_region() {
        let p = profile_for(Model::Sg1000Ntsc);
        assert_eq!(p.region, Region::Ntsc);
        assert!(p.firmware.is_empty());
        assert_eq!(p.media_slots.len(), 1);
        assert!(p.media_slots[0].required);
    }
}
