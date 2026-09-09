//! Spectrum-family machine catalogue.
//!
//! The `Model` enum enumerates every variant the crate covers; `profile_for`
//! builds the `MachineProfile` descriptor for one; `profiles()` returns the
//! full catalogue. The actual runtime behaviour lives in `spectrum_runtime`,
//! `spectrum_48k`, and `variants` — this module owns only metadata.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, FirmwareSource, MachineId,
    MachineProfile, MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
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
    /// ZX Spectrum 16K PAL.
    Spectrum16KPal,
    /// ZX Spectrum 48K PAL.
    Spectrum48KPal,
    /// ZX Spectrum+ PAL (1984, full-stroke keyboard, electrically identical to 48K).
    SpectrumPlus,
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
            Self::Spectrum16KPal => "sinclair-zx-spectrum-16k",
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k",
            Self::SpectrumPlus => "sinclair-zx-spectrum-plus",
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
            Self::Spectrum16KPal => "sinclair-zx-spectrum-16k-pal",
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k-pal",
            Self::SpectrumPlus => "sinclair-zx-spectrum-plus-pal",
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
            Self::Spectrum16KPal => "ZX Spectrum 16K (PAL)",
            Self::Spectrum48KPal => "ZX Spectrum 48K (PAL)",
            Self::SpectrumPlus => "ZX Spectrum+ (PAL)",
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

    /// Every model in catalogue order: the SOLID 8 (16K → 48K → + →
    /// 128K → +2 → +2A → +2B → +3) then the five exotics (Pentagon →
    /// Scorpion → TC2048 → TC2068 → TS2068). Stable order matters for
    /// the window's variant menu.
    pub const ALL: [Self; 13] = [
        Self::Spectrum16KPal,
        Self::Spectrum48KPal,
        Self::SpectrumPlus,
        Self::Spectrum128KPal,
        Self::SpectrumPlus2,
        Self::SpectrumPlus2A,
        Self::SpectrumPlus2B,
        Self::SpectrumPlus3,
        Self::Pentagon128,
        Self::ScorpionZS256,
        Self::TimexTC2048,
        Self::TimexTC2068,
        Self::TimexTS2068,
    ];

    /// Every variant id, in [`Self::ALL`] order.
    pub const VARIANT_IDS: [&'static str; 13] = [
        "spectrum_16k",
        "spectrum_48k",
        "spectrum_plus",
        "spectrum_128k",
        "spectrum_plus2",
        "spectrum_plus2a",
        "spectrum_plus2b",
        "spectrum_plus3",
        "pentagon_128",
        "scorpion_zs256",
        "timex_tc2048",
        "timex_tc2068",
        "timex_ts2068",
    ];

    /// The snake-case id `set_machine` steps, the `--machine` flag and the
    /// window's variant menu use for this model.
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::Spectrum16KPal => "spectrum_16k",
            Self::Spectrum48KPal => "spectrum_48k",
            Self::SpectrumPlus => "spectrum_plus",
            Self::Spectrum128KPal => "spectrum_128k",
            Self::SpectrumPlus2 => "spectrum_plus2",
            Self::SpectrumPlus2A => "spectrum_plus2a",
            Self::SpectrumPlus2B => "spectrum_plus2b",
            Self::SpectrumPlus3 => "spectrum_plus3",
            Self::Pentagon128 => "pentagon_128",
            Self::ScorpionZS256 => "scorpion_zs256",
            Self::TimexTC2048 => "timex_tc2048",
            Self::TimexTC2068 => "timex_tc2068",
            Self::TimexTS2068 => "timex_ts2068",
        }
    }

    /// The model a variant id names.
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    /// The short label the window's Machine menu and title show.
    #[must_use]
    pub const fn menu_label(self) -> &'static str {
        match self {
            Self::Spectrum16KPal => "ZX Spectrum 16K",
            Self::Spectrum48KPal => "ZX Spectrum 48K",
            Self::SpectrumPlus => "ZX Spectrum+",
            Self::Spectrum128KPal => "ZX Spectrum 128",
            Self::SpectrumPlus2 => "ZX Spectrum +2",
            Self::SpectrumPlus2A => "ZX Spectrum +2A",
            Self::SpectrumPlus2B => "ZX Spectrum +2B",
            Self::SpectrumPlus3 => "ZX Spectrum +3",
            Self::Pentagon128 => "Pentagon 128",
            Self::ScorpionZS256 => "Scorpion ZS-256",
            Self::TimexTC2048 => "Timex TC2048",
            Self::TimexTC2068 => "Timex TC2068",
            Self::TimexTS2068 => "Timex TS2068",
        }
    }

    /// The ROM images this model boots and their conventional file names
    /// under `~/.emu198x/roms`.
    ///
    /// 16K/48K/+ share `sinclair-zx-spectrum-48k/48.rom`; 128K, +2, +2A/+3,
    /// and +2B each have their own bundle directory. The +2A and +3 share
    /// `amstrad-zx-spectrum-plus3/plus3-{0..3}.rom` (ROM v4.0); the +2B
    /// uses its own `amstrad-zx-spectrum-plus2b/plus3-{0..3}.rom` (ROM
    /// v4.1). The exotics live under `pentagon-128/`, `scorpion-zs256/`,
    /// `timex-tc2048/`, and `timex-ts2068/`.
    #[must_use]
    pub fn firmware_sources(self) -> Vec<FirmwareSource> {
        match self {
            Self::Spectrum16KPal | Self::Spectrum48KPal | Self::SpectrumPlus => {
                vec![FirmwareSource::required(
                    "sinclair-zx-spectrum-48k-rom",
                    &["sinclair-zx-spectrum-48k/48.rom"],
                )]
            }
            Self::Spectrum128KPal => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-128k-rom-0",
                    &["sinclair-zx-spectrum-128k/128-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-128k-rom-1",
                    &["sinclair-zx-spectrum-128k/128-1.rom"],
                ),
            ],
            Self::SpectrumPlus2 => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus2-rom-0",
                    &["amstrad-zx-spectrum-plus2/plus2-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus2-rom-1",
                    &["amstrad-zx-spectrum-plus2/plus2-1.rom"],
                ),
            ],
            Self::SpectrumPlus2A | Self::SpectrumPlus3 => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-0",
                    &["amstrad-zx-spectrum-plus3/plus3-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-1",
                    &["amstrad-zx-spectrum-plus3/plus3-1.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-2",
                    &["amstrad-zx-spectrum-plus3/plus3-2.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-3",
                    &["amstrad-zx-spectrum-plus3/plus3-3.rom"],
                ),
            ],
            Self::SpectrumPlus2B => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-0",
                    &["amstrad-zx-spectrum-plus2b/plus3-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-1",
                    &["amstrad-zx-spectrum-plus2b/plus3-1.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-2",
                    &["amstrad-zx-spectrum-plus2b/plus3-2.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-3",
                    &["amstrad-zx-spectrum-plus2b/plus3-3.rom"],
                ),
            ],
            Self::Pentagon128 => vec![
                FirmwareSource::required("pentagon-rom-0", &["pentagon-128/pentagon-0.rom"]),
                FirmwareSource::required("pentagon-rom-1", &["pentagon-128/pentagon-1.rom"]),
            ],
            Self::ScorpionZS256 => vec![
                FirmwareSource::required("scorpion-rom-0", &["scorpion-zs256/scorpion-0.rom"]),
                FirmwareSource::required("scorpion-rom-1", &["scorpion-zs256/scorpion-1.rom"]),
                FirmwareSource::required("scorpion-rom-2", &["scorpion-zs256/scorpion-2.rom"]),
                FirmwareSource::required("scorpion-rom-3", &["scorpion-zs256/scorpion-3.rom"]),
            ],
            Self::TimexTC2048 => vec![FirmwareSource::required(
                "timex-tc2048-rom",
                &["timex-tc2048/tc2048.rom"],
            )],
            Self::TimexTC2068 | Self::TimexTS2068 => vec![
                FirmwareSource::required("timex-ts2068-rom-0", &["timex-ts2068/ts2068.rom"]),
                FirmwareSource::required("timex-ts2068-rom-1", &["timex-ts2068/exrom.rom"]),
            ],
        }
    }

    /// Year of original release (for catalogue display).
    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::Spectrum16KPal | Self::Spectrum48KPal => 1982,
            Self::SpectrumPlus => 1984,
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
        profile_for(Model::Spectrum16KPal),
        profile_for(Model::Spectrum48KPal),
        profile_for(Model::SpectrumPlus),
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
        known_capability("memory-watch"),
        known_capability("port-io"),
        known_capability("basic-program-load"),
        known_capability("tape-autoload"),
        known_capability("variant-switch"),
    ])
}

/// Returns the profile metadata for one Spectrum model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::Spectrum16KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: 1982,
            summary:
                "16K PAL — the half-RAM 1982 Spectrum. Same Ferranti ULA, same 48K ROM, upper 32 KiB electrically disconnected."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![FirmwareRequirement::new(
                "sinclair-zx-spectrum-48k-rom",
                "ZX Spectrum 48K ROM",
                false,
            )],
            media_slots: vec![tape_slot()],
            capabilities: CapabilitySet::with_all([
                known_capability("cycle-profile"),
                known_capability("beeper-audio"),
                known_capability("keyboard-matrix"),
                known_capability("snapshot-export"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
                known_capability("memory-watch"),
                known_capability("port-io"),
                known_capability("basic-program-load"),
                known_capability("tape-autoload"),
                known_capability("variant-switch"),
            ]),
        },
        Model::Spectrum48KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
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
                known_capability("cycle-profile"),
                known_capability("beeper-audio"),
                known_capability("keyboard-matrix"),
                known_capability("snapshot-export"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
                known_capability("memory-watch"),
                known_capability("port-io"),
                known_capability("basic-program-load"),
                known_capability("tape-autoload"),
                known_capability("variant-switch"),
            ]),
        },
        Model::SpectrumPlus => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: 1984,
            summary:
                "1984 ZX Spectrum+ — full-stroke keyboard with reset button. Electrically identical to the 48K (same Ferranti ULA, same 48K ROM, same RAM), distinct catalogue identity."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![FirmwareRequirement::new(
                "sinclair-zx-spectrum-48k-rom",
                "ZX Spectrum 48K ROM",
                false,
            )],
            media_slots: vec![tape_slot()],
            capabilities: CapabilitySet::with_all([
                known_capability("cycle-profile"),
                known_capability("beeper-audio"),
                known_capability("keyboard-matrix"),
                known_capability("snapshot-export"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
                known_capability("memory-watch"),
                known_capability("port-io"),
                known_capability("basic-program-load"),
                known_capability("tape-autoload"),
                known_capability("variant-switch"),
            ]),
        },
        Model::Spectrum128KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
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
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("cycle-profile"));
                caps
            },
        },
        Model::SpectrumPlus2 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
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
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("cycle-profile"));
                caps
            },
        },
        Model::SpectrumPlus2A | Model::SpectrumPlus2B => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
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
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("cycle-profile"));
                caps
            },
        },
        Model::SpectrumPlus3 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
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
                caps.insert(known_capability("cycle-profile"));
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
                known_capability("memory-watch"),
                known_capability("port-io"),
                known_capability("basic-program-load"),
                known_capability("tape-autoload"),
                known_capability("variant-switch"),
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
    fn variant_ids_round_trip_for_every_model() {
        for (model, id) in Model::ALL.into_iter().zip(Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(Model::from_variant_id(id), Some(model));
        }
        assert_eq!(Model::from_variant_id("spectrum_999k"), None);
    }

    #[test]
    fn every_firmware_source_is_a_profile_requirement() {
        // `from_firmware` validates the set against the profile, so a
        // source the profile does not declare could never boot.
        for model in Model::ALL {
            let profile = profile_for(model);
            let sources = model.firmware_sources();
            assert!(!sources.is_empty(), "{} has no ROMs", model.variant_id());
            for source in &sources {
                assert!(
                    profile.firmware.iter().any(|req| req.id == source.id),
                    "{} source {} is not in its profile",
                    model.variant_id(),
                    source.id
                );
            }
            assert_eq!(
                sources.len(),
                profile.firmware.len(),
                "{} declares more firmware than it sources",
                model.variant_id()
            );
        }
    }

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

    #[test]
    fn model_id_is_unique_and_stable_for_every_variant() {
        // Every entry in the catalogue exposes a `model_id` distinct
        // from the corresponding `profile_id`. Pre-Cov-5b nothing called
        // `Model::model_id`, so the match arms had no coverage.
        let models = [
            Model::Spectrum16KPal,
            Model::Spectrum48KPal,
            Model::SpectrumPlus,
            Model::Spectrum128KPal,
            Model::SpectrumPlus2,
            Model::SpectrumPlus2A,
            Model::SpectrumPlus2B,
            Model::SpectrumPlus3,
            Model::Pentagon128,
            Model::ScorpionZS256,
            Model::TimexTC2048,
            Model::TimexTC2068,
            Model::TimexTS2068,
        ];
        let mut ids: Vec<&'static str> = models.iter().map(|m| m.model_id()).collect();
        let count_before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count_before, "model ids must be unique");
        // Spot-check an obvious one to defend against an accidental
        // global rename.
        assert_eq!(Model::Spectrum48KPal.model_id(), "sinclair-zx-spectrum-48k");
    }

    #[test]
    fn release_year_covers_every_documented_variant() {
        // 1982-1991 covers the entire family — 1983 is the earliest
        // (Timex) and 1991 is the latest (Scorpion ZS-256). Without this
        // test the standalone TimexTC2068/Plus2B/Pentagon arms never run.
        assert_eq!(Model::Spectrum16KPal.release_year(), 1982);
        assert_eq!(Model::Spectrum48KPal.release_year(), 1982);
        assert_eq!(Model::SpectrumPlus.release_year(), 1984);
        assert_eq!(Model::TimexTC2068.release_year(), 1983);
        assert_eq!(Model::TimexTS2068.release_year(), 1983);
        assert_eq!(Model::Spectrum128KPal.release_year(), 1985);
        assert_eq!(Model::TimexTC2048.release_year(), 1985);
        assert_eq!(Model::SpectrumPlus2.release_year(), 1986);
        assert_eq!(Model::SpectrumPlus2A.release_year(), 1986);
        assert_eq!(Model::SpectrumPlus3.release_year(), 1986);
        assert_eq!(Model::SpectrumPlus2B.release_year(), 1988);
        assert_eq!(Model::Pentagon128.release_year(), 1989);
        assert_eq!(Model::ScorpionZS256.release_year(), 1991);
    }

    #[test]
    fn display_name_returns_a_human_readable_string_per_variant() {
        // Drives every arm of `display_name`.
        assert_eq!(
            Model::Spectrum16KPal.display_name(),
            "ZX Spectrum 16K (PAL)"
        );
        assert_eq!(
            Model::Spectrum48KPal.display_name(),
            "ZX Spectrum 48K (PAL)"
        );
        assert_eq!(Model::SpectrumPlus.display_name(), "ZX Spectrum+ (PAL)");
        assert_eq!(
            Model::Spectrum128KPal.display_name(),
            "ZX Spectrum 128K (PAL)"
        );
        assert_eq!(Model::SpectrumPlus2.display_name(), "ZX Spectrum +2 (PAL)");
        assert_eq!(
            Model::SpectrumPlus2A.display_name(),
            "ZX Spectrum +2A (PAL)"
        );
        assert_eq!(
            Model::SpectrumPlus2B.display_name(),
            "ZX Spectrum +2B (PAL)"
        );
        assert_eq!(Model::SpectrumPlus3.display_name(), "ZX Spectrum +3 (PAL)");
        assert_eq!(Model::Pentagon128.display_name(), "Pentagon 128");
        assert_eq!(Model::ScorpionZS256.display_name(), "Scorpion ZS-256");
        assert_eq!(Model::TimexTC2048.display_name(), "Timex TC2048");
        assert_eq!(Model::TimexTC2068.display_name(), "Timex TC2068");
        assert_eq!(Model::TimexTS2068.display_name(), "Timex TS2068 (NTSC)");
    }
}
