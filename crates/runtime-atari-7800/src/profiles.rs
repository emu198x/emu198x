//! Atari 7800 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Model {
    #[default]
    A7800Ntsc,
    A7800Pal,
}

impl Model {
    /// Existing regional presets.
    pub const ALL: [Self; 2] = [Self::A7800Ntsc, Self::A7800Pal];
    pub const VARIANT_IDS: [&'static str; 2] = ["atari-7800-ntsc", "atari-7800-pal"];
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "atari-7800-ntsc" | "ntsc" => Some(Self::A7800Ntsc),
            "atari-7800-pal" | "pal" => Some(Self::A7800Pal),
            _ => None,
        }
    }
    /// Existing host budget in colour clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::A7800Ntsc => 262 * 228,
            Self::A7800Pal => 312 * 228,
        }
    }
    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        Vec::new()
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A7800Ntsc => "atari-7800-ntsc",
            Self::A7800Pal => "atari-7800-pal",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::A7800Ntsc => "Atari 7800 (NTSC)",
            Self::A7800Pal => "Atari 7800 (PAL)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::A7800Ntsc => Region::Ntsc,
            Self::A7800Pal => Region::Pal,
        }
    }
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::A7800Ntsc), profile_for(Model::A7800Pal)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("atari-7800"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1986,
        summary: "Atari 7800 — 6502C + MARIA video + TIA audio + 4 KB RAM, cartridge required, BIOS-less in v1.".into(),
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
}
