//! Commodore 64 family metadata.
//!
//! This crate currently owns the C64 family profile catalogue for the fresh
//! workspace and the first firmware-backed C64 runtime surface.

mod runtime;

use common_commodore_c64::timing::{TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

pub use runtime::{C64Runtime, C64SessionQueryProvider};

/// Supported C64 models in the fresh workspace bootstrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Commodore 64 PAL breadbin.
    C64PalBreadbin,
    /// Commodore 64 NTSC breadbin.
    C64NtscBreadbin,
}

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::C64PalBreadbin => "commodore-c64-pal-breadbin",
            Self::C64NtscBreadbin => "commodore-c64-ntsc-breadbin",
        }
    }

    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    /// User-facing display name for this profile.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::C64PalBreadbin => "Commodore 64 (PAL Breadbin)",
            Self::C64NtscBreadbin => "Commodore 64 (NTSC Breadbin)",
        }
    }
}

/// Returns the initial C64 family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::C64PalBreadbin),
        profile_for(Model::C64NtscBreadbin),
    ]
}

/// Returns the profile metadata for one C64 model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let (region, support_tier, summary, clock_rate, release_year) = match model {
        Model::C64PalBreadbin => (
            Region::Pal,
            SupportTier::Boots,
            "PAL breadbin baseline now boots real BASIC/KERNAL/CHARGEN ROMs to the BASIC READY. prompt in the fresh workspace. Live 6502, CIA, and VIC-II are wired; SID remains shadowed and media plus snapshots are still pending.",
            ClockRate::from_hz(TIMING_PAL_BREADBIN.cpu_hz),
            1982,
        ),
        Model::C64NtscBreadbin => (
            Region::Ntsc,
            SupportTier::Research,
            "NTSC breadbin follow-on profile on the same live 6502/CIA/VIC-II substrate. Fresh-workspace frame execution exists, but NTSC boot validation and media plus snapshot support are still pending.",
            ClockRate::from_hz(TIMING_NTSC_BREADBIN.cpu_hz),
            1982,
        ),
    };

    MachineProfile {
        machine_id: MachineId::from("commodore-c64"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::C64,
        region,
        support_tier,
        release_year,
        summary: summary.into(),
        clock: ClockDesc::new("phi2-cycle", clock_rate),
        firmware: vec![
            FirmwareRequirement::new("commodore-c64-basic-rom", "C64 BASIC ROM", false),
            FirmwareRequirement::new("commodore-c64-kernal-rom", "C64 KERNAL ROM", false),
            FirmwareRequirement::new(
                "commodore-c64-character-rom",
                "C64 Character Generator ROM",
                false,
            ),
        ],
        media_slots: vec![
            MediaSlot::new(
                "tape-1",
                "Datasette",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
            MediaSlot::new(
                "drive-8",
                "Disk Drive 8",
                MediaKind::Disk,
                false,
                WritebackPolicy::SidecarOnly,
            ),
            MediaSlot::new(
                "cartridge-1",
                "Cartridge Port",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
        ],
        capabilities: CapabilitySet::with_all([
            known_capability("keyboard-matrix"),
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
    fn pal_profile_uses_phi2_clock() {
        let profile = profile_for(Model::C64PalBreadbin);
        assert_eq!(profile.clock.unit.as_ref(), "phi2-cycle");
        assert_eq!(profile.clock.rate.numerator_hz, TIMING_PAL_BREADBIN.cpu_hz);
        assert_eq!(profile.clock.rate.denominator_hz, 1);
        assert_eq!(profile.region, Region::Pal);
    }

    #[test]
    fn ntsc_profile_uses_phi2_clock() {
        let profile = profile_for(Model::C64NtscBreadbin);
        assert_eq!(profile.clock.unit.as_ref(), "phi2-cycle");
        assert_eq!(profile.clock.rate.numerator_hz, TIMING_NTSC_BREADBIN.cpu_hz);
        assert_eq!(profile.clock.rate.denominator_hz, 1);
        assert_eq!(profile.region, Region::Ntsc);
    }

    #[test]
    fn both_profiles_require_all_three_roms() {
        for profile in profiles() {
            let ids: Vec<&str> = profile.firmware.iter().map(|rom| rom.id.as_ref()).collect();
            assert_eq!(
                ids,
                vec![
                    "commodore-c64-basic-rom",
                    "commodore-c64-kernal-rom",
                    "commodore-c64-character-rom",
                ]
            );
        }
    }

    #[test]
    fn media_slots_match_bootstrap_scope() {
        let profile = profile_for(Model::C64PalBreadbin);
        let ids: Vec<&str> = profile
            .media_slots
            .iter()
            .map(|slot| slot.id.as_ref())
            .collect();
        assert_eq!(ids, vec!["tape-1", "drive-8", "cartridge-1"]);
        assert_eq!(profile.media_slots[0].kind, MediaKind::Tape);
        assert_eq!(profile.media_slots[1].kind, MediaKind::Disk);
        assert_eq!(profile.media_slots[2].kind, MediaKind::Cartridge);
        assert_eq!(
            profile.media_slots[1].writeback,
            WritebackPolicy::SidecarOnly
        );
    }

    #[test]
    fn profiles_stay_honest_about_current_support_tier() {
        assert_eq!(
            profile_for(Model::C64PalBreadbin).support_tier,
            SupportTier::Boots
        );
        assert_eq!(
            profile_for(Model::C64NtscBreadbin).support_tier,
            SupportTier::Research
        );
    }
}
