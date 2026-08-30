//! Tatung Einstein family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    Einstein,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        "tatung-einstein"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        "Tatung Einstein"
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const ROM_FIRMWARE_ID: &str = "tatung-einstein-mos";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Einstein)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("tatung-einstein"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1984,
        summary: "Tatung Einstein TC-01 — Z80A + TMS9929A + AY-3-8910 + Intel 8251 + WD1770, 64 KB RAM, 8 KB MOS ROM.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(4_000_000)),
        firmware: vec![FirmwareRequirement::new(
            ROM_FIRMWARE_ID,
            "Einstein MOS ROM (8 KB)",
            false,
        )],
        media_slots: vec![MediaSlot::new(
            "floppy-0",
            "Floppy Drive 0",
            MediaKind::Disk,
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
    fn profile_declares_rom_firmware_and_floppy() {
        let p = profile_for(Model::Einstein);
        assert_eq!(p.firmware.len(), 1);
        assert_eq!(p.media_slots.len(), 1);
        assert_eq!(p.media_slots[0].id, "floppy-0");
        assert_eq!(p.media_slots[0].kind, MediaKind::Disk);
    }
}
