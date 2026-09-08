//! Spectravideo SVI-328 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, FirmwareSource, MachineId,
    MachineProfile, MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// SVI-328 NTSC (US).
    Svi328Ntsc,
    /// SVI-328 PAL (EU).
    Svi328Pal,
}

impl Model {
    pub const ALL: [Self; 2] = [Self::Svi328Ntsc, Self::Svi328Pal];
    pub const VARIANT_IDS: [&'static str; 2] =
        ["spectravideo-svi-328-ntsc", "spectravideo-svi-328-pal"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.profile_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<FirmwareSource> {
        vec![
            FirmwareSource::required(BIOS_FIRMWARE_ID, &["svi-328.rom"])
                .with_env_var("EMU198X_SVI_328_BIOS"),
        ]
    }

    /// Existing host frame budget, in CPU clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Svi328Ntsc => 228 * 262,
            Self::Svi328Pal => 228 * 313,
        }
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Svi328Ntsc => "spectravideo-svi-328-ntsc",
            Self::Svi328Pal => "spectravideo-svi-328-pal",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Svi328Ntsc => "Spectravideo SVI-328 (NTSC)",
            Self::Svi328Pal => "Spectravideo SVI-328 (PAL)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Svi328Ntsc => Region::Ntsc,
            Self::Svi328Pal => Region::Pal,
        }
    }
}

pub const BIOS_FIRMWARE_ID: &str = "spectravideo-svi-328-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("spectravideo-svi-328"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1983,
        summary:
            "Spectravideo SVI-328 — Z80A + 32 KB BASIC ROM, 64 KB RAM, TMS9918 VDP, AY-3-8910 PSG."
                .into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "SVI-328 system ROM (32 KB)",
            false,
        )],
        media_slots: vec![
            MediaSlot::new(
                "cartridge-1",
                "Cartridge Slot",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
            MediaSlot::new(
                "tape-1",
                "Cassette Tape",
                MediaKind::Tape,
                false,
                WritebackPolicy::SidecarOnly,
            ),
        ],
        capabilities: CapabilitySet::with_all([
            known_capability("variant-switch"),
            known_capability("keyboard-input"),
            known_capability("ay-audio"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_profiles() {
        let p = profiles();
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|p| p.firmware.len() == 1));
        assert!(p.iter().all(|p| {
            p.media_slots
                .iter()
                .any(|slot| slot.id.as_ref() == "tape-1" && slot.kind == MediaKind::Tape)
        }));
    }
}
