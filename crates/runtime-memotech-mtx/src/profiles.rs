//! MTX family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    ProfileId, Region, SupportTier, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// MTX 500 (32 KB RAM).
    Mtx500,
    /// MTX 512 (64 KB RAM).
    Mtx512,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Mtx500 => "memotech-mtx-500",
            Self::Mtx512 => "memotech-mtx-512",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Mtx500 => "Memotech MTX 500",
            Self::Mtx512 => "Memotech MTX 512",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const ROM_FIRMWARE_ID: &str = "memotech-mtx-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Mtx500), profile_for(Model::Mtx512)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("memotech-mtx"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1983,
        summary: "Memotech MTX — Z80A + TMS9918A + SN76489, 16 KB OS + BASIC ROM, 32 or 64 KB RAM."
            .into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(4_000_000)),
        firmware: vec![FirmwareRequirement::new(
            ROM_FIRMWARE_ID,
            "MTX OS + BASIC ROM (16 KB)",
            false,
        )],
        media_slots: vec![],
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
        let p = profile_for(Model::Mtx500);
        assert_eq!(p.firmware.len(), 1);
    }
}
