//! NES family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, FirmwareSource, MachineId,
    MachineProfile, MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

/// The Famicom Disk System BIOS's firmware id.
pub const FDS_BIOS_FIRMWARE_ID: &str = "fds-bios";

/// The media slot a Famicom Disk System disk goes in.
pub const FDS_DISK_SLOT: &str = "disk-1";

/// Supported NES models in the fresh-workspace bootstrap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Model {
    /// Nintendo Entertainment System / Famicom NTSC baseline.
    #[default]
    NesNtsc,
    /// PAL NES, using the existing 2A07/2C07 machine timing.
    NesPal,
}

impl Model {
    pub const ALL: [Self; 2] = [Self::NesNtsc, Self::NesPal];
    pub const VARIANT_IDS: [&'static str; 2] =
        [Self::NesNtsc.variant_id(), Self::NesPal.variant_id()];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "nintendo-nes-ntsc" | "ntsc" => Some(Self::NesNtsc),
            "nintendo-nes-pal" | "pal" => Some(Self::NesPal),
            _ => None,
        }
    }

    /// Existing host budget, in PPU dots.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        341 * (self.machine_region().pre_render_line() as u64 + 1)
    }

    #[must_use]
    pub const fn machine_region(self) -> machine_nintendo_nes::Region {
        match self {
            Self::NesNtsc => machine_nintendo_nes::Region::Ntsc,
            Self::NesPal => machine_nintendo_nes::Region::Pal,
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::NesNtsc => Region::Ntsc,
            Self::NesPal => Region::Pal,
        }
    }

    /// Dot rates recorded in the NES clock-topology decision.
    #[must_use]
    pub const fn ppu_dot_hz(self) -> u64 {
        match self {
            Self::NesNtsc => 5_369_318,
            Self::NesPal => 5_320_342,
        }
    }

    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::NesNtsc => "nintendo-nes-ntsc",
            Self::NesPal => "nintendo-nes-pal",
        }
    }

    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    /// Firmware this model can use: the FDS BIOS, on the NTSC
    /// (Famicom) model only, since the Disk System never left Japan.
    #[must_use]
    pub fn firmware_sources(self) -> Vec<FirmwareSource> {
        match self {
            Self::NesNtsc => vec![FirmwareSource {
                id: FDS_BIOS_FIRMWARE_ID,
                candidates: &["disksys.rom", "fds-bios.rom"],
                optional: true,
                env_var: Some("EMU198X_FDS_BIOS"),
            }],
            Self::NesPal => Vec::new(),
        }
    }

    /// User-facing display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::NesNtsc => "Nintendo NES (NTSC)",
            Self::NesPal => "Nintendo NES (PAL)",
        }
    }
}

/// Returns the initial NES family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

/// Returns the profile metadata for one NES model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
            machine_id: MachineId::from("nintendo-nes"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Nes,
            region: model.region(),
            release_year: if model == Model::NesNtsc { 1985 } else { 1986 },
            summary: "NES NTSC/PAL runtime with headless cartridge boot, NROM/MMC1/UxROM/CNROM/MMC3/MMC5/AxROM/Color Dreams/VRC2a/Action 53/BxROM/NINA-001/Sunsoft-4/Camerica mapper support, Famicom Disk System disks on the NTSC model, region-specific CPU/PPU/APU execution, RGBA frame output, mono audio, snapshots, and controller input.".into(),
            clock: ClockDesc::new("ppu-dot", ClockRate::from_hz(model.ppu_dot_hz())),
            firmware: match model {
                Model::NesNtsc => vec![FirmwareRequirement::new(
                    FDS_BIOS_FIRMWARE_ID,
                    "Famicom Disk System BIOS (8 KB) — optional, needed for disks",
                    true,
                )],
                Model::NesPal => vec![],
            },
            media_slots: media_slots(model),
            capabilities: CapabilitySet::with_all([
                known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
                known_capability("variant-switch"),
                known_capability("controller-input"),
                known_capability("scripted-input"),
                known_capability("snapshot-export"),
                known_capability("snapshot-import"),
            ]),
    }
}

fn media_slots(model: Model) -> Vec<MediaSlot> {
    let mut slots = vec![MediaSlot::new(
        "cartridge-1",
        "Cartridge Slot",
        MediaKind::Cartridge,
        false,
        WritebackPolicy::InMemoryOnly,
    )];
    if model == Model::NesNtsc {
        slots.push(MediaSlot::new(
            FDS_DISK_SLOT,
            "Famicom Disk System",
            MediaKind::Disk,
            false,
            WritebackPolicy::InMemoryOnly,
        ));
    }
    slots
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
        assert_eq!(profile.firmware.len(), 1);
        assert!(
            profile.firmware[0].optional,
            "cartridges boot without the BIOS"
        );
        assert_eq!(profile.media_slots.len(), 2);
        assert_eq!(profile.media_slots[0].id.as_ref(), "cartridge-1");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Cartridge);
        assert_eq!(profile.media_slots[1].id.as_ref(), FDS_DISK_SLOT);
        assert_eq!(profile.media_slots[1].kind, MediaKind::Disk);
        let pal = profile_for(Model::NesPal);
        assert!(pal.firmware.is_empty());
        assert_eq!(pal.media_slots.len(), 1);
    }
}
