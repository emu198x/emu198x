//! Atari ST profile catalogue.

use crate::AtariStRuntime;
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};
use machine_atari_st::{Monitor, NTSC_MASTER_HZ, PAL_MASTER_HZ, VideoStandard};

/// The TOS image's firmware id.
pub const TOS_FIRMWARE_ID: &str = "atari-st-tos";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// 520ST(F/M), 512 KB, PAL.
    St520Pal,
    /// 520ST(F/M), 512 KB, NTSC.
    St520Ntsc,
    /// 1040ST(F), 1 MB, PAL.
    St1040Pal,
    /// 1040ST(F), 1 MB, NTSC.
    St1040Ntsc,
}

impl Model {
    pub const ALL: [Self; 4] = [
        Self::St520Pal,
        Self::St520Ntsc,
        Self::St1040Pal,
        Self::St1040Ntsc,
    ];
    pub const VARIANT_IDS: [&'static str; 4] =
        ["520st-pal", "520st-ntsc", "1040st-pal", "1040st-ntsc"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::St520Pal => "520st-pal",
            Self::St520Ntsc => "520st-ntsc",
            Self::St1040Pal => "1040st-pal",
            Self::St1040Ntsc => "1040st-ntsc",
        }
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|model| model.variant_id() == id || model.profile_id() == id)
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::St520Pal => "atari-520st-pal",
            Self::St520Ntsc => "atari-520st-ntsc",
            Self::St1040Pal => "atari-1040st-pal",
            Self::St1040Ntsc => "atari-1040st-ntsc",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::St520Pal => "Atari 520ST (PAL)",
            Self::St520Ntsc => "Atari 520ST (NTSC)",
            Self::St1040Pal => "Atari 1040ST (PAL)",
            Self::St1040Ntsc => "Atari 1040ST (NTSC)",
        }
    }

    /// The machine crate's model: how much RAM.
    #[must_use]
    pub const fn machine_model(self) -> machine_atari_st::Model {
        match self {
            Self::St520Pal | Self::St520Ntsc => machine_atari_st::Model::St520,
            Self::St1040Pal | Self::St1040Ntsc => machine_atari_st::Model::St1040,
        }
    }

    /// The machine crate's video standard: which master clock.
    #[must_use]
    pub const fn standard(self) -> VideoStandard {
        match self {
            Self::St520Pal | Self::St1040Pal => VideoStandard::Pal,
            Self::St520Ntsc | Self::St1040Ntsc => VideoStandard::Ntsc,
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self.standard() {
            VideoStandard::Pal => Region::Pal,
            VideoStandard::Ntsc => Region::Ntsc,
        }
    }

    /// The CPU clock, a quarter of the master clock: the unit of
    /// `MachineTime`.
    #[must_use]
    pub const fn cpu_hz(self) -> u64 {
        match self.standard() {
            VideoStandard::Pal => PAL_MASTER_HZ / 4,
            VideoStandard::Ntsc => NTSC_MASTER_HZ / 4,
        }
    }

    /// One frame in CPU cycles, as TOS sets the video up: 313 lines of 512
    /// at 50 Hz on a PAL colour machine, 263 of 508 at 60 Hz on NTSC, and
    /// 501 of 224 at 71 Hz on a monochrome monitor.
    #[must_use]
    pub const fn frame_ticks(self, monitor: Monitor) -> u64 {
        match (monitor, self.standard()) {
            (Monitor::Mono, _) => 501 * 224,
            (Monitor::Colour, VideoStandard::Pal) => 313 * 512,
            (Monitor::Colour, VideoStandard::Ntsc) => 263 * 508,
        }
    }

    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::St520Pal | Self::St520Ntsc => 1985,
            Self::St1040Pal | Self::St1040Ntsc => 1986,
        }
    }

    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::St520Pal => {
                "Atari 520ST — 68000 at 8 MHz, GLUE/MMU/Shifter, MFP 68901, YM2149, WD1772, 512 KB RAM, PAL."
            }
            Self::St520Ntsc => {
                "Atari 520ST — 68000 at 8 MHz, GLUE/MMU/Shifter, MFP 68901, YM2149, WD1772, 512 KB RAM, NTSC."
            }
            Self::St1040Pal => {
                "Atari 1040ST — 68000 at 8 MHz, GLUE/MMU/Shifter, MFP 68901, YM2149, WD1772, 1 MB RAM, PAL."
            }
            Self::St1040Ntsc => {
                "Atari 1040ST — 68000 at 8 MHz, GLUE/MMU/Shifter, MFP 68901, YM2149, WD1772, 1 MB RAM, NTSC."
            }
        }
    }

    /// Where the launcher looks for TOS: an `atari-st` directory under the
    /// ROM root, the files tried in order.
    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        vec![
            emu198x_shell::FirmwareSource::required(
                TOS_FIRMWARE_ID,
                &[
                    "tos.img",
                    "tos.rom",
                    "etos256uk.img",
                    "etos256us.img",
                    "etos192uk.img",
                    "etos192us.img",
                ],
            )
            .with_env_var("EMU198X_ATARI_ST_TOS"),
        ]
    }
}

/// A runtime with no TOS: it waits for firmware.
#[must_use]
pub fn blank(model: Model) -> AtariStRuntime {
    AtariStRuntime::blank(model)
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("atari-st"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: model.release_year(),
        summary: model.summary().into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(model.cpu_hz())),
        firmware: vec![FirmwareRequirement::new(
            TOS_FIRMWARE_ID,
            "TOS or EmuTOS (192 KB or 256 KB)",
            false,
        )],
        media_slots: vec![
            MediaSlot::new(
                "floppy-0",
                "Floppy Drive A",
                MediaKind::Disk,
                true,
                WritebackPolicy::InMemoryOnly,
            ),
            MediaSlot::new(
                "floppy-1",
                "Floppy Drive B",
                MediaKind::Disk,
                true,
                WritebackPolicy::InMemoryOnly,
            ),
        ],
        capabilities: CapabilitySet::with_all([
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("variant-switch"),
            known_capability("keyboard-input"),
            known_capability("mouse-input"),
            known_capability("controller-input"),
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
        let mut ids: Vec<&str> = profiles.iter().map(|p| p.profile_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn variant_ids_round_trip() {
        for (model, id) in Model::ALL.into_iter().zip(Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(Model::from_variant_id(id), Some(model));
            assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        }
        assert_eq!(Model::from_variant_id("mega-ste"), None);
    }

    #[test]
    fn the_clock_is_a_quarter_of_the_master_clock() {
        assert_eq!(
            profile_for(Model::St520Pal).clock.rate.numerator_hz,
            8_021_247
        );
        assert_eq!(
            profile_for(Model::St1040Ntsc).clock.rate.numerator_hz,
            8_010_600
        );
        let hz = |model: Model, monitor| model.cpu_hz() as f64 / model.frame_ticks(monitor) as f64;
        assert!((hz(Model::St520Pal, Monitor::Colour) - 50.05).abs() < 0.01);
        assert!((hz(Model::St520Ntsc, Monitor::Colour) - 59.96).abs() < 0.01);
        assert!((hz(Model::St1040Pal, Monitor::Mono) - 71.47).abs() < 0.01);
    }

    #[test]
    fn tos_is_required_and_both_drives_are_offered() {
        for profile in profiles() {
            assert_eq!(profile.machine_id.as_str(), "atari-st");
            assert_eq!(profile.firmware.len(), 1);
            assert!(!profile.firmware[0].optional);
            assert_eq!(profile.media_slots.len(), 2);
        }
        assert_eq!(Model::St520Pal.firmware_sources()[0].id, TOS_FIRMWARE_ID);
        assert_eq!(
            Model::St1040Pal.machine_model(),
            machine_atari_st::Model::St1040
        );
    }
}
