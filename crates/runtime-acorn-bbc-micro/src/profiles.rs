//! BBC Micro family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    ProfileId, Region, SupportTier, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// BBC Micro Model B.
    BbcModelB,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        "acorn-bbc-micro-b"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        "Acorn BBC Micro Model B"
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const MOS_FIRMWARE_ID: &str = "acorn-bbc-mos";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::BbcModelB)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("acorn-bbc-micro"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1981,
        summary: "Acorn BBC Micro Model B — 6502 + 6845 CRTC + Video ULA + 2× 6522 VIA + SN76489 + Intel 8271, 16 KB MOS ROM, 16 KB sideways ROM slots.".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(2_000_000)),
        firmware: vec![FirmwareRequirement::new(
            MOS_FIRMWARE_ID,
            "BBC MOS ROM (16 KB)",
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
    fn profile_declares_mos_firmware() {
        let p = profile_for(Model::BbcModelB);
        assert_eq!(p.firmware.len(), 1);
    }
}
