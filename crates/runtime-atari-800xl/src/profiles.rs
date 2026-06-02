//! Atari 800XL family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    A800xlNtsc,
    A800xlPal,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A800xlNtsc => "atari-800xl-ntsc",
            Self::A800xlPal => "atari-800xl-pal",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::A800xlNtsc => "Atari 800XL (NTSC)",
            Self::A800xlPal => "Atari 800XL (PAL)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::A800xlNtsc => Region::Ntsc,
            Self::A800xlPal => Region::Pal,
        }
    }
}

pub const OS_FIRMWARE_ID: &str = "atari-800xl-os";
pub const BASIC_FIRMWARE_ID: &str = "atari-800xl-basic";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::A800xlNtsc),
        profile_for(Model::A800xlPal),
    ]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("atari-800xl"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1983,
        summary: "Atari 800XL — 6502C + ANTIC + GTIA + POKEY + PIA, 64 KB RAM, optional 16 KB OS ROM + 8 KB BASIC ROM, optional cartridge.".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(1_790_000)),
        firmware: vec![
            FirmwareRequirement::new(OS_FIRMWARE_ID, "Atari 800XL OS ROM (16 KB) — optional", true),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID, "Atari BASIC ROM (8 KB) — optional", true),
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
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_declares_two_optional_firmware() {
        let p = profile_for(Model::A800xlNtsc);
        assert_eq!(p.firmware.len(), 2);
        assert!(p.firmware.iter().all(|f| f.optional));
    }
}
