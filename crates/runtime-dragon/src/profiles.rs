//! Dragon family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};
use serde::{Deserialize, Serialize};

/// Dragon family model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Model {
    /// Dragon Data Dragon 32, PAL.
    Dragon32Pal,
    /// Dragon Data Dragon 64, PAL.
    Dragon64Pal,
    /// Tandy Color Computer (1980), NTSC: 16 or 32 KiB, Color BASIC with
    /// Extended Color BASIC if fitted.
    CoCo1Ntsc,
    /// Tandy Color Computer 2 (1983), NTSC, with 64 KiB.
    CoCo2Ntsc,
}

impl Model {
    pub const ALL: [Self; 4] = [
        Self::Dragon32Pal,
        Self::Dragon64Pal,
        Self::CoCo1Ntsc,
        Self::CoCo2Ntsc,
    ];
    pub const VARIANT_IDS: [&'static str; 4] = ["dragon32", "dragon64", "coco1", "coco2"];

    /// Whether this is one of Tandy's Color Computers rather than a Dragon.
    #[must_use]
    pub const fn is_coco(self) -> bool {
        matches!(self, Self::CoCo1Ntsc | Self::CoCo2Ntsc)
    }

    /// The board's television standard.
    #[must_use]
    pub const fn video_standard(self) -> machine_dragon_32::VideoStandard {
        if self.is_coco() {
            machine_dragon_32::VideoStandard::Ntsc
        } else {
            machine_dragon_32::VideoStandard::Pal
        }
    }

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "dragon32",
            Self::Dragon64Pal => "dragon64",
            Self::CoCo1Ntsc => "coco1",
            Self::CoCo2Ntsc => "coco2",
        }
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "dragon32" | "dragon-32" | "dragon-32-pal" => Some(Self::Dragon32Pal),
            "dragon64" | "dragon-64" | "dragon-64-pal" => Some(Self::Dragon64Pal),
            "coco1" | "coco" | "tandy-coco-1-ntsc" => Some(Self::CoCo1Ntsc),
            "coco2" | "tandy-coco-2-ntsc" => Some(Self::CoCo2Ntsc),
            _ => None,
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        use emu198x_shell::FirmwareSource;
        match self {
            Self::Dragon32Pal => vec![
                FirmwareSource::required(self.firmware_id(), &["dragon32.rom"])
                    .with_env_var("EMU198X_DRAGON32_ROM"),
            ],
            Self::Dragon64Pal => vec![
                FirmwareSource::required(self.firmware_id(), &["dragon64-compat.rom"])
                    .with_env_var("EMU198X_DRAGON64_COMPAT_ROM"),
                FirmwareSource::required("dragon64-basic-rom", &["dragon64.rom"])
                    .with_env_var("EMU198X_DRAGON64_ROM"),
            ],
            // XRoar's and MAME's file names, newest revision first.
            Self::CoCo1Ntsc | Self::CoCo2Ntsc => vec![
                FirmwareSource::required(
                    COCO_COLOR_BASIC_ROM_ID,
                    &["bas13.rom", "bas12.rom", "bas11.rom", "bas10.rom"],
                )
                .with_env_var("EMU198X_COCO_BASIC_ROM"),
                FirmwareSource::optional(
                    COCO_EXTENDED_BASIC_ROM_ID,
                    &["extbas11.rom", "extbas10.rom"],
                )
                .with_env_var("EMU198X_COCO_EXTBASIC_ROM"),
            ],
        }
    }

    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        self.video_standard().frame_cycles()
    }

    /// Existing native-window budget, retained independently of the harness's
    /// completed-VDG-frame budget.
    #[must_use]
    pub const fn native_frame_ticks(self) -> u64 {
        894_886 / self.video_standard().frame_hz()
    }

    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "dragon-32-pal",
            Self::Dragon64Pal => "dragon-64-pal",
            Self::CoCo1Ntsc => "tandy-coco-1-ntsc",
            Self::CoCo2Ntsc => "tandy-coco-2-ntsc",
        }
    }

    /// User-facing display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "Dragon 32 (PAL)",
            Self::Dragon64Pal => "Dragon 64 (PAL)",
            Self::CoCo1Ntsc => "Tandy Color Computer (NTSC)",
            Self::CoCo2Ntsc => "Tandy Color Computer 2 (NTSC)",
        }
    }

    /// Firmware image identifier used by the shared shell.
    #[must_use]
    pub const fn firmware_id(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "dragon32-basic-rom",
            Self::Dragon64Pal => "dragon64-compatible-rom",
            Self::CoCo1Ntsc | Self::CoCo2Ntsc => COCO_COLOR_BASIC_ROM_ID,
        }
    }

    const fn firmware_label(self) -> &'static str {
        match self {
            Self::Dragon32Pal => "Dragon 32 BASIC ROM",
            Self::Dragon64Pal => "Dragon 64 BASIC ROM",
            Self::CoCo1Ntsc | Self::CoCo2Ntsc => "Color BASIC ROM (8 KB)",
        }
    }
}

/// Color BASIC, the CoCo's 8 KB ROM at `$A000-$BFFF`.
pub const COCO_COLOR_BASIC_ROM_ID: &str = "coco-color-basic-rom";
/// Extended Color BASIC, the optional 8 KB ROM at `$8000-$9FFF`.
pub const COCO_EXTENDED_BASIC_ROM_ID: &str = "coco-extended-basic-rom";

/// A CoCo profile: tape and cartridge slots only.
///
/// The Disk BASIC controller (FD-500/502: a control latch at `$FF40`, a
/// WD1793 at `$FF48` and HALT-driven transfers) is not the DragonDOS
/// controller this runtime models, and PC-Dragon snapshots and DragonDOS
/// binaries are Dragon formats, so those slots are not offered.
fn coco_profile(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("tandy-coco"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Dragon,
        region: Region::Ntsc,
        release_year: match model {
            Model::CoCo2Ntsc => 1983,
            _ => 1980,
        },
        summary: match model {
            Model::CoCo2Ntsc => "Tandy Color Computer 2 (NTSC) — the Dragon's MC6809E, SAM, MC6847 and two PIAs on Tandy's 64 KiB board: Color BASIC with Extended Color BASIC if fitted, the CoCo keyboard wiring, the 262-line NTSC frame, cassette and ROM cartridges.",
            _ => "Tandy Color Computer (NTSC) — the Dragon's chips on Tandy's original board with 16 or 32 KiB: Color BASIC with Extended Color BASIC if fitted, the CoCo keyboard wiring, the 262-line NTSC frame, cassette and ROM cartridges.",
        }
        .into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(894_886)),
        firmware: vec![
            FirmwareRequirement::new(COCO_COLOR_BASIC_ROM_ID, model.firmware_label(), false),
            FirmwareRequirement::new(
                COCO_EXTENDED_BASIC_ROM_ID,
                "Extended Color BASIC ROM (8 KB) — optional",
                true,
            ),
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
                "Program Pak",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
        ],
        capabilities: CapabilitySet::with_all([
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("variant-switch"),
            known_capability("cassette-media"),
            known_capability("cartridge-media"),
            known_capability("joystick-input"),
            known_capability("keyboard-matrix"),
            known_capability("scripted-input"),
            known_capability("video-framebuffer"),
        ]),
    }
}

/// Returns the Dragon family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

/// Returns one Dragon profile.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::CoCo1Ntsc | Model::CoCo2Ntsc => coco_profile(model),
        Model::Dragon32Pal => MachineProfile {
            machine_id: MachineId::from("dragon"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Dragon,
            region: Region::Pal,
            release_year: 1982,
            summary: "Dragon 32 PAL bring-up runtime. It boots the real BASIC ROM through the shared MC6809/PIA/SAM/VDG machine substrate, mounts CAS tapes, VDK disks, ROM/DGN cartridges, and PC-Dragon PAK snapshots, emits the current MC6847 text, semigraphics, or graphics framebuffer, produces mono audio from the PIA DAC/mux path, and exposes Dragon analogue joystick hardware.".into(),
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
                    "drive-1",
                    "DragonDOS drive 1",
                    MediaKind::Disk,
                    false,
                    WritebackPolicy::SidecarOnly,
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
                known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
                known_capability("variant-switch"),
                known_capability("cassette-media"),
                known_capability("cartridge-media"),
                known_capability("disk-media"),
                known_capability("disk-writeback"),
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
            release_year: 1983,
            summary: "Dragon 64 PAL runtime. It cold-boots in Dragon 32-compatible mode from the compatible BASIC ROM, switches to the high BASIC ROM for EXEC 48000 64-mode entry, adds the Dragon 64 ACIA decode and SAM-backed 64K RAM paging, and keeps the same cassette, VDK disk, cartridge, snapshot, program, keyboard, joystick, framebuffer, and mono audio surfaces as the Dragon 32 profile.".into(),
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
                    "drive-1",
                    "DragonDOS drive 1",
                    MediaKind::Disk,
                    false,
                    WritebackPolicy::SidecarOnly,
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
                known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
                known_capability("variant-switch"),
                known_capability("cassette-media"),
                known_capability("cartridge-media"),
                known_capability("disk-media"),
                known_capability("disk-writeback"),
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
        assert_eq!(profile.media_slots.len(), 5);
        assert_eq!(profile.media_slots[0].id.as_ref(), "tape-1");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Tape);
        assert_eq!(profile.media_slots[1].id.as_ref(), "cartridge-1");
        assert_eq!(profile.media_slots[1].kind, MediaKind::Cartridge);
        assert_eq!(profile.media_slots[2].id.as_ref(), "drive-1");
        assert_eq!(profile.media_slots[2].kind, MediaKind::Disk);
        assert_eq!(profile.media_slots[3].id.as_ref(), "snapshot-1");
        assert_eq!(profile.media_slots[3].kind, MediaKind::Snapshot);
        assert_eq!(profile.media_slots[4].id.as_ref(), "program-1");
        assert_eq!(profile.media_slots[4].kind, MediaKind::Program);

        let profile = profile_for(Model::Dragon64Pal);
        assert_eq!(profile.family, Family::Dragon);
        assert_eq!(profile.region, Region::Pal);
        assert_eq!(profile.firmware.len(), 2);
        assert_eq!(profile.firmware[0].id.as_ref(), "dragon64-compatible-rom");
        assert!(!profile.firmware[0].optional);
        assert_eq!(profile.firmware[1].id.as_ref(), "dragon64-basic-rom");
        assert!(!profile.firmware[1].optional);
        assert_eq!(profile.media_slots.len(), 5);
    }

    #[test]
    fn coco_profiles_are_ntsc_with_color_basic_and_optional_extended_basic() {
        for model in [Model::CoCo1Ntsc, Model::CoCo2Ntsc] {
            let profile = profile_for(model);
            assert_eq!(profile.machine_id.as_str(), "tandy-coco");
            assert_eq!(profile.family, Family::Dragon);
            assert_eq!(profile.region, Region::Ntsc);
            assert_eq!(profile.firmware.len(), 2);
            assert_eq!(profile.firmware[0].id.as_ref(), COCO_COLOR_BASIC_ROM_ID);
            assert!(!profile.firmware[0].optional);
            assert_eq!(profile.firmware[1].id.as_ref(), COCO_EXTENDED_BASIC_ROM_ID);
            assert!(profile.firmware[1].optional);
            assert_eq!(profile.media_slots.len(), 2);
            assert_eq!(model.frame_ticks(), machine_dragon_32::NTSC_FRAME_CYCLES);
            assert_eq!(model.native_frame_ticks(), 894_886 / 60);
            assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
            assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        }
        assert_eq!(Model::Dragon32Pal.native_frame_ticks(), 894_886 / 50);
    }
}
