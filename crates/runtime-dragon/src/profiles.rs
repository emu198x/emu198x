//! Dragon family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

/// Dragon family model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Dragon Data Dragon 32, PAL.
    Dragon32Pal,
}

impl Model {
    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "dragon-32-pal",
        }
    }

    /// User-facing display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "Dragon 32 (PAL)",
        }
    }
}

/// Returns the Dragon family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Dragon32Pal)]
}

/// Returns one Dragon profile.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::Dragon32Pal => MachineProfile {
            machine_id: MachineId::from("dragon"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Dragon,
            region: Region::Pal,
            support_tier: SupportTier::Boots,
            release_year: 1982,
            summary: "Dragon 32 PAL bring-up runtime. It boots the real BASIC ROM through the shared MC6809/PIA/SAM/VDG machine substrate, mounts CAS tapes, and emits the current MC6847 text, semigraphics, or graphics framebuffer; audio, joystick, and Dragon 64 remain pending.".into(),
            clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(894_886)),
            firmware: vec![FirmwareRequirement::new(
                "dragon32-basic-rom",
                "Dragon 32 BASIC ROM",
                false,
            )],
            media_slots: vec![MediaSlot::new(
                "tape-1",
                "Cassette",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            capabilities: CapabilitySet::with_all([
                known_capability("cassette-media"),
                known_capability("keyboard-matrix"),
                known_capability("scripted-input"),
                known_capability("video-framebuffer"),
            ]),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_has_unique_profile_ids() {
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
    fn dragon32_profile_declares_required_basic_rom() {
        let profile = profile_for(Model::Dragon32Pal);
        assert_eq!(profile.family, Family::Dragon);
        assert_eq!(profile.region, Region::Pal);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(profile.firmware[0].id.as_ref(), "dragon32-basic-rom");
        assert_eq!(profile.media_slots.len(), 1);
        assert_eq!(profile.media_slots[0].id.as_ref(), "tape-1");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Tape);
    }
}
