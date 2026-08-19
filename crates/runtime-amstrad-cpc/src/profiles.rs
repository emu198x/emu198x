//! Amstrad CPC family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

/// The CPC models this runtime can build.
///
/// Only the 464 for now: the 664 and 6128 add a disc drive and, on the 6128,
/// banked RAM — neither of which `machine-amstrad-cpc` models. Adding a variant
/// here before the machine can run it would advertise a machine that does not
/// exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    Cpc464,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        "amstrad-cpc464"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        "Amstrad CPC464"
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const ROM_FIRMWARE_ID: &str = "amstrad-cpc464-firmware";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Cpc464)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("amstrad-cpc"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1984,
        summary: "Amstrad CPC464 — Z80A at 4 MHz, Gate Array, HD6845S CRTC, AY-3-8912, Intel 8255 PPI, 64 KB RAM, 32 KB firmware, cassette."
            .into(),
        // The Z80 runs at 4 MHz off a 16 MHz crystal; the runtime's time unit
        // is that T-state, which is what `AmstradCpc::run_frame` returns.
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(4_000_000)),
        firmware: vec![FirmwareRequirement::new(
            ROM_FIRMWARE_ID,
            "CPC464 firmware (32 KB: 16 KB OS + 16 KB BASIC)",
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
    fn profile_declares_firmware_and_a_tape_slot() {
        let p = profile_for(Model::Cpc464);
        assert_eq!(p.firmware.len(), 1);
        assert_eq!(p.firmware[0].id, ROM_FIRMWARE_ID);
        assert_eq!(p.media_slots.len(), 1);
        assert_eq!(p.media_slots[0].id, "tape-1");
    }
}
