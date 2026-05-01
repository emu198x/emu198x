//! Commodore Amiga family profile catalogue.

use commodore_agnus_ocs::{NTSC_CCKS_PER_FRAME, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME};
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};
use machine_commodore_amiga_ocs::RamConfig;

/// Supported Amiga models in the fresh workspace bootstrap.
///
/// Each variant bundles a named RAM layout and a video region. The
/// PAL group covers the common A500 configurations users actually
/// shipped in Europe (50 Hz, 312-line frames). The NTSC group covers
/// the same configurations as sold in North America (60 Hz, 262-line
/// frames with the short/long line alternation modelled in the chip
/// layer per HRM p. 785). Custom layouts outside these presets are
/// available through `AmigaRuntime::from_ram_config`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Model {
    /// A1000 OCS PAL (shipping config, 1985): 256 KiB chip RAM,
    /// 64 KiB bootstrap ROM, and writable WOM for Kickstart loaded
    /// from floppy.
    A1000OcsPal,
    /// A1000 OCS NTSC (US shipping config, 1985): same RAM/bootstrap
    /// layout as the PAL A1000; differs only in Agnus region.
    A1000OcsNtsc,
    /// Stock A500 OCS PAL: 512 KiB chip RAM only.
    A500OcsPal,
    /// Stock A500 OCS NTSC.
    A500OcsNtsc,
    /// A500 + A501 trapdoor: 512 KiB chip + 512 KiB slow RAM (PAL).
    A500OcsPalA501,
    /// A500 + A501 trapdoor (NTSC).
    A500OcsNtscA501,
    /// A500+ (Plus) layout: 1 MiB chip RAM, no trapdoor (PAL).
    /// **ECS chipset** — A500+ shipped with ECS Agnus 8375 and ECS
    /// (Super) Denise 8373, plus Kickstart 2.04. Backed by
    /// `AmigaEcs` not `AmigaOcs`.
    A500PlusEcsPal,
    /// A500+ (Plus) layout (NTSC). ECS chipset; Kickstart 2.04.
    A500PlusEcsNtsc,
    /// Maxed A500: 1 MiB chip + 512 KiB slow + 8 MiB Zorro-II fast (PAL).
    A500OcsPalMaxed,
    /// Maxed A500 (NTSC).
    A500OcsNtscMaxed,
}

/// Native PAL frame length in Agnus colour clocks.
pub const A500_PAL_FRAME_CCKS: u64 = PAL_CCKS_PER_LINE as u64 * PAL_LINES_PER_FRAME as u64;

/// Native PAL frame length in machine ticks (master/4 = 2 per CCK).
pub const A500_PAL_FRAME_TICKS: u64 = A500_PAL_FRAME_CCKS * 2;

/// PAL Agnus colour-clock rate in Hz. PAL master clock = 28.37516
/// MHz; CCK = master / 8.
pub const A500_PAL_CCK_HZ: u64 = 28_375_160 / 8;

/// Native NTSC frame length in Agnus colour clocks. NTSC alternates
/// short (227 CCK) and long (228 CCK) lines per HRM p. 785, totalling
/// 131 × 227 + 131 × 228 = 59,605 CCKs across the 262-line frame.
pub const A500_NTSC_FRAME_CCKS: u64 = NTSC_CCKS_PER_FRAME as u64;

/// Native NTSC frame length in machine ticks (master/4 = 2 per CCK).
/// 59,605 × 2 = 119,210 ticks.
pub const A500_NTSC_FRAME_TICKS: u64 = A500_NTSC_FRAME_CCKS * 2;

/// NTSC Agnus colour-clock rate in Hz. NTSC master clock = 28.63636
/// MHz (4 × the colour subcarrier 3.579545 MHz); CCK = master / 8.
pub const A500_NTSC_CCK_HZ: u64 = 28_636_360 / 8;

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A1000OcsPal => "commodore-amiga-a1000-ocs-pal",
            Self::A1000OcsNtsc => "commodore-amiga-a1000-ocs-ntsc",
            Self::A500OcsPal => "commodore-amiga-a500-ocs-pal",
            Self::A500OcsNtsc => "commodore-amiga-a500-ocs-ntsc",
            Self::A500OcsPalA501 => "commodore-amiga-a500-ocs-pal-a501",
            Self::A500OcsNtscA501 => "commodore-amiga-a500-ocs-ntsc-a501",
            Self::A500PlusEcsPal => "commodore-amiga-a500-plus-ecs-pal",
            Self::A500PlusEcsNtsc => "commodore-amiga-a500-plus-ecs-ntsc",
            Self::A500OcsPalMaxed => "commodore-amiga-a500-ocs-pal-maxed",
            Self::A500OcsNtscMaxed => "commodore-amiga-a500-ocs-ntsc-maxed",
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
            Self::A1000OcsPal => "Commodore Amiga 1000 (OCS PAL)",
            Self::A1000OcsNtsc => "Commodore Amiga 1000 (OCS NTSC)",
            Self::A500OcsPal => "Commodore Amiga 500 (OCS PAL)",
            Self::A500OcsNtsc => "Commodore Amiga 500 (OCS NTSC)",
            Self::A500OcsPalA501 => "Commodore Amiga 500 + A501 trapdoor (OCS PAL)",
            Self::A500OcsNtscA501 => "Commodore Amiga 500 + A501 trapdoor (OCS NTSC)",
            Self::A500PlusEcsPal => "Commodore Amiga 500+ (ECS PAL)",
            Self::A500PlusEcsNtsc => "Commodore Amiga 500+ (ECS NTSC)",
            Self::A500OcsPalMaxed => "Commodore Amiga 500 maxed (OCS PAL, 1M+512K+8M)",
            Self::A500OcsNtscMaxed => "Commodore Amiga 500 maxed (OCS NTSC, 1M+512K+8M)",
        }
    }

    /// RAM layout for this model. Use `AmigaRuntime::from_ram_config`
    /// if you need a layout outside the preset set. The PAL/NTSC pairs
    /// share a layout — only Agnus differs.
    #[must_use]
    pub const fn ram_config(self) -> RamConfig {
        match self {
            Self::A1000OcsPal | Self::A1000OcsNtsc => RamConfig {
                chip_kb: 256,
                slow_kb: 0,
                fast_kb: 0,
            },
            Self::A500OcsPal | Self::A500OcsNtsc => RamConfig::bare(),
            Self::A500OcsPalA501 | Self::A500OcsNtscA501 => RamConfig::a501_trapdoor(),
            Self::A500PlusEcsPal | Self::A500PlusEcsNtsc => RamConfig::a500_plus(),
            Self::A500OcsPalMaxed | Self::A500OcsNtscMaxed => RamConfig::a500_maxed(),
        }
    }

    /// Whether this model uses the A1000 bootstrap-ROM boot path
    /// (64 KiB bootstrap into WOM) or the standard Kickstart boot
    /// path (256 / 512 KiB Kickstart). True for both A1000 PAL and
    /// A1000 NTSC.
    #[must_use]
    pub const fn is_a1000(self) -> bool {
        matches!(self, Self::A1000OcsPal | Self::A1000OcsNtsc)
    }

    /// Whether this model is NTSC. Drives the Agnus region selection
    /// and the runtime frame-tick / CCK-rate constants.
    #[must_use]
    pub const fn is_ntsc(self) -> bool {
        matches!(
            self,
            Self::A1000OcsNtsc
                | Self::A500OcsNtsc
                | Self::A500OcsNtscA501
                | Self::A500PlusEcsNtsc
                | Self::A500OcsNtscMaxed
        )
    }
}

/// Returns the initial Amiga family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::A1000OcsPal),
        profile_for(Model::A1000OcsNtsc),
        profile_for(Model::A500OcsPal),
        profile_for(Model::A500OcsNtsc),
        profile_for(Model::A500OcsPalA501),
        profile_for(Model::A500OcsNtscA501),
        profile_for(Model::A500PlusEcsPal),
        profile_for(Model::A500PlusEcsNtsc),
        profile_for(Model::A500OcsPalMaxed),
        profile_for(Model::A500OcsNtscMaxed),
    ]
}

/// Returns the profile metadata for one Amiga model.
///
/// PAL and NTSC variants of the same hardware share firmware, media
/// slots, capabilities, and family — they differ in `region`,
/// `clock`, and the descriptive strings. NTSC variants advertise the
/// 3.579545 MHz colour-clock rate; PAL advertises 3.546895 MHz.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let release_year = match model {
        Model::A1000OcsPal | Model::A1000OcsNtsc => 1985,
        Model::A500OcsPal
        | Model::A500OcsNtsc
        | Model::A500OcsPalA501
        | Model::A500OcsNtscA501
        | Model::A500OcsPalMaxed
        | Model::A500OcsNtscMaxed => 1987,
        Model::A500PlusEcsPal | Model::A500PlusEcsNtsc => 1991,
    };
    let (firmware_id, firmware_name) = if model.is_a1000() {
        (
            "commodore-amiga-a1000-bootstrap-rom",
            "Amiga 1000 bootstrap ROM",
        )
    } else {
        ("commodore-amiga-kickstart-rom", "Amiga Kickstart ROM")
    };
    let region = if model.is_ntsc() {
        Region::Ntsc
    } else {
        Region::Pal
    };
    let cck_hz = if model.is_ntsc() {
        A500_NTSC_CCK_HZ
    } else {
        A500_PAL_CCK_HZ
    };
    MachineProfile {
        machine_id: MachineId::from("commodore-amiga"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Amiga,
        region,
        support_tier: SupportTier::Boots,
        release_year,
        summary: match model {
            Model::A1000OcsPal => "Amiga 1000 OCS PAL — bootstrap-ROM cold boot into writable WOM, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. Kickstart-to-Workbench disk swaps are scriptable via headless media reloads.".into(),
            Model::A1000OcsNtsc => "Amiga 1000 OCS NTSC — bootstrap-ROM cold boot into writable WOM, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. Boot path matches PAL A1000; Agnus runs on the NTSC clock with the short/long line alternation modelled in the chip layer.".into(),
            Model::A500OcsPal | Model::A500OcsPalA501 | Model::A500PlusEcsPal | Model::A500OcsPalMaxed => "Amiga OCS PAL — Kickstart-backed headless boot, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. Snapshots and broader software validation still pending.".into(),
            Model::A500OcsNtsc | Model::A500OcsNtscA501 | Model::A500PlusEcsNtsc | Model::A500OcsNtscMaxed => "Amiga OCS NTSC — Kickstart-backed headless boot at the US 60 Hz field rate, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. NTSC boot validation still pending; structural plumbing is in place via the chip-layer short/long line alternation.".into(),
        },
        clock: ClockDesc::new("cck", ClockRate::from_hz(cck_hz)),
        firmware: vec![FirmwareRequirement::new(firmware_id, firmware_name, false)],
        media_slots: vec![MediaSlot::new(
            "floppy-0",
            "DF0:",
            MediaKind::Disk,
            false,
            WritebackPolicy::SidecarOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("keyboard-input"),
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
        let mut ids: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn amiga_profile_declares_kickstart_and_df0() {
        let profile = profile_for(Model::A500OcsPal);
        assert_eq!(profile.family, Family::Amiga);
        assert_eq!(profile.region, Region::Pal);
        assert_eq!(profile.support_tier, SupportTier::Boots);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(
            profile.firmware[0].id.as_ref(),
            "commodore-amiga-kickstart-rom"
        );
        assert_eq!(profile.media_slots.len(), 1);
        assert_eq!(profile.media_slots[0].id.as_ref(), "floppy-0");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Disk);
    }

    #[test]
    fn a1000_profile_declares_bootstrap_rom() {
        let profile = profile_for(Model::A1000OcsPal);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(
            profile.firmware[0].id.as_ref(),
            "commodore-amiga-a1000-bootstrap-rom"
        );
        assert_eq!(profile.media_slots.len(), 1);
        assert_eq!(profile.media_slots[0].id.as_ref(), "floppy-0");
    }
}
