//! Sord M5 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Sord M5 NTSC.
    M5Ntsc,
    /// Sord M5 PAL.
    M5Pal,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::M5Ntsc => "sord-m5-ntsc",
            Self::M5Pal => "sord-m5-pal",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::M5Ntsc => "Sord M5 (NTSC)",
            Self::M5Pal => "Sord M5 (PAL)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::M5Ntsc => Region::Ntsc,
            Self::M5Pal => Region::Pal,
        }
    }
}

pub const ROM_FIRMWARE_ID: &str = "sord-m5-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::M5Ntsc), profile_for(Model::M5Pal)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sord-m5"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1982,
        summary: "Sord M5 — Z80A + TMS9918A + SN76489, 4 KB RAM, monitor + BASIC-I ROM in 8 KB, cartridge slot.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware: vec![FirmwareRequirement::new(
            ROM_FIRMWARE_ID,
            "Sord M5 monitor + BASIC-I ROM (8 KB)",
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
    fn profile_declares_rom_firmware() {
        let p = profile_for(Model::M5Ntsc);
        assert_eq!(p.firmware.len(), 1);
    }
}
