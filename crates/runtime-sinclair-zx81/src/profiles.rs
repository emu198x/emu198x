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
    /// The 60 Hz board, as sold in the US. ULA pin 22 grounded.
    ///
    /// The hardware is otherwise identical -- the ULA is the same part and
    /// generates the same line, and the ZX81 has no frame counter for a region
    /// to change. The whole difference is that the ROM reads the strap on port
    /// bit 6 and sets `MARGIN` from it, so it lays out fewer lines per field.
    /// A 60 Hz machine measures 59.93 Hz against 50.65 Hz.
    Zx81Ntsc,
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
            Self::Zx81Ntsc => "sinclair-zx81-ntsc",
        }
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Zx81 => "Sinclair ZX81 (50 Hz)",
            Self::Zx81Ntsc => "Sinclair ZX81 (60 Hz)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Zx81 => Region::Pal,
            Self::Zx81Ntsc => Region::Ntsc,
        }
    }

    /// The board strap the ROM reads on port bit 6.
    #[must_use]
    pub const fn television_standard(self) -> TelevisionStandard {
        match self {
            Self::Zx81 => TelevisionStandard::FiftyHz,
            Self::Zx81Ntsc => TelevisionStandard::SixtyHz,
        }
    }
}

pub const ROM_FIRMWARE_ID: &str = "sinclair-zx81-rom";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Zx81), profile_for(Model::Zx81Ntsc)]
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

    /// Both boards are one machine with one ROM. They must stay joined on
    /// `machine_id` — the registry keys on it, and a second machine_id would
    /// un-join the 60 Hz board from the ZX81's own issues and milestone.
    #[test]
    fn the_two_boards_share_a_machine_but_not_a_profile() {
        let pal = profile_for(Model::Zx81);
        let ntsc = profile_for(Model::Zx81Ntsc);

        assert_eq!(pal.machine_id, ntsc.machine_id);
        assert_ne!(pal.profile_id, ntsc.profile_id);
        assert_eq!(pal.region, Region::Pal);
        assert_eq!(ntsc.region, Region::Ntsc);
        assert_eq!(profiles().len(), 2);
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
            Model::Zx81Ntsc.television_standard(),
            TelevisionStandard::SixtyHz
        );
        assert!(
            Model::Zx81Ntsc
                .television_standard()
                .slow_mode_frame_tstates()
                < Model::Zx81.television_standard().slow_mode_frame_tstates(),
        );
    }
}
