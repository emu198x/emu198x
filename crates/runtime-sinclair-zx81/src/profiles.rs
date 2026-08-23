//! ZX81 family profile catalogue.

use machine_sinclair_zx81::TelevisionStandard;

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// The 50 Hz board. ULA pin 22 left high.
    Zx81,
    /// A ZX81 with the 16 KB RAM pack on the back.
    ///
    /// The pack is what most software assumes: 1,204 of the 1,206 images in
    /// the TOSEC `[P]` set need more than the 1 KB the bare machine has.
    Zx81Ram16k,
    /// Timex/Sinclair TS1000 -- the US machine: 2 KB and 60 Hz.
    ///
    /// *"`ZX81` and `TS1000` are the same machine. The differences are RAM
    /// size (1 K vs 2 K) and television standard."*
    /// (`reference/by-system/sinclair-zx81/zx81-hardware-reference.md`)
    ///
    /// The 60 Hz half is one strap: ULA pin 22 grounded, which the ROM reads
    /// on port bit 6 and turns into a shorter field via `MARGIN`. The ULA is
    /// the same part and the ZX81 has no frame counter for a region to
    /// change, so a 60 Hz machine simply measures 59.93 Hz against 50.65.
    Ts1000,
}

impl Model {
    #[must_use]
    /// Both boards are the same machine, so they share a `machine_id`. The
    /// registry joins on this, and it is deliberately not the profile id.
    pub const fn model_id(self) -> &'static str {
        "sinclair-zx81"
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Zx81 => "sinclair-zx81",
            Self::Zx81Ram16k => "sinclair-zx81-16k",
            Self::Ts1000 => "timex-ts1000",
        }
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Zx81 => "Sinclair ZX81 (1 KB)",
            Self::Zx81Ram16k => "Sinclair ZX81 (16 KB RAM pack)",
            Self::Ts1000 => "Timex/Sinclair TS1000 (2 KB, 60 Hz)",
        }
    }

    /// RAM fitted, in bytes.
    ///
    /// Decoded internally as A0-A9 for 1 KB and A0-A10 for 2 KB; the pack is
    /// external and decoded A0-A13 (hardware reference, memory map).
    #[must_use]
    pub const fn ram_bytes(self) -> usize {
        match self {
            Self::Zx81 => 1024,
            Self::Zx81Ram16k => 16 * 1024,
            Self::Ts1000 => 2048,
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Zx81 | Self::Zx81Ram16k => Region::Pal,
            Self::Ts1000 => Region::Ntsc,
        }
    }

    /// The board strap the ROM reads on port bit 6.
    #[must_use]
    pub const fn television_standard(self) -> TelevisionStandard {
        match self {
            Self::Zx81 | Self::Zx81Ram16k => TelevisionStandard::FiftyHz,
            Self::Ts1000 => TelevisionStandard::SixtyHz,
        }
    }

    /// Every board, for the variant menu and the profile catalogue.
    pub const ALL: [Self; 3] = [Self::Zx81, Self::Zx81Ram16k, Self::Ts1000];
}

pub const ROM_FIRMWARE_ID: &str = "sinclair-zx81-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.iter().copied().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sinclair-zx81"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Spectrum,
        region: model.region(),
        release_year: 1981,
        summary: "Sinclair ZX81 / Timex TS1000 — Z80A + ULA, 1 KB internal RAM (16 KB expansion), 8 KB monitor ROM.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_250_000)),
        firmware: vec![FirmwareRequirement::new(
            ROM_FIRMWARE_ID,
            "ZX81 monitor ROM (8 KB)",
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
        let p = profile_for(Model::Zx81);
        assert_eq!(p.firmware.len(), 1);
        assert_eq!(p.firmware[0].id.as_ref(), ROM_FIRMWARE_ID);
    }

    /// The boards are one machine with one ROM. They must stay joined on
    /// `machine_id` — the registry keys on it, and a second machine_id would
    /// un-join the other boards from the ZX81's own issues and milestone.
    #[test]
    fn the_boards_share_a_machine_but_not_a_profile() {
        let ids: Vec<_> = Model::ALL.iter().map(|m| profile_for(*m)).collect();
        for board in &ids {
            assert_eq!(board.machine_id, ids[0].machine_id);
        }
        let mut profile_ids: Vec<String> = Model::ALL
            .iter()
            .map(|m| m.profile_id().to_owned())
            .collect();
        profile_ids.sort();
        let count = profile_ids.len();
        profile_ids.dedup();
        assert_eq!(profile_ids.len(), count, "profile ids must be distinct");
        assert_eq!(profiles().len(), count);
    }

    /// The strap is the region, and the frame budget follows it. The 60 Hz
    /// budget is far below the 50 Hz one, which is why a shared constant
    /// cannot serve both.
    #[test]
    fn the_strap_and_its_budget_follow_the_model() {
        assert_eq!(
            Model::Zx81.television_standard(),
            TelevisionStandard::FiftyHz
        );
        assert_eq!(
            Model::Ts1000.television_standard(),
            TelevisionStandard::SixtyHz
        );
        assert!(
            Model::Ts1000
                .television_standard()
                .slow_mode_frame_tstates()
                < Model::Zx81.television_standard().slow_mode_frame_tstates(),
        );
    }

    /// The reference puts the whole ZX81/TS1000 difference in two things:
    /// *"RAM size (1 K vs 2 K) and television standard"*. Both, and nothing
    /// else, separate these two profiles.
    #[test]
    fn the_ts1000_differs_from_the_zx81_in_ram_and_standard_only() {
        let zx81 = profile_for(Model::Zx81);
        let ts1000 = profile_for(Model::Ts1000);

        assert_eq!(Model::Zx81.ram_bytes(), 1024);
        assert_eq!(Model::Ts1000.ram_bytes(), 2048);
        assert_ne!(zx81.region, ts1000.region);
        assert_eq!(zx81.clock, ts1000.clock);
        assert_eq!(zx81.firmware.len(), ts1000.firmware.len());
    }

    /// The RAM pack is a 50 Hz ZX81 with more memory, nothing else.
    #[test]
    fn the_ram_pack_changes_only_the_memory() {
        assert_eq!(Model::Zx81Ram16k.ram_bytes(), 16 * 1024);
        assert_eq!(
            Model::Zx81Ram16k.television_standard(),
            Model::Zx81.television_standard()
        );
        assert_eq!(
            profile_for(Model::Zx81Ram16k).region,
            profile_for(Model::Zx81).region
        );
    }
}
