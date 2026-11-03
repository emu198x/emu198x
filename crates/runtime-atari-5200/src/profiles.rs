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
    /// Original four-port board.
    Ca018087,
    /// Four-port universal board.
    Ca020108,
    /// Two-port universal board.
    Ca021374,
}

impl Model {
    pub const ALL: [Self; 3] = [Self::Ca018087, Self::Ca020108, Self::Ca021374];
    pub const VARIANT_IDS: [&'static str; 3] = [
        Self::Ca018087.variant_id(),
        Self::Ca020108.variant_id(),
        Self::Ca021374.variant_id(),
    ];

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
            Self::Ca018087 => "atari-5200-ca018087",
            Self::Ca020108 => "atari-5200-ca020108",
            Self::Ca021374 => "atari-5200-ca021374",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Ca018087 => "Atari 5200 CA018087 (four-port NTSC)",
            Self::Ca020108 => "Atari 5200 CA020108 (four-port universal NTSC)",
            Self::Ca021374 => "Atari 5200 CA021374 (two-port universal NTSC)",
        }
    }
    #[must_use]
    pub fn machine_id(self) -> MachineId {
        match self {
            Self::Ca018087 => MachineId::from("atari-5200-ca018087"),
            Self::Ca020108 => MachineId::from("atari-5200-ca020108"),
            Self::Ca021374 => MachineId::from("atari-5200-ca021374"),
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Ca018087 | Self::Ca020108 | Self::Ca021374 => Region::Ntsc,
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
        machine_id: model.machine_id(),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1982,
        summary: "Atari 5200 — 6502C + ANTIC + GTIA + POKEY, 16 KB RAM, optional 2 KB BIOS, cartridge required. VCS adaptor and analogue calibration remain unqualified.".into(),
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
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
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
        let p = profile_for(Model::Ca018087);
        assert_eq!(p.firmware.len(), 1);
        assert!(p.firmware[0].optional);
    }
}
