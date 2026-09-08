//! Acorn Atom family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Base 2.5 KB-RAM Atom.
    AtomBase,
    /// Fully expanded 32 KB-RAM Atom.
    AtomFull,
}

impl Model {
    pub const ALL: [Self; 2] = [Self::AtomBase, Self::AtomFull];
    pub const VARIANT_IDS: [&'static str; 2] = ["acorn-atom-base", "acorn-atom-full"];

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
            emu198x_shell::FirmwareSource::required(BIOS_FIRMWARE_ID, &["atom.rom"])
                .with_env_var("EMU198X_ACORN_ATOM_ROM"),
        ]
    }

    /// Existing host frame budget in native machine ticks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        20_000
    }

    /// Legacy `--ram-kb` selects presets: 12 or more requests the full expansion.
    #[must_use]
    pub const fn from_ram_kb(ram_kb: usize) -> Self {
        if ram_kb >= 12 {
            Self::AtomFull
        } else {
            Self::AtomBase
        }
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::AtomBase => "acorn-atom-base",
            Self::AtomFull => "acorn-atom-full",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::AtomBase => "Acorn Atom (2.5 KB)",
            Self::AtomFull => "Acorn Atom (32 KB)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }

    #[must_use]
    pub const fn ram_bytes(self) -> usize {
        match self {
            Self::AtomBase => 2560,
            // Fully-expanded Atom: contiguous low RAM $0000-$7FFF (video is at
            // $8000), enough for `.atm` programs that load into the text space at
            // $2800+.
            Self::AtomFull => 32 * 1024,
        }
    }
}

pub const BIOS_FIRMWARE_ID: &str = "acorn-atom-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("acorn-atom"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1980,
        summary: "Acorn Atom — 6502 + 24 KB combined ROM (BASIC1 + FP + BASIC2 + OS), 2.5 KB / 32 KB RAM, VDG 6847 display.".into(),
        clock: ClockDesc::new("m6502-cycle", ClockRate::from_hz(1_000_000)),
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "Acorn Atom combined ROM (24 KB)",
            false,
        )],
        media_slots: vec![
            MediaSlot::new(
                "tape-1",
                "Cassette Tape",
                MediaKind::Tape,
                false,
                WritebackPolicy::SidecarOnly,
            ),
            MediaSlot::new(
                "program-1",
                "Program (.atm)",
                MediaKind::Program,
                false,
                WritebackPolicy::SidecarOnly,
            ),
            MediaSlot::new(
                "rom-pack-1",
                "Utility ROM ($A000)",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::SidecarOnly,
            ),
        ],
        capabilities: CapabilitySet::with_all([
            known_capability("variant-switch"),
            known_capability("keyboard-input"),
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
    }
}
