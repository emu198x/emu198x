//! Commodore Amiga family metadata and runtime surface.

mod runtime;

use commodore_agnus_ocs::{PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME};
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

pub use runtime::{AmigaRuntime, AmigaSessionQueryProvider, DISPLAY_HEIGHT, DISPLAY_WIDTH};

/// Supported Amiga models in the fresh workspace bootstrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Commodore Amiga 500 OCS PAL baseline.
    A500OcsPal,
}

/// Native PAL frame length in Agnus colour clocks.
pub const A500_PAL_FRAME_TICKS: u64 = PAL_CCKS_PER_LINE as u64 * PAL_LINES_PER_FRAME as u64;

/// PAL Agnus colour-clock rate in Hz.
pub const A500_PAL_CCK_HZ: u64 = 28_375_160 / 8;

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A500OcsPal => "commodore-amiga-a500-ocs-pal",
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
            Self::A500OcsPal => "Commodore Amiga 500 (OCS PAL)",
        }
    }
}

/// Returns the initial Amiga family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::A500OcsPal)]
}

/// Returns the profile metadata for one Amiga model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::A500OcsPal => MachineProfile {
            machine_id: MachineId::from("commodore-amiga"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Amiga,
            region: Region::Pal,
            support_tier: SupportTier::Boots,
            release_year: 1987,
            summary: "A500 OCS PAL baseline with Kickstart-backed headless boot, cropped RGBA framebuffer output, stereo Paula audio capture, DF0 ADF insertion, and shared keyboard input. Native verifier UI, snapshots, and broader software validation are still pending.".into(),
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
