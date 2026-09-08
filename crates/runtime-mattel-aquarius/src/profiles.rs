//! Mattel Aquarius family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Mattel Aquarius (NTSC).
    Aquarius,
}

impl Model {
    pub const ALL: [Self; 1] = [Self::Aquarius];
    pub const VARIANT_IDS: [&'static str; 1] = ["mattel-aquarius"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        vec![
            emu198x_shell::FirmwareSource::required(BIOS_FIRMWARE_ID, &["aquarius.rom"])
                .with_env_var("EMU198X_AQUARIUS_BIOS"),
            emu198x_shell::FirmwareSource::required(CHAR_FIRMWARE_ID, &["aquarius-char.rom"])
                .with_env_var("EMU198X_AQUARIUS_CHAR"),
        ]
    }

    #[must_use]
    pub const fn machine_region(self) -> machine_mattel_aquarius::AquariusRegion {
        machine_mattel_aquarius::AquariusRegion::Ntsc
    }

    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        self.machine_region().tstates_per_frame()
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Aquarius => "mattel-aquarius",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        "Mattel Aquarius"
    }

    #[must_use]
    /// The runtime builds the Mattel US machine, which is NTSC (see
    /// `rebuild_machine`); the profile said PAL until 2026-09-07 while
    /// every host budget assumed PAL too, so one budgeted frame ran two.
    pub const fn region(self) -> Region {
        Region::Ntsc
    }
}

pub const BIOS_FIRMWARE_ID: &str = "mattel-aquarius-rom";
/// Firmware id for the separate 2 KB character-generator ROM.
pub const CHAR_FIRMWARE_ID: &str = "mattel-aquarius-char-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("mattel-aquarius"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1983,
        summary: "Mattel Aquarius — Z80A + Microsoft BASIC ROM (8 KB), 4 KB internal RAM, optional 16 KB expansion, character display.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware: vec![
            FirmwareRequirement::new(
                BIOS_FIRMWARE_ID,
                "Aquarius BASIC ROM (8 KB)",
                false,
            ),
            FirmwareRequirement::new(
                CHAR_FIRMWARE_ID,
                "Aquarius character-generator ROM (2 KB)",
                false,
            ),
        ],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            false,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("keyboard-input"),
            known_capability("ay-audio"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_uses_ntsc_region() {
        let p = profile_for(Model::Aquarius);
        assert_eq!(p.region, Region::Ntsc);
        assert_eq!(p.firmware.len(), 2);
        assert_eq!(p.firmware[0].id.as_ref(), BIOS_FIRMWARE_ID);
        assert_eq!(p.firmware[1].id.as_ref(), CHAR_FIRMWARE_ID);
        assert!(p.firmware.iter().all(|firmware| !firmware.optional));
    }
}
