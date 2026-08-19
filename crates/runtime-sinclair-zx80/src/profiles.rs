//! ZX80 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// As sold: 1 KB on the board.
    Zx80,
    /// With Sinclair's 16 KB RAM pack on the edge connector.
    ///
    /// Almost all ZX80 software wants this. 1 KB leaves room for a display
    /// file and very little else, and the RAM pack is what the type-in
    /// listings of the period assume.
    Zx80RamPack,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        "sinclair-zx80"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            // Unchanged, and deliberately not renamed to `-1k`: this id is
            // already in the registry and in staged scripts.
            Self::Zx80 => "sinclair-zx80",
            Self::Zx80RamPack => "sinclair-zx80-16k",
        }
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Zx80 => "Sinclair ZX80",
            Self::Zx80RamPack => "Sinclair ZX80 (16 KB RAM pack)",
        }
    }
    /// RAM the profile boots with. The machine accepts anything up to 16 KB;
    /// this is what selecting the profile means.
    #[must_use]
    pub const fn ram_bytes(self) -> usize {
        match self {
            Self::Zx80 => 1024,
            Self::Zx80RamPack => 16 * 1024,
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        Region::Pal
    }
}

pub const ROM_FIRMWARE_ID: &str = "sinclair-zx80-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Zx80), profile_for(Model::Zx80RamPack)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sinclair-zx80"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Spectrum,
        region: model.region(),
        release_year: 1980,
        summary: match model {
            Model::Zx80 => "Sinclair ZX80 — Z80A + discrete keyboard / display logic, 1 KB internal RAM, 4 KB monitor ROM.",
            Model::Zx80RamPack => "Sinclair ZX80 with the 16 KB RAM pack — Z80A + discrete keyboard / display logic, 4 KB monitor ROM.",
        }
        .into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_250_000)),
        firmware: vec![FirmwareRequirement::new(
            ROM_FIRMWARE_ID,
            "ZX80 monitor ROM (4 KB)",
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
        for model in [Model::Zx80, Model::Zx80RamPack] {
            let p = profile_for(model);
            assert_eq!(p.firmware.len(), 1);
            assert_eq!(p.firmware[0].id.as_ref(), ROM_FIRMWARE_ID);
        }
    }

    /// Both profiles are the same machine, so they share a `machine_id` and
    /// differ only where the RAM pack makes them differ.
    #[test]
    fn ram_pack_is_a_second_profile_of_the_same_machine() {
        let stock = profile_for(Model::Zx80);
        let packed = profile_for(Model::Zx80RamPack);

        assert_eq!(stock.machine_id, packed.machine_id);
        assert_ne!(stock.profile_id, packed.profile_id);
        assert_eq!(profiles().len(), 2);

        // The existing id is load-bearing: it is in the registry and in
        // scripts people have already written.
        assert_eq!(stock.profile_id, ProfileId::from("sinclair-zx80"));
        assert_eq!(Model::Zx80.ram_bytes(), 1024);
        assert_eq!(Model::Zx80RamPack.ram_bytes(), 16 * 1024);
    }
}
