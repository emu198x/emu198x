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
    /// SG-1000 II (1984): the same machine re-cased, with detachable
    /// controllers and the My Card adapter port. Japan, NTSC.
    Sg1000IiNtsc,
}

impl Model {
    pub const ALL: [Self; 3] = [Self::Sg1000Ntsc, Self::Sg1000Pal, Self::Sg1000IiNtsc];
    pub const VARIANT_IDS: [&'static str; 3] = [
        Self::Sg1000Ntsc.variant_id(),
        Self::Sg1000Pal.variant_id(),
        Self::Sg1000IiNtsc.variant_id(),
    ];

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
        Vec::new()
    }

    /// Existing native host frame budget, in CPU clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Sg1000Ntsc | Self::Sg1000IiNtsc => 228 * 262,
            Self::Sg1000Pal => 228 * 313,
        }
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Sg1000Ntsc => "sega-sg-1000-ntsc",
            Self::Sg1000Pal => "sega-sg-1000-pal",
            Self::Sg1000IiNtsc => "sega-sg-1000-ii-ntsc",
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
            Self::Sg1000IiNtsc => "Sega SG-1000 II (NTSC)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Sg1000Ntsc | Self::Sg1000IiNtsc => Region::Ntsc,
            Self::Sg1000Pal => Region::Pal,
        }
    }
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sega-sg-1000"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: match model {
            Model::Sg1000Ntsc | Model::Sg1000Pal => 1983,
            Model::Sg1000IiNtsc => 1984,
        },
        summary: match model {
            Model::Sg1000Ntsc | Model::Sg1000Pal => "Sega SG-1000 — Z80A + TMS9918A + SN76489, 1 KB RAM, BIOS-less cartridge boot. Predecessor to the Master System.",
            Model::Sg1000IiNtsc => "Sega SG-1000 II (1984) — the SG-1000's chips in a new case with detachable controllers and a My Card adapter port. Electrically the same machine.",
        }
        .into(),
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
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("variant-switch"),
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
    fn the_sg_1000_ii_is_the_same_machine_in_a_new_case() {
        let ii = profile_for(Model::Sg1000IiNtsc);
        assert_eq!(ii.machine_id.as_str(), "sega-sg-1000");
        assert_eq!(ii.region, Region::Ntsc);
        assert_eq!(ii.release_year, 1984);
        assert_eq!(
            Model::Sg1000IiNtsc.frame_ticks(),
            Model::Sg1000Ntsc.frame_ticks()
        );
        assert_eq!(
            Model::from_variant_id("sega-sg-1000-ii-ntsc"),
            Some(Model::Sg1000IiNtsc)
        );
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
