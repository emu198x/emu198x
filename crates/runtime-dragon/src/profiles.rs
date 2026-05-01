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
    /// Dragon Data Dragon 64, PAL.
    Dragon64Pal,
}

impl Model {
    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "dragon-32-pal",
            Self::Dragon64Pal => "dragon-64-pal",
        }
    }

    /// User-facing display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "Dragon 32 (PAL)",
            Self::Dragon64Pal => "Dragon 64 (PAL)",
        }
    }

    /// Firmware image identifier used by the shared shell.
    #[must_use]
    pub const fn firmware_id(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "dragon32-basic-rom",
            Self::Dragon64Pal => "dragon64-compatible-rom",
        }
    }

    const fn firmware_label(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "Dragon 32 BASIC ROM",
            Self::Dragon64Pal => "Dragon 64 BASIC ROM",
        }
    }
}

/// Returns the Dragon family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Dragon32Pal),
        profile_for(Model::Dragon64Pal),
    ]
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
            summary: "Dragon 32 PAL bring-up runtime. It boots the real BASIC ROM through the shared MC6809/PIA/SAM/VDG machine substrate, mounts CAS tapes, ROM/DGN cartridges, and PC-Dragon PAK snapshots, emits the current MC6847 text, semigraphics, or graphics framebuffer, produces mono audio from the PIA DAC/mux path, and exposes Dragon analogue joystick hardware; Dragon 64 remains pending.".into(),
            clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(894_886)),
            firmware: vec![FirmwareRequirement::new(
                "dragon32-basic-rom",
                "Dragon 32 BASIC ROM",
                false,
            )],
            media_slots: vec![
                MediaSlot::new(
                    "tape-1",
                    "Cassette",
                    MediaKind::Tape,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
                MediaSlot::new(
                    "cartridge-1",
                    "Cartridge",
                    MediaKind::Cartridge,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
                MediaSlot::new(
                    "snapshot-1",
                    "PC-Dragon snapshot",
                    MediaKind::Snapshot,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
                MediaSlot::new(
                    "program-1",
                    "DragonDOS binary program",
                    MediaKind::Program,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
            ],
            capabilities: CapabilitySet::with_all([
                known_capability("cassette-media"),
                known_capability("cartridge-media"),
                known_capability("joystick-input"),
                known_capability("keyboard-matrix"),
                known_capability("scripted-input"),
                known_capability("video-framebuffer"),
            ]),
        },
        Model::Dragon64Pal => MachineProfile {
            machine_id: MachineId::from("dragon"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Dragon,
            region: Region::Pal,
            support_tier: SupportTier::Boots,
            release_year: 1983,
            summary: "Dragon 64 PAL runtime. It cold-boots in Dragon 32-compatible mode from the compatible BASIC ROM, switches to the high BASIC ROM for EXEC 48000 64-mode entry, adds the Dragon 64 ACIA decode and SAM-backed 64K RAM paging, and keeps the same cassette, cartridge, snapshot, program, keyboard, joystick, framebuffer, and mono audio surfaces as the Dragon 32 profile.".into(),
            clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(894_886)),
            firmware: vec![
                FirmwareRequirement::new(
                    model.firmware_id(),
                    "Dragon 64 compatible-mode BASIC ROM",
                    false,
                ),
                FirmwareRequirement::new("dragon64-basic-rom", model.firmware_label(), false),
            ],
            media_slots: vec![
                MediaSlot::new(
                    "tape-1",
                    "Cassette",
                    MediaKind::Tape,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
                MediaSlot::new(
                    "cartridge-1",
                    "Cartridge",
                    MediaKind::Cartridge,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
                MediaSlot::new(
                    "snapshot-1",
                    "PC-Dragon snapshot",
                    MediaKind::Snapshot,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
                MediaSlot::new(
                    "program-1",
                    "DragonDOS binary program",
                    MediaKind::Program,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
            ],
            capabilities: CapabilitySet::with_all([
                known_capability("cassette-media"),
                known_capability("cartridge-media"),
                known_capability("joystick-input"),
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
    fn dragon_profiles_declare_required_basic_roms() {
        let profile = profile_for(Model::Dragon32Pal);
        assert_eq!(profile.family, Family::Dragon);
        assert_eq!(profile.region, Region::Pal);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(profile.firmware[0].id.as_ref(), "dragon32-basic-rom");
        assert_eq!(profile.media_slots.len(), 4);
        assert_eq!(profile.media_slots[0].id.as_ref(), "tape-1");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Tape);
        assert_eq!(profile.media_slots[1].id.as_ref(), "cartridge-1");
        assert_eq!(profile.media_slots[1].kind, MediaKind::Cartridge);
        assert_eq!(profile.media_slots[2].id.as_ref(), "snapshot-1");
        assert_eq!(profile.media_slots[2].kind, MediaKind::Snapshot);
        assert_eq!(profile.media_slots[3].id.as_ref(), "program-1");
        assert_eq!(profile.media_slots[3].kind, MediaKind::Program);

        let profile = profile_for(Model::Dragon64Pal);
        assert_eq!(profile.family, Family::Dragon);
        assert_eq!(profile.region, Region::Pal);
        assert_eq!(profile.firmware.len(), 2);
        assert_eq!(profile.firmware[0].id.as_ref(), "dragon64-compatible-rom");
        assert!(!profile.firmware[0].optional);
        assert_eq!(profile.firmware[1].id.as_ref(), "dragon64-basic-rom");
        assert!(!profile.firmware[1].optional);
        assert_eq!(profile.media_slots.len(), 4);
    }
}
