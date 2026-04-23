//! Spectrum-family machine catalogue.
//!
//! The `Model` enum enumerates every variant the crate covers; `profile_for`
//! builds the `MachineProfile` descriptor for one; `profiles()` returns the
//! full catalogue. The actual runtime behaviour lives in `spectrum_runtime`,
//! `spectrum_48k`, and `variants` — this module owns only metadata.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

/// Supported Spectrum family models.
///
/// Every variant in this enum has both a working machine crate and a
/// `MachineCore` runtime wrapper. The 48K runtime (`Spectrum48kRuntime`)
/// is bespoke — it carries the rich session query provider with ROM
/// glyph decoding and boot detection. The 128K / +2 / +2A / +2B / +3 /
/// Pentagon / Scorpion / Timex runtimes are generic
/// `SpectrumRuntime<M>` instantiations exposed as type aliases
/// (e.g. `Spectrum128kRuntime`, `Pentagon128Runtime`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// ZX Spectrum 48K PAL.
    Spectrum48KPal,
    /// ZX Spectrum 128K PAL ("toastrack").
    Spectrum128KPal,
    /// ZX Spectrum +2 (Sinclair-branded, Amstrad-built, 128K-compatible).
    SpectrumPlus2,
    /// ZX Spectrum +2A (Amstrad gate array, 4 ROMs, no disk).
    SpectrumPlus2A,
    /// ZX Spectrum +2B (Amstrad gate array, ROM revision, no disk).
    SpectrumPlus2B,
    /// ZX Spectrum +3 (Amstrad gate array, built-in 3" disk drive).
    SpectrumPlus3,
    /// Pentagon 128 (Russian Spectrum clone, no contention).
    Pentagon128,
    /// Scorpion ZS-256 (Russian extended Spectrum, 256K RAM, no contention).
    ScorpionZS256,
    /// Timex TC2048 (Portuguese 48K-compatible with SCLD video modes).
    TimexTC2048,
    /// Timex TC2068 (PAL Timex with DOCK/EXROM paging + AY).
    TimexTC2068,
    /// Timex TS2068 (NTSC US Timex, 14.112 MHz crystal).
    TimexTS2068,
}

impl Model {
    /// Stable model identifier for this model.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k",
            Self::SpectrumPlus2 => "sinclair-zx-spectrum-plus2",
            Self::SpectrumPlus2A => "sinclair-zx-spectrum-plus2a",
            Self::SpectrumPlus2B => "sinclair-zx-spectrum-plus2b",
            Self::SpectrumPlus3 => "sinclair-zx-spectrum-plus3",
            Self::Pentagon128 => "pentagon-128",
            Self::ScorpionZS256 => "scorpion-zs256",
            Self::TimexTC2048 => "timex-tc2048",
            Self::TimexTC2068 => "timex-tc2068",
            Self::TimexTS2068 => "timex-ts2068",
        }
    }

    /// Stable profile identifier for this model.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k-pal",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k-pal",
            Self::SpectrumPlus2 => "sinclair-zx-spectrum-plus2-pal",
            Self::SpectrumPlus2A => "sinclair-zx-spectrum-plus2a-pal",
            Self::SpectrumPlus2B => "sinclair-zx-spectrum-plus2b-pal",
            Self::SpectrumPlus3 => "sinclair-zx-spectrum-plus3-pal",
            Self::Pentagon128 => "pentagon-128-pal",
            Self::ScorpionZS256 => "scorpion-zs256-pal",
            Self::TimexTC2048 => "timex-tc2048-pal",
            Self::TimexTC2068 => "timex-tc2068-pal",
            Self::TimexTS2068 => "timex-ts2068-ntsc",
        }
    }

    /// User-facing display name for this model.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "ZX Spectrum 48K (PAL)",
            Self::Spectrum128KPal => "ZX Spectrum 128K (PAL)",
            Self::SpectrumPlus2 => "ZX Spectrum +2 (PAL)",
            Self::SpectrumPlus2A => "ZX Spectrum +2A (PAL)",
            Self::SpectrumPlus2B => "ZX Spectrum +2B (PAL)",
            Self::SpectrumPlus3 => "ZX Spectrum +3 (PAL)",
            Self::Pentagon128 => "Pentagon 128",
            Self::ScorpionZS256 => "Scorpion ZS-256",
            Self::TimexTC2048 => "Timex TC2048",
            Self::TimexTC2068 => "Timex TC2068",
            Self::TimexTS2068 => "Timex TS2068 (NTSC)",
        }
    }

    /// Year of original release (for catalogue display).
    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::Spectrum48KPal => 1982,
            Self::Spectrum128KPal | Self::TimexTC2048 => 1985,
            Self::SpectrumPlus2 | Self::SpectrumPlus2A | Self::SpectrumPlus3 => 1986,
            Self::SpectrumPlus2B => 1988,
            Self::Pentagon128 => 1989,
            Self::ScorpionZS256 => 1991,
            Self::TimexTC2068 | Self::TimexTS2068 => 1983,
        }
    }
}

/// Returns the full Spectrum family catalogue. All 11 entries have a
/// working machine crate; only `Spectrum48KPal` currently has a
/// MachineCore runtime wrapper.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Spectrum48KPal),
        profile_for(Model::Spectrum128KPal),
        profile_for(Model::SpectrumPlus2),
        profile_for(Model::SpectrumPlus2A),
        profile_for(Model::SpectrumPlus2B),
        profile_for(Model::SpectrumPlus3),
        profile_for(Model::Pentagon128),
        profile_for(Model::ScorpionZS256),
        profile_for(Model::TimexTC2048),
        profile_for(Model::TimexTC2068),
        profile_for(Model::TimexTS2068),
    ]
}

/// Tape-only single-deck media slot — every Spectrum variant ships
/// with this. The +3 also gets a separate Disk slot inside its arm.
fn tape_slot() -> MediaSlot {
    MediaSlot::new(
        "tape-1",
        "Tape Deck",
        MediaKind::Tape,
        false,
        WritebackPolicy::InMemoryOnly,
    )
}

/// Capability bundle for the AY-PSG-and-banked-memory Sinclair models
/// (128K, +2). The +2A/+3 use this as a baseline and add disk-related
/// capabilities on top.
fn ay_capabilities() -> CapabilitySet {
    CapabilitySet::with_all([
        known_capability("ay-audio"),
        known_capability("banked-memory"),
        known_capability("keyboard-matrix"),
        known_capability("tape-input"),
        known_capability("tape-transport-control"),
        known_capability("snapshot-import"),
        known_capability("scripted-input"),
    ])
}

/// Returns the profile metadata for one Spectrum model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::Spectrum48KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: 1982,
            summary: "48K PAL baseline for the first reference Spectrum implementation.".into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![FirmwareRequirement::new(
                "sinclair-zx-spectrum-48k-rom",
                "ZX Spectrum 48K ROM",
                false,
            )],
            media_slots: vec![MediaSlot::new(
                "tape-1",
                "Tape Deck",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            capabilities: CapabilitySet::with_all([
                known_capability("beeper-audio"),
                known_capability("keyboard-matrix"),
                known_capability("snapshot-export"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
            ]),
        },
        Model::Spectrum128KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary:
                "128K PAL follow-on profile with banked memory, AY audio, and tape-era baseline media."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: vec![
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
            media_slots: vec![tape_slot()],
            capabilities: ay_capabilities(),
        },
        Model::SpectrumPlus2 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary:
                "Sinclair-branded Amstrad-built 128K-compatible. Same chip set as the 128K plus a built-in tape deck."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: vec![
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-plus2-rom-0",
                    "ZX Spectrum +2 ROM 0",
                    false,
                ),
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-plus2-rom-1",
                    "ZX Spectrum +2 ROM 1",
                    false,
                ),
            ],
            media_slots: vec![tape_slot()],
            capabilities: ay_capabilities(),
        },
        Model::SpectrumPlus2A | Model::SpectrumPlus2B => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary:
                "Amstrad-built +2A / +2B with the 40077 gate array, 4 ROMs, and extended `$1FFD` paging. No floppy drive."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: (0..4)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("sinclair-zx-spectrum-plus3-rom-{i}"),
                        format!("ZX Spectrum +2A/+3 ROM {i}"),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![tape_slot()],
            capabilities: ay_capabilities(),
        },
        Model::SpectrumPlus3 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary:
                "Amstrad-built +3 with the 40077 gate array, 4 ROMs, extended `$1FFD` paging, and a built-in 3\" floppy drive driven by an NEC µPD765A."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: (0..4)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("sinclair-zx-spectrum-plus3-rom-{i}"),
                        format!("ZX Spectrum +3 ROM {i}"),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![
                tape_slot(),
                MediaSlot::new(
                    "disk-a",
                    "Floppy Drive A:",
                    MediaKind::Disk,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
            ],
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("disk-input"));
                caps
            },
        },
        Model::Pentagon128 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary: "Russian Spectrum clone with no contention, AY, and Beta 128 disk interface."
                .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_336_000)),
            firmware: vec![
                FirmwareRequirement::new("pentagon-rom-0", "Pentagon ROM 0", false),
                FirmwareRequirement::new("pentagon-rom-1", "Pentagon ROM 1", false),
            ],
            media_slots: vec![tape_slot()],
            capabilities: ay_capabilities(),
        },
        Model::ScorpionZS256 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary:
                "Russian extended Spectrum: 256 KB RAM in 16 banks, 4 ROMs, AY, no contention, Beta disk."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: (0..4)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("scorpion-rom-{i}"),
                        format!("Scorpion ROM {i}"),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![tape_slot()],
            capabilities: ay_capabilities(),
        },
        Model::TimexTC2048 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary:
                "Portuguese 48K-compatible with the SCLD chip — 8 video modes, full I/O decoding, no AY."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![FirmwareRequirement::new(
                "timex-tc2048-rom",
                "Timex TC2048 ROM",
                false,
            )],
            media_slots: vec![tape_slot()],
            capabilities: CapabilitySet::with_all([
                known_capability("beeper-audio"),
                known_capability("keyboard-matrix"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
            ]),
        },
        Model::TimexTC2068 | Model::TimexTS2068 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: if matches!(model, Model::TimexTS2068) {
                Region::Ntsc
            } else {
                Region::Pal
            },
            support_tier: SupportTier::Research,
            release_year: model.release_year(),
            summary:
                "Timex TC2068 (PAL) / TS2068 (NTSC): SCLD video, DOCK/EXROM paging via `$F4`, AY on `$F5`/`$F6`."
                    .into(),
            clock: ClockDesc::new(
                "master-cycle",
                ClockRate::from_hz(if matches!(model, Model::TimexTS2068) {
                    14_112_000
                } else {
                    14_000_000
                }),
            ),
            firmware: (0..2)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("timex-ts2068-rom-{i}"),
                        format!("Timex TS2068 ROM {i}"),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![tape_slot()],
            capabilities: ay_capabilities(),
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
