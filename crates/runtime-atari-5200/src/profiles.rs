//! Atari 5200 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

/// The 5200 shipped in one television standard.
///
/// Atari's own *CX5200 Field Service Manual* (Rev 4, 1983) tells
/// technicians that the power-up screen "displays … the type of TIA in
/// the unit. NTSC appears if the GTIA is the proper one for that unit.
/// If PAL appears, replace with a GTIA from your kit." A PAL GTIA in a
/// 5200 is a part to swap out, not a regional variant, so there is no
/// PAL model to select.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Atari 5200 NTSC.
    A5200Ntsc,
}

impl Model {
    pub const ALL: [Self; 1] = [Self::A5200Ntsc];
    pub const VARIANT_IDS: [&'static str; 1] = [Self::A5200Ntsc.variant_id()];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        vec![
            emu198x_shell::FirmwareSource::optional(BIOS_FIRMWARE_ID, &["bios.rom", "5200.rom"])
                .with_env_var("EMU198X_A5200_BIOS"),
        ]
    }

    /// Existing native host budget, in colour clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        262 * 228
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A5200Ntsc => "atari-5200-ntsc",
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
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::A5200Ntsc => Region::Ntsc,
        }
    }
}

pub const BIOS_FIRMWARE_ID: &str = "atari-5200-bios";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("atari-5200"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
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
