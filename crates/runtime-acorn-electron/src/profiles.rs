//! Acorn Electron family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    Electron,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        "acorn-electron"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        "Acorn Electron"
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

/// OS ROM firmware identifier (16 KB).
pub const OS_FIRMWARE_ID: &str = "acorn-electron-os";
/// BASIC ROM firmware identifier (16 KB).
pub const BASIC_FIRMWARE_ID: &str = "acorn-electron-basic";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Electron)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("acorn-electron"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1983,
        summary: "Acorn Electron — 6502A + ULA + 32 KB RAM, dual 16 KB ROMs (OS + BBC BASIC II).".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(2_000_000)),
        firmware: vec![
            FirmwareRequirement::new(OS_FIRMWARE_ID, "Electron OS ROM (16 KB)", false),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID, "BBC BASIC II ROM (16 KB)", false),
        ],
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
    fn profile_declares_dual_firmware() {
        let p = profile_for(Model::Electron);
        assert_eq!(p.firmware.len(), 2);
        assert_eq!(p.firmware[0].id.as_ref(), OS_FIRMWARE_ID);
        assert_eq!(p.firmware[1].id.as_ref(), BASIC_FIRMWARE_ID);
    }
}
