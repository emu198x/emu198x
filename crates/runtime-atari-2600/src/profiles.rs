//! Atari 2600 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Model {
    /// Atari 2600 NTSC.
    #[default]
    Vcs2600Ntsc,
    /// Atari 2600 PAL.
    Vcs2600Pal,
}

impl Model {
    /// Existing regional presets.
    pub const ALL: [Self; 2] = [Self::Vcs2600Ntsc, Self::Vcs2600Pal];
    pub const VARIANT_IDS: [&'static str; 2] = ["atari-2600-ntsc", "atari-2600-pal"];
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "atari-2600-ntsc" | "ntsc" => Some(Self::Vcs2600Ntsc),
            "atari-2600-pal" | "pal" => Some(Self::Vcs2600Pal),
            _ => None,
        }
    }
    /// Existing host budget in colour clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Vcs2600Ntsc => 262 * 228,
            Self::Vcs2600Pal => 312 * 228,
        }
    }
    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        Vec::new()
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Vcs2600Ntsc => "atari-2600-ntsc",
            Self::Vcs2600Pal => "atari-2600-pal",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Vcs2600Ntsc => "Atari 2600 (NTSC)",
            Self::Vcs2600Pal => "Atari 2600 (PAL)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Vcs2600Ntsc => Region::Ntsc,
            Self::Vcs2600Pal => Region::Pal,
        }
    }
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Vcs2600Ntsc),
        profile_for(Model::Vcs2600Pal),
    ]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("atari-2600"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1977,
        summary: "Atari 2600 / VCS — 6507 + TIA + 6532 RIOT, 128 bytes RAM, cartridge boot (NROM / banking variants).".into(),
        clock: ClockDesc::new("colour-clock", ClockRate::from_hz(3_579_545)),
        firmware: vec![],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            true,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
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
    fn pal_profile_uses_pal_region() {
        assert_eq!(profile_for(Model::Vcs2600Pal).region, Region::Pal);
    }
}
