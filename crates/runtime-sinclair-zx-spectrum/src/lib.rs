//! Sinclair ZX Spectrum family metadata.
//!
//! This crate intentionally begins with profile and model metadata only. The
//! timing model, CPU, ULA, media path, and concrete machine implementations
//! will land in later passes against the fresh architecture.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

/// Supported Spectrum family models in the initial bootstrap pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// ZX Spectrum 48K PAL.
    Spectrum48KPal,
    /// ZX Spectrum 128K PAL.
    Spectrum128KPal,
}

impl Model {
    /// Stable model identifier for this model.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k",
        }
    }

    /// Stable profile identifier for this model.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k-pal",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k-pal",
        }
    }

    /// User-facing display name for this model.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "ZX Spectrum 48K (PAL)",
            Self::Spectrum128KPal => "ZX Spectrum 128K (PAL)",
        }
    }
}

/// Returns the initial Spectrum family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Spectrum48KPal),
        profile_for(Model::Spectrum128KPal),
    ]
}

/// Returns the profile metadata for one Spectrum model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::Spectrum48KPal => MachineProfile::new(
            MachineId::from("sinclair-zx-spectrum"),
            ProfileId::from(model.profile_id()),
            model.display_name(),
            Family::Spectrum,
            Region::Pal,
            SupportTier::Research,
            1982,
            "48K PAL baseline for the first reference Spectrum implementation.",
            ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            vec![FirmwareRequirement::new(
                "sinclair-zx-spectrum-48k-rom",
                "ZX Spectrum 48K ROM",
                false,
            )],
            vec![MediaSlot::new(
                "tape-1",
                "Tape Deck",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            CapabilitySet::with_all([
                known_capability("keyboard-matrix"),
                known_capability("tape-input"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
            ]),
        ),
        Model::Spectrum128KPal => MachineProfile::new(
            MachineId::from("sinclair-zx-spectrum"),
            ProfileId::from(model.profile_id()),
            model.display_name(),
            Family::Spectrum,
            Region::Pal,
            SupportTier::Research,
            1985,
            "128K PAL follow-on profile with banked memory, AY audio, and tape-era baseline media.",
            ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            vec![
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-128k-rom-0",
                    "ZX Spectrum 128K ROM 0",
                    false,
                ),
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-128k-rom-1",
                    "ZX Spectrum 128K ROM 1",
                    false,
                ),
            ],
            vec![MediaSlot::new(
                "tape-1",
                "Tape Deck",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            CapabilitySet::with_all([
                known_capability("ay-audio"),
                known_capability("banked-memory"),
                known_capability("keyboard-matrix"),
                known_capability("tape-input"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
            ]),
        ),
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
    fn spectrum_48k_uses_documented_master_clock() {
        let profile = profile_for(Model::Spectrum48KPal);
        assert_eq!(profile.clock.unit.as_ref(), "master-cycle");
        assert_eq!(profile.clock.rate.numerator_hz, 14_000_000);
        assert_eq!(profile.clock.rate.denominator_hz, 1);
    }

    #[test]
    fn all_profiles_require_firmware() {
        for profile in profiles() {
            assert!(
                !profile.firmware.is_empty(),
                "{} should declare firmware",
                profile.display_name
            );
        }
    }
}
