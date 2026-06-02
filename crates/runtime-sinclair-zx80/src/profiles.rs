//! ZX80 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    Zx80,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        "sinclair-zx80"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        "Sinclair ZX80"
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const ROM_FIRMWARE_ID: &str = "sinclair-zx80-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Zx80)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sinclair-zx80"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Spectrum,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1980,
        summary: "Sinclair ZX80 — Z80A + custom keyboard / display logic, 1 KB internal RAM (16 KB expansion), 4 KB monitor ROM.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_250_000)),
        firmware: vec![FirmwareRequirement::new(
            ROM_FIRMWARE_ID,
            "ZX80 monitor ROM (4 KB)",
            false,
        )],
        media_slots: vec![MediaSlot::new(
            "tape-1",
            "Cassette Tape",
            MediaKind::Tape,
            false,
            WritebackPolicy::SidecarOnly,
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
        let p = profile_for(Model::Zx80);
        assert_eq!(p.firmware.len(), 1);
        assert_eq!(p.firmware[0].id.as_ref(), ROM_FIRMWARE_ID);
    }
}
