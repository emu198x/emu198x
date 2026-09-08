//! Oric-1 / Atmos family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Oric-1 (1983) — 48 KB RAM.
    Oric1,
    /// Oric Atmos (1984) — 64 KB RAM, improved keyboard.
    Atmos,
}

impl Model {
    pub const ALL: [Self; 2] = [Self::Oric1, Self::Atmos];
    pub const VARIANT_IDS: [&'static str; 2] = ["oric-1", "atmos"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::Oric1 => "oric-1",
            Self::Atmos => "atmos",
        }
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "oric-1" | "oric1" => Some(Self::Oric1),
            "atmos" | "oric-atmos" => Some(Self::Atmos),
            _ => None,
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        // Prefer a model-specific image; preserve the legacy shared filename.
        let candidates: &'static [&'static str] = match self {
            Self::Oric1 => &["oric-1.rom", "oric.rom"],
            Self::Atmos => &["atmos.rom", "oric.rom"],
        };
        vec![
            emu198x_shell::FirmwareSource::required(BIOS_FIRMWARE_ID, candidates)
                .with_env_var("EMU198X_ORIC_ROM"),
        ]
    }

    /// Existing host budget, in 6502 clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        19_968
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Oric1 => "oric-1",
            Self::Atmos => "oric-atmos",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Oric1 => "Oric-1",
            Self::Atmos => "Oric Atmos",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const BIOS_FIRMWARE_ID: &str = "oric-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("oric"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: match model {
            Model::Oric1 => 1983,
            Model::Atmos => 1984,
        },
        summary: "Oric — 6502A + 16 KB BASIC/OS ROM, 48 KB (Oric-1) or 64 KB (Atmos) RAM, VIA + AY-via-VIA, TEXT + HIRES ULA.".into(),
        clock: ClockDesc::new("m6502-cycle", ClockRate::from_hz(1_000_000)),
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "Oric BASIC + OS ROM (16 KB)",
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
        assert!(p.iter().all(|p| p.media_slots.len() == 1));
        assert!(p.iter().all(|p| p.media_slots[0].id == "tape-1"));
    }
}
