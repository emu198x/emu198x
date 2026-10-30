//! Atari 800XL family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    A800xlNtsc,
    A800xlPal,
    /// The 130XE adds a controller for 64 KiB of extended RAM behind PORTB.
    A130xeNtsc,
    A130xePal,
}

impl Model {
    pub const ALL: [Self; 4] = [
        Self::A800xlNtsc,
        Self::A800xlPal,
        Self::A130xeNtsc,
        Self::A130xePal,
    ];
    pub const VARIANT_IDS: [&'static str; 4] = [
        Self::A800xlNtsc.variant_id(),
        Self::A800xlPal.variant_id(),
        Self::A130xeNtsc.variant_id(),
        Self::A130xePal.variant_id(),
    ];

    /// The physical board's memory controller.
    #[must_use]
    pub const fn memory(self) -> common_atari_800xl::MemoryModel {
        match self {
            Self::A800xlNtsc | Self::A800xlPal => common_atari_800xl::MemoryModel::Xl64k,
            Self::A130xeNtsc | Self::A130xePal => common_atari_800xl::MemoryModel::Xe128k,
        }
    }

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "ntsc" => Some(Self::A800xlNtsc),
            "pal" => Some(Self::A800xlPal),
            "130xe" => Some(Self::A130xePal),
            _ => Self::ALL.into_iter().find(|model| model.variant_id() == id),
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        vec![
            emu198x_shell::FirmwareSource::optional(OS_FIRMWARE_ID, &["atarixl.rom"])
                .with_env_var("EMU198X_A800XL_OS"),
            emu198x_shell::FirmwareSource::optional(BASIC_FIRMWARE_ID, &["ataribas.rom"])
                .with_env_var("EMU198X_A800XL_BASIC"),
        ]
    }

    /// Existing native host budget, in colour clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::A800xlNtsc | Self::A130xeNtsc => 262 * 228,
            Self::A800xlPal | Self::A130xePal => 312 * 228,
        }
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A800xlNtsc => "atari-800xl-ntsc",
            Self::A800xlPal => "atari-800xl-pal",
            Self::A130xeNtsc => "atari-130xe-ntsc",
            Self::A130xePal => "atari-130xe-pal",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::A800xlNtsc => "Atari 800XL (NTSC)",
            Self::A800xlPal => "Atari 800XL (PAL)",
            Self::A130xeNtsc => "Atari 130XE (NTSC)",
            Self::A130xePal => "Atari 130XE (PAL)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::A800xlNtsc | Self::A130xeNtsc => Region::Ntsc,
            Self::A800xlPal | Self::A130xePal => Region::Pal,
        }
    }
}

pub const OS_FIRMWARE_ID: &str = "atari-800xl-os";
pub const BASIC_FIRMWARE_ID: &str = "atari-800xl-basic";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from(match model{Model::A800xlNtsc=>"atari-800xl",Model::A800xlPal=>"atari-800xl-pal",Model::A130xeNtsc=>"atari-130xe-ntsc",Model::A130xePal=>"atari-130xe-pal"}),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: match model {
            Model::A800xlNtsc | Model::A800xlPal => 1983,
            Model::A130xeNtsc | Model::A130xePal => 1985,
        },
        summary: match model.memory() {
            common_atari_800xl::MemoryModel::Xl64k => "Atari 800XL — 6502C + ANTIC + GTIA + POKEY + PIA, 64 KB RAM, optional 16 KB OS ROM + 8 KB BASIC ROM, optional cartridge.",
            common_atari_800xl::MemoryModel::Xe128k => "Atari 130XE — the 800XL's chips with 128 KB RAM: four 16 KB banks behind PORTB, separately for the CPU and ANTIC. Same OS and BASIC ROMs.",
        }
        .into(),
        // Runtime time and frame_ticks count colour clocks, two per CPU
        // cycle (Atari 400/800 Hardware Manual, 1982, section II; vendored
        // Atari800 src/antic.c uses 114 CPU cycles per 228-clock line).
        // Keep the regional rates aligned with machine::Region::cpu_hz.
        clock: ClockDesc::new("colour-clock", ClockRate::from_hz(match model.region() {
            Region::Pal => 2 * 1_773_447,
            _ => 2 * 1_789_772,
        })),
        firmware: vec![
            FirmwareRequirement::new(OS_FIRMWARE_ID, "Atari 800XL OS ROM (16 KB) — optional", true),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID, "Atari BASIC ROM (8 KB) — optional", true),
        ],
        media_slots: vec![
            MediaSlot::new(
                "cartridge-1",
                "Cartridge Slot",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
            MediaSlot::new(
                "program-1",
                "XEX Program",
                MediaKind::Program,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
            MediaSlot::new(
                "disk-1",
                "Disk Drive D1:",
                MediaKind::Disk,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
        ],
        capabilities: CapabilitySet::with_all([
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("variant-switch"),
            known_capability("keyboard-input"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_declares_two_optional_firmware() {
        let p = profile_for(Model::A800xlNtsc);
        assert_eq!(p.firmware.len(), 2);
        assert!(p.firmware.iter().all(|f| f.optional));
    }

    #[test]
    fn board_identities_are_distinct_while_profile_ids_and_aliases_stay_stable() {
        for (model, board, profile) in [
            (Model::A800xlNtsc, "atari-800xl", "atari-800xl-ntsc"),
            (Model::A800xlPal, "atari-800xl-pal", "atari-800xl-pal"),
            (Model::A130xeNtsc, "atari-130xe-ntsc", "atari-130xe-ntsc"),
            (Model::A130xePal, "atari-130xe-pal", "atari-130xe-pal"),
        ] {
            let p = profile_for(model);
            assert_eq!(p.machine_id.as_str(), board);
            assert_eq!(p.profile_id.as_str(), profile);
            assert_eq!(Model::from_variant_id(profile), Some(model));
        }
        assert_eq!(Model::from_variant_id("ntsc"), Some(Model::A800xlNtsc));
        assert_eq!(Model::from_variant_id("pal"), Some(Model::A800xlPal));
        assert_eq!(Model::from_variant_id("130xe"), Some(Model::A130xePal));
    }

    #[test]
    fn profile_declares_one_disk_slot() {
        let p = profile_for(Model::A800xlNtsc);
        let disks: Vec<_> = p
            .media_slots
            .iter()
            .filter(|slot| slot.kind == MediaKind::Disk)
            .collect();
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].id, "disk-1");
        assert!(!disks[0].required);
    }
}
