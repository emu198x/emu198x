//! Jupiter Ace family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    ProfileId, Region, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// 3 KB stock Jupiter Ace.
    Ace3k,
    /// 16 KB-expanded Jupiter Ace.
    Ace16k,
    /// 48 KB-expanded Jupiter Ace.
    Ace48k,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Ace3k => "jupiter-ace-3k",
            Self::Ace16k => "jupiter-ace-16k",
            Self::Ace48k => "jupiter-ace-48k",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Ace3k => "Jupiter Ace (3 KB)",
            Self::Ace16k => "Jupiter Ace (16 KB)",
            Self::Ace48k => "Jupiter Ace (48 KB)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }

    #[must_use]
    pub const fn ram_kb(self) -> usize {
        match self {
            Self::Ace3k => 3,
            Self::Ace16k => 16,
            Self::Ace48k => 48,
        }
    }

    /// RAM fitted at `$4000+`, excluding the three on-board 1 KB banks.
    #[must_use]
    pub const fn expansion_ram_kb(self) -> usize {
        match self {
            Self::Ace3k => 0,
            Self::Ace16k => 16,
            Self::Ace48k => 48,
        }
    }
}

pub const BIOS_FIRMWARE_ID: &str = "jupiter-ace-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Ace3k),
        profile_for(Model::Ace16k),
        profile_for(Model::Ace48k),
    ]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("jupiter-ace"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1982,
        summary: "Jupiter Ace — Z80A + 8 KB Forth ROM, 1 KB character RAM, optional 16/48 KB RAM expansion.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_250_000)),
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "Jupiter Ace Forth ROM (8 KB)",
            false,
        )],
        media_slots: Vec::new(),
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
    fn three_profiles() {
        let p = profiles();
        assert_eq!(p.len(), 3);
        assert!(p.iter().all(|p| p.firmware.len() == 1));
    }
}
