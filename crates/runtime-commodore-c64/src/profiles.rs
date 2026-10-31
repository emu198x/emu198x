//! Commodore 64 family profile catalogue.

use common_commodore_c64::timing::{C64Timing, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, FirmwareSource, MachineId,
    MachineProfile, MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

/// Supported C64 models in the fresh workspace bootstrap.
///
/// The breadbin and C64C variants share timing; the C64C carries the HMOS-II
/// VIC-II (8565 PAL, 8562 NTSC) and the MOS 8580 SID rather than the
/// breadbin's 6569/6567 and 6581.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Commodore 64 PAL breadbin (MOS 6581 SID).
    C64PalBreadbin,
    /// Commodore 64 NTSC breadbin (MOS 6581 SID).
    C64NtscBreadbin,
    /// Commodore 64C PAL (MOS 8580 SID).
    C64cPal,
    /// Commodore 64C NTSC (MOS 8580 SID).
    C64cNtsc,
}

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::C64PalBreadbin => "commodore-c64-pal-breadbin",
            Self::C64NtscBreadbin => "commodore-c64-ntsc-breadbin",
            Self::C64cPal => "commodore-c64c-pal",
            Self::C64cNtsc => "commodore-c64c-ntsc",
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
            Self::C64cPal => "Commodore 64C (PAL)",
            Self::C64cNtsc => "Commodore 64C (NTSC)",
        }
    }

    /// Every model in menu order.
    pub const ALL: [Self; 4] = [
        Self::C64PalBreadbin,
        Self::C64NtscBreadbin,
        Self::C64cPal,
        Self::C64cNtsc,
    ];

    /// Every variant id, in [`Self::ALL`] order: the `--model` spellings.
    pub const VARIANT_IDS: [&'static str; 4] = ["pal", "ntsc", "c64c-pal", "c64c-ntsc"];

    /// The id `--model`, `set_machine` and the window's variant menu use.
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::C64PalBreadbin => "pal",
            Self::C64NtscBreadbin => "ntsc",
            Self::C64cPal => "c64c-pal",
            Self::C64cNtsc => "c64c-ntsc",
        }
    }

    /// The model a variant id names; `c64c` is accepted for the PAL C64C,
    /// as `--model` always has.
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        if id == "c64c" {
            return Some(Self::C64cPal);
        }
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    /// The short label the window's Machine menu and title show: region
    /// and SID revision.
    #[must_use]
    pub const fn menu_label(self) -> &'static str {
        match self {
            Self::C64PalBreadbin => "PAL Breadbin (6581)",
            Self::C64NtscBreadbin => "NTSC Breadbin (6581)",
            Self::C64cPal => "PAL C64C (8580)",
            Self::C64cNtsc => "NTSC C64C (8580)",
        }
    }

    /// The region's timing: frame length and CPU clock.
    #[must_use]
    pub const fn timing(self) -> &'static C64Timing {
        match self {
            Self::C64PalBreadbin | Self::C64cPal => &TIMING_PAL_BREADBIN,
            Self::C64NtscBreadbin | Self::C64cNtsc => &TIMING_NTSC_BREADBIN,
        }
    }

    /// The ROMs every model boots and their conventional file names in the
    /// family's ROM directory. KERNAL, BASIC and the character generator
    /// are required; the drive DOS ROMs are optional and, when present,
    /// let the per-port drive selector offer that model.
    #[must_use]
    pub fn firmware_sources(self) -> Vec<FirmwareSource> {
        vec![
            FirmwareSource::required(
                "commodore-c64-kernal-rom",
                &["kernal.rom", "c64-kernal.rom"],
            ),
            FirmwareSource::required("commodore-c64-basic-rom", &["basic.rom", "c64-basic.rom"]),
            FirmwareSource::required(
                "commodore-c64-character-rom",
                &["chargen.rom", "c64-chargen.rom"],
            ),
            FirmwareSource::optional(
                "commodore-1541-dos-rom",
                &["1541.rom", "dos1541.rom", "c1541.rom"],
            ),
            FirmwareSource::optional(
                "commodore-1571-dos-rom",
                &["1571.rom", "dos1571.rom", "c1571.rom"],
            ),
            FirmwareSource::optional(
                "commodore-1581-dos-rom",
                &["1581.rom", "dos1581.rom", "c1581.rom"],
            ),
        ]
    }
}

/// Returns the initial C64 family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::C64PalBreadbin),
        profile_for(Model::C64NtscBreadbin),
        profile_for(Model::C64cPal),
        profile_for(Model::C64cNtsc),
    ]
}

/// Returns the profile metadata for one C64 model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let (region, summary, clock_rate, release_year) = match model {
        Model::C64PalBreadbin => (
            Region::Pal,
            "PAL breadbin baseline now boots real BASIC/KERNAL/CHARGEN ROMs to the BASIC READY. prompt in the fresh workspace. Live 6502, CIA, VIC-II, and SID are wired; headless frame and mono audio output plus runtime snapshot import/export now work. TAP-backed datasette transport is now wired through the 6510/CIA path; broader software validation is still pending.",
            ClockRate::from_hz(TIMING_PAL_BREADBIN.cpu_hz),
            1982,
        ),
        Model::C64NtscBreadbin => (
            Region::Ntsc,
            "NTSC breadbin follow-on profile on the same live 6502/CIA/VIC-II/SID substrate. Fresh-workspace frame and audio execution plus runtime snapshot support exist; datasette transport is on the shared board path, but NTSC boot validation and software/media verification are still pending.",
            ClockRate::from_hz(TIMING_NTSC_BREADBIN.cpu_hz),
            1982,
        ),
        Model::C64cPal => (
            Region::Pal,
            "PAL Commodore 64C: the same live 6502/CIA/VIC-II substrate as the PAL breadbin, fitted with the cost-reduced MOS 8580 SID (more linear filter, distinct combined-waveform behaviour). Boots real BASIC/KERNAL/CHARGEN ROMs to READY.; the shared datasette/disk/cartridge paths apply.",
            ClockRate::from_hz(TIMING_PAL_BREADBIN.cpu_hz),
            1986,
        ),
        Model::C64cNtsc => (
            Region::Ntsc,
            "NTSC Commodore 64C on the shared 6502/CIA/VIC-II substrate with the MOS 8580 SID. Fresh-workspace frame and audio execution plus snapshot support exist; NTSC boot and software/media verification are still pending, as for the NTSC breadbin.",
            ClockRate::from_hz(TIMING_NTSC_BREADBIN.cpu_hz),
            1986,
        ),
    };

    MachineProfile {
        machine_id: MachineId::from(model.model_id()),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::C64,
        region,
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
            FirmwareRequirement::new("commodore-1541-dos-rom", "1541 DOS ROM", true),
            FirmwareRequirement::new("commodore-1571-dos-rom", "1571 DOS ROM", true),
            FirmwareRequirement::new("commodore-1581-dos-rom", "1581 DOS ROM", true),
        ],
        media_slots: vec![
            MediaSlot::new(
                "tape-1",
                "Datasette",
                MediaKind::Tape,
                false,
                WritebackPolicy::SidecarOnly,
            ),
            MediaSlot::new(
                "drive-8",
                "Disk Drive 8",
                MediaKind::Disk,
                false,
                WritebackPolicy::SidecarOnly,
            ),
            MediaSlot::new(
                "drive-9",
                "Disk Drive 9 (1581)",
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
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("keyboard-matrix"),
            known_capability("snapshot-export"),
            known_capability("snapshot-import"),
            known_capability("scripted-input"),
            known_capability("tape-transport-control"),
            known_capability("basic-program-load"),
            known_capability("tape-autoload"),
            known_capability("variant-switch"),
        ]),
    }
}

/// Commodore 128 models, in C128 mode.
///
/// Both run the same international ROMs; the KERNAL tells PAL from NTSC by
/// counting the VIC-IIe's lines. The machine does not yet model C64 mode,
/// the 2 MHz clock, the datasette, serial drives or cartridges, so the
/// profiles offer no media slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum C128Model {
    /// PAL C128: 8566 VIC-IIe, 985,248 Hz.
    C128Pal,
    /// NTSC C128: 8564 VIC-IIe, 1,022,727 Hz.
    C128Ntsc,
}

impl C128Model {
    pub const ALL: [Self; 2] = [Self::C128Pal, Self::C128Ntsc];

    /// Distinct from the C64's ids, so one variant list can hold both.
    pub const VARIANT_IDS: [&'static str; 2] = ["c128-pal", "c128-ntsc"];

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::C128Pal => "commodore-c128-pal",
            Self::C128Ntsc => "commodore-c128-ntsc",
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::C128Pal => "Commodore 128 (PAL)",
            Self::C128Ntsc => "Commodore 128 (NTSC)",
        }
    }

    #[must_use]
    pub const fn menu_label(self) -> &'static str {
        match self {
            Self::C128Pal => "PAL C128 (8566)",
            Self::C128Ntsc => "NTSC C128 (8564)",
        }
    }

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::C128Pal => "c128-pal",
            Self::C128Ntsc => "c128-ntsc",
        }
    }

    /// `c128` alone names the PAL machine.
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        if id == "c128" {
            return Some(Self::C128Pal);
        }
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    /// The machine crate's model for this one.
    #[must_use]
    pub const fn machine_model(self) -> common_commodore_c128::C128Model {
        match self {
            Self::C128Pal => common_commodore_c128::C128Model::Pal,
            Self::C128Ntsc => common_commodore_c128::C128Model::Ntsc,
        }
    }

    /// The ROMs, under VICE's names first (as its `data/C128` ships them,
    /// and as the machine's own boot tests read them).
    ///
    /// The family app looks in the C64's ROM directory, so each name is
    /// also tried in a `c128/` directory inside it; a C128 runtime on its
    /// own looks in `commodore-c128` (see `rom_convention`).
    #[must_use]
    pub fn firmware_sources(self) -> Vec<FirmwareSource> {
        vec![
            FirmwareSource::required(
                "commodore-c128-kernal-rom",
                &[
                    "kernal-318020-05.bin",
                    "c128-kernal.rom",
                    "c128/kernal-318020-05.bin",
                ],
            ),
            FirmwareSource::required(
                "commodore-c128-basic-lo-rom",
                &[
                    "basiclo-318018-04.bin",
                    "c128-basiclo.rom",
                    "c128/basiclo-318018-04.bin",
                ],
            ),
            FirmwareSource::required(
                "commodore-c128-basic-hi-rom",
                &[
                    "basichi-318019-04.bin",
                    "c128-basichi.rom",
                    "c128/basichi-318019-04.bin",
                ],
            ),
            FirmwareSource::required(
                "commodore-c128-character-rom",
                &[
                    "chargen-390059-01.bin",
                    "c128-chargen.rom",
                    "c128/chargen-390059-01.bin",
                ],
            ),
        ]
    }
}

/// Every C128 profile.
#[must_use]
pub fn c128_profiles() -> Vec<MachineProfile> {
    C128Model::ALL.into_iter().map(c128_profile_for).collect()
}

/// The profile for one C128 model.
#[must_use]
pub fn c128_profile_for(model: C128Model) -> MachineProfile {
    let (region, summary, clock_rate) = match model {
        C128Model::C128Pal => (
            Region::Pal,
            "PAL Commodore 128 in C128 mode. The Z80 boots first and hands the bus to the 8502, which runs BASIC 7.0 to READY. on the 8566 VIC-IIe; with the 40/80 key down the KERNAL brings BASIC up on the 8563 VDC's 80-column screen instead. Real ROMs boot on both screens. Not yet modelled: C64 mode, the 2 MHz clock, the datasette, serial drives and cartridges.",
            ClockRate::from_hz(TIMING_PAL_BREADBIN.cpu_hz),
        ),
        C128Model::C128Ntsc => (
            Region::Ntsc,
            "NTSC Commodore 128 in C128 mode on the 8564 VIC-IIe: the same international ROMs boot BASIC 7.0 to READY. on 40 or 80 columns. Not yet modelled: C64 mode, the 2 MHz clock, the datasette, serial drives and cartridges.",
            ClockRate::from_hz(TIMING_NTSC_BREADBIN.cpu_hz),
        ),
    };

    MachineProfile {
        machine_id: MachineId::from(model.model_id()),
        profile_id: ProfileId::from(model.model_id()),
        display_name: model.display_name().into(),
        family: Family::C64,
        region,
        release_year: 1985,
        summary: summary.into(),
        clock: ClockDesc::new("phi2-cycle", clock_rate),
        firmware: vec![
            FirmwareRequirement::new("commodore-c128-kernal-rom", "C128 KERNAL ROM", false),
            FirmwareRequirement::new("commodore-c128-basic-lo-rom", "C128 BASIC LO ROM", false),
            FirmwareRequirement::new("commodore-c128-basic-hi-rom", "C128 BASIC HI ROM", false),
            FirmwareRequirement::new(
                "commodore-c128-character-rom",
                "C128 Character Generator ROM",
                false,
            ),
        ],
        media_slots: Vec::new(),
        capabilities: CapabilitySet::with_all([
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("keyboard-matrix"),
            known_capability("snapshot-export"),
            known_capability("snapshot-import"),
            known_capability("scripted-input"),
            known_capability("variant-switch"),
        ]),
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
        assert_eq!(Model::from_variant_id("c64c"), Some(Model::C64cPal));
        assert_eq!(Model::from_variant_id("secam"), None);
    }

    #[test]
    fn every_firmware_source_is_a_profile_requirement_with_the_same_optionality() {
        for model in Model::ALL {
            let profile = profile_for(model);
            for source in model.firmware_sources() {
                let requirement = profile
                    .firmware
                    .iter()
                    .find(|req| req.id == source.id)
                    .unwrap_or_else(|| panic!("{} is not in the profile", source.id));
                assert_eq!(requirement.optional, source.optional, "{}", source.id);
            }
        }
    }

    #[test]
    fn profile_ids_are_unique() {
        let profiles: Vec<_> = profiles().into_iter().chain(c128_profiles()).collect();
        let mut ids: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn c128_variant_ids_round_trip_and_do_not_collide_with_the_c64s() {
        for (model, id) in C128Model::ALL.into_iter().zip(C128Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(C128Model::from_variant_id(id), Some(model));
            assert_eq!(Model::from_variant_id(id), None);
        }
        assert_eq!(C128Model::from_variant_id("c128"), Some(C128Model::C128Pal));
        assert_eq!(C128Model::from_variant_id("pal"), None);
    }

    #[test]
    fn every_c128_firmware_source_is_a_required_profile_requirement() {
        for model in C128Model::ALL {
            let profile = c128_profile_for(model);
            assert_eq!(profile.machine_id.as_str(), model.model_id());
            assert_eq!(profile.firmware.len(), model.firmware_sources().len());
            for source in model.firmware_sources() {
                let requirement = profile
                    .firmware
                    .iter()
                    .find(|req| req.id == source.id)
                    .unwrap_or_else(|| panic!("{} is not in the profile", source.id));
                assert!(!requirement.optional && !source.optional, "{}", source.id);
            }
        }
        assert_eq!(c128_profile_for(C128Model::C128Ntsc).region, Region::Ntsc);
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
    fn both_profiles_require_declared_roms() {
        for profile in profiles() {
            let ids: Vec<&str> = profile.firmware.iter().map(|rom| rom.id.as_ref()).collect();
            assert_eq!(
                ids,
                vec![
                    "commodore-c64-basic-rom",
                    "commodore-c64-kernal-rom",
                    "commodore-c64-character-rom",
                    "commodore-1541-dos-rom",
                    "commodore-1571-dos-rom",
                    "commodore-1581-dos-rom",
                ]
            );
            assert!(!profile.firmware[0].optional);
            assert!(!profile.firmware[1].optional);
            assert!(!profile.firmware[2].optional);
            assert!(profile.firmware[3].optional);
            assert!(profile.firmware[4].optional);
            assert!(profile.firmware[5].optional);
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
        assert_eq!(ids, vec!["tape-1", "drive-8", "drive-9", "cartridge-1"]);
        assert_eq!(profile.media_slots[0].kind, MediaKind::Tape);
        assert_eq!(profile.media_slots[1].kind, MediaKind::Disk);
        assert_eq!(profile.media_slots[2].kind, MediaKind::Disk);
        assert_eq!(profile.media_slots[3].kind, MediaKind::Cartridge);
        assert_eq!(
            profile.media_slots[1].writeback,
            WritebackPolicy::SidecarOnly
        );
        // The datasette also flushes a SAVE to a sidecar `.tap`.
        assert_eq!(
            profile.media_slots[0].writeback,
            WritebackPolicy::SidecarOnly
        );
    }
}
