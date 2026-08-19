//! Mattel Aquarius family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Mattel Aquarius (PAL — Aquarius was Europe-first).
    Aquarius,
}

impl Model {
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
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const BIOS_FIRMWARE_ID: &str = "mattel-aquarius-rom";
/// Firmware id for the separate 2 KB character-generator ROM.
pub const CHAR_FIRMWARE_ID: &str = "mattel-aquarius-char-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Aquarius)]
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
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "Aquarius BASIC ROM (8 KB)",
            false,
        )],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            false,
            WritebackPolicy::InMemoryOnly,
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
    fn profile_uses_pal_region() {
        let p = profile_for(Model::Aquarius);
        assert_eq!(p.region, Region::Pal);
        assert_eq!(p.firmware.len(), 1);
        assert_eq!(p.firmware[0].id.as_ref(), BIOS_FIRMWARE_ID);
    }
}
