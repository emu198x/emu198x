//! Commodore Amiga runtime — bridges `machine-commodore-amiga-ocs` with
//! the `emu198x-shell` MachineCore trait.
//!
//! The runtime exposes one model for now — A500 OCS PAL — and is
//! deliberately minimal: profile metadata, ADF insertion, the per-
//! frame tick loop that emits ARGB frames from Denise's framebuffer,
//! plus a small set of query paths useful for boot-status diagnostics.
//!
//! Query paths are keyed by dotted string and surface just the state
//! the shell needs to drive its verifier UI. Host snapshots and
//! control commands are reserved for later tasks.

mod runtime;

use commodore_agnus_ocs::{PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME};
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};
pub use machine_commodore_amiga_ocs::RamConfig;

pub use runtime::{AmigaRuntime, AmigaSessionQueryProvider, DISPLAY_HEIGHT, DISPLAY_WIDTH};

/// Supported Amiga models in the fresh workspace bootstrap.
///
/// Each variant bundles a named RAM layout. The variants cover the
/// common A500 configurations users actually shipped: stock, A501
/// trapdoor, the A500+ chip bump, and a maxed-out A500 with both
/// trapdoor and Zorro-II fast RAM. Custom layouts outside these
/// presets are available through `AmigaRuntime::from_ram_config`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// A1000 OCS PAL (shipping config, 1985): 256 KiB chip RAM.
    /// Shares the OCS chipset with the A500; the differences (RAM
    /// size, expansion layout, keyboard/case) don't affect the
    /// chipset tick path, so we reuse `AmigaOcs` as-is.
    A1000OcsPal,
    /// Stock A500 OCS PAL: 512 KiB chip RAM only.
    A500OcsPal,
    /// A500 + A501 trapdoor: 512 KiB chip + 512 KiB slow RAM.
    A500OcsPalA501,
    /// A500+ layout: 1 MiB chip RAM, no trapdoor.
    A500PlusOcsPal,
    /// Maxed A500: 1 MiB chip + 512 KiB slow + 8 MiB Zorro-II fast.
    A500OcsPalMaxed,
}

/// Native PAL frame length in Agnus colour clocks.
pub const A500_PAL_FRAME_CCKS: u64 = PAL_CCKS_PER_LINE as u64 * PAL_LINES_PER_FRAME as u64;

/// Native PAL frame length in machine ticks (master/4 = 2 per CCK).
pub const A500_PAL_FRAME_TICKS: u64 = A500_PAL_FRAME_CCKS * 2;

/// PAL Agnus colour-clock rate in Hz.
pub const A500_PAL_CCK_HZ: u64 = 28_375_160 / 8;

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A1000OcsPal => "commodore-amiga-a1000-ocs-pal",
            Self::A500OcsPal => "commodore-amiga-a500-ocs-pal",
            Self::A500OcsPalA501 => "commodore-amiga-a500-ocs-pal-a501",
            Self::A500PlusOcsPal => "commodore-amiga-a500-plus-ocs-pal",
            Self::A500OcsPalMaxed => "commodore-amiga-a500-ocs-pal-maxed",
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
            Self::A500OcsPal => "Commodore Amiga 500 (OCS PAL)",
            Self::A500OcsPalA501 => "Commodore Amiga 500 + A501 trapdoor (OCS PAL)",
            Self::A500PlusOcsPal => "Commodore Amiga 500+ (OCS PAL)",
            Self::A500OcsPalMaxed => "Commodore Amiga 500 maxed (OCS PAL, 1M+512K+8M)",
        }
    }

    /// RAM layout for this model. Use `AmigaRuntime::from_ram_config`
    /// if you need a layout outside the preset set.
    #[must_use]
    pub const fn ram_config(self) -> RamConfig {
        match self {
            Self::A1000OcsPal => RamConfig {
                chip_kb: 256,
                slow_kb: 0,
                fast_kb: 0,
            },
            Self::A500OcsPal => RamConfig::bare(),
            Self::A500OcsPalA501 => RamConfig::a501_trapdoor(),
            Self::A500PlusOcsPal => RamConfig::a500_plus(),
            Self::A500OcsPalMaxed => RamConfig::a500_maxed(),
        }
    }
}

/// Returns the initial Amiga family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::A1000OcsPal),
        profile_for(Model::A500OcsPal),
        profile_for(Model::A500OcsPalA501),
        profile_for(Model::A500PlusOcsPal),
        profile_for(Model::A500OcsPalMaxed),
    ]
}

/// Returns the profile metadata for one Amiga model.
///
/// All OCS-PAL variants share firmware, media slots, capabilities,
/// and clock rate — only the display name, profile ID, and release
/// year differ. The runtime distinguishes them by their RAM layout
/// via `Model::ram_config`.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let release_year = match model {
        Model::A1000OcsPal => 1985,
        Model::A500OcsPal | Model::A500OcsPalA501 | Model::A500OcsPalMaxed => 1987,
        Model::A500PlusOcsPal => 1991,
    };
    MachineProfile {
        machine_id: MachineId::from("commodore-amiga"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Amiga,
        region: Region::Pal,
        support_tier: SupportTier::Boots,
        release_year,
        summary: "Amiga OCS PAL — Kickstart-backed headless boot, 768x576 ARGB framebuffer, DF0 ADF insertion, keyboard input. Audio, snapshots, and broader software validation still pending.".into(),
        clock: ClockDesc::new("cck", ClockRate::from_hz(A500_PAL_CCK_HZ)),
        firmware: vec![FirmwareRequirement::new(
            "commodore-amiga-kickstart-rom",
            "Amiga Kickstart ROM",
            false,
        )],
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
}
