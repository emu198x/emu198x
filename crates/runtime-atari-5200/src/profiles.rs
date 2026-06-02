//! Atari 5200 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Atari 5200 NTSC.
    A5200Ntsc,
    /// Atari 5200 PAL.
    A5200Pal,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A5200Ntsc => "atari-5200-ntsc",
            Self::A5200Pal => "atari-5200-pal",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::A5200Ntsc => "Atari 5200 (NTSC)",
            Self::A5200Pal => "Atari 5200 (PAL)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::A5200Ntsc => Region::Ntsc,
            Self::A5200Pal => Region::Pal,
        }
    }
}

pub const BIOS_FIRMWARE_ID: &str = "atari-5200-bios";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::A5200Ntsc), profile_for(Model::A5200Pal)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("atari-5200"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1982,
        summary: "Atari 5200 — 6502C + ANTIC + GTIA + POKEY, 16 KB RAM, optional 2 KB BIOS, cartridge required.".into(),
        clock: ClockDesc::new("colour-clock", ClockRate::from_hz(3_579_545)),
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "Atari 5200 BIOS (2 KB) — optional",
            true,
        )],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            true,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("controller-input"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_declares_optional_bios() {
        let p = profile_for(Model::A5200Ntsc);
        assert_eq!(p.firmware.len(), 1);
        assert!(p.firmware[0].optional);
    }
}
