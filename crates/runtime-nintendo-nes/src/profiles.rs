//! NES family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

/// Supported NES models in the fresh-workspace bootstrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Nintendo Entertainment System / Famicom NTSC baseline.
    NesNtsc,
}

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::NesNtsc => "nintendo-nes-ntsc",
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
            Self::NesNtsc => "Nintendo NES (NTSC)",
        }
    }
}

/// Returns the initial NES family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::NesNtsc)]
}

/// Returns the profile metadata for one NES model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::NesNtsc => MachineProfile {
            machine_id: MachineId::from("nintendo-nes"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Nes,
            region: Region::Ntsc,
            support_tier: SupportTier::Boots,
            release_year: 1985,
            summary: "NTSC NES baseline with headless cartridge boot, NROM mapper support, live 2A03/2C02/APU execution, RGBA frame output, mono audio, and controller input.".into(),
            clock: ClockDesc::new("ppu-dot", ClockRate::from_hz(5_369_318)),
            firmware: vec![],
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
        },
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
    fn nes_profile_declares_cartridge_bootstrap_scope() {
        let profile = profile_for(Model::NesNtsc);
        assert_eq!(profile.family, Family::Nes);
        assert_eq!(profile.region, Region::Ntsc);
        assert_eq!(profile.support_tier, SupportTier::Boots);
        assert!(profile.firmware.is_empty());
        assert_eq!(profile.media_slots.len(), 1);
        assert_eq!(profile.media_slots[0].id.as_ref(), "cartridge-1");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Cartridge);
    }
}
