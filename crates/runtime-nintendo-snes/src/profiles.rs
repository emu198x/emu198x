//! Super NES profile catalogue.

use crate::SnesRuntime;
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};
use machine_nintendo_snes::{NTSC_MASTER_HZ, PAL_MASTER_HZ, ipl};

/// Lines in an NTSC and a PAL frame, and master clocks in a line.
const NTSC_LINES: u64 = 262;
const PAL_LINES: u64 = 312;
const LINE_CLOCKS: u64 = 1364;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// The Super NES sold in North America: NTSC.
    SnesNtsc,
    /// The Super Nintendo sold in Europe and Australia: PAL.
    SnesPal,
    /// The Super Famicom: Japanese, NTSC.
    SuperFamicom,
}

impl Model {
    pub const ALL: [Self; 3] = [Self::SnesNtsc, Self::SnesPal, Self::SuperFamicom];
    pub const VARIANT_IDS: [&'static str; 3] = ["snes-ntsc", "snes-pal", "super-famicom"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::SnesNtsc => "snes-ntsc",
            Self::SnesPal => "snes-pal",
            Self::SuperFamicom => "super-famicom",
        }
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|model| model.variant_id() == id || model.profile_id() == id)
    }

    /// One frame in master clocks: 262 lines of 1364 on NTSC, 312 on PAL.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::SnesPal => PAL_LINES * LINE_CLOCKS,
            Self::SnesNtsc | Self::SuperFamicom => NTSC_LINES * LINE_CLOCKS,
        }
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::SnesNtsc => "nintendo-snes-ntsc",
            Self::SnesPal => "nintendo-snes-pal",
            Self::SuperFamicom => "nintendo-super-famicom",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::SnesNtsc => "Super NES (NTSC)",
            Self::SnesPal => "Super Nintendo (PAL)",
            Self::SuperFamicom => "Super Famicom",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::SnesPal => Region::Pal,
            Self::SnesNtsc | Self::SuperFamicom => Region::Ntsc,
        }
    }

    #[must_use]
    pub const fn machine_region(self) -> machine_nintendo_snes::Region {
        match self {
            Self::SnesPal => machine_nintendo_snes::Region::Pal,
            Self::SnesNtsc | Self::SuperFamicom => machine_nintendo_snes::Region::Ntsc,
        }
    }

    /// The model a cartridge's header asks for: PAL for a PAL destination
    /// code, the Super Famicom for Japan's, the NTSC Super NES otherwise.
    #[must_use]
    pub const fn for_destination(destination: u8, pal: bool) -> Self {
        if pal {
            Self::SnesPal
        } else if destination == 0 {
            Self::SuperFamicom
        } else {
            Self::SnesNtsc
        }
    }

    #[must_use]
    pub const fn master_hz(self) -> u64 {
        match self {
            Self::SnesPal => PAL_MASTER_HZ,
            Self::SnesNtsc | Self::SuperFamicom => NTSC_MASTER_HZ,
        }
    }

    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::SuperFamicom => 1990,
            Self::SnesNtsc => 1991,
            Self::SnesPal => 1992,
        }
    }

    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::SnesNtsc => {
                "Super NES — 5A22 (65C816 + DMA/HDMA) + S-PPU1/S-PPU2 + S-SMP/S-DSP sound, 128 KB RAM, NTSC."
            }
            Self::SnesPal => {
                "Super Nintendo — 5A22 (65C816 + DMA/HDMA) + S-PPU1/S-PPU2 + S-SMP/S-DSP sound, 128 KB RAM, PAL."
            }
            Self::SuperFamicom => {
                "Super Famicom — 5A22 (65C816 + DMA/HDMA) + S-PPU1/S-PPU2 + S-SMP/S-DSP sound, 128 KB RAM, Japanese NTSC."
            }
        }
    }

    /// The optional IPL ROM, by the names emulators keep it under.
    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        vec![
            emu198x_shell::FirmwareSource::optional(
                ipl::FIRMWARE_ID,
                &["ipl.rom", "spc700.rom", "snes-ipl.rom"],
            )
            .with_env_var("EMU198X_SNES_IPL"),
        ]
    }
}

/// A runtime with no cartridge inserted.
#[must_use]
pub fn blank(model: Model) -> SnesRuntime {
    SnesRuntime::blank(model)
}

/// A runtime with `image` inserted.
#[must_use]
pub fn with_cartridge(model: Model, image: Vec<u8>) -> SnesRuntime {
    SnesRuntime::new(model, image)
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("nintendo-snes"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: model.release_year(),
        summary: model.summary().into(),
        clock: ClockDesc::new("master-cycle", ClockRate::from_hz(model.master_hz())),
        firmware: vec![FirmwareRequirement::new(
            ipl::FIRMWARE_ID,
            "S-SMP IPL ROM (optional; a clean-room boot program is built in)",
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
            known_capability("variant-switch"),
            known_capability("controller-input"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_ids_round_trip_and_are_unique() {
        for (model, id) in Model::ALL.into_iter().zip(Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(Model::from_variant_id(id), Some(model));
            assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        }
        assert_eq!(Model::from_variant_id("genesis"), None);
        let mut ids: Vec<_> = profiles().iter().map(|p| p.profile_id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn frames_run_at_about_60_and_50_hz() {
        let hz = |model: Model| model.master_hz() as f64 / model.frame_ticks() as f64;
        assert!((hz(Model::SnesNtsc) - 60.1).abs() < 0.05);
        assert!((hz(Model::SnesPal) - 50.0).abs() < 0.05);
        assert_eq!(profile_for(Model::SnesPal).region, Region::Pal);
    }

    #[test]
    fn the_header_picks_the_model() {
        assert_eq!(Model::for_destination(0, false), Model::SuperFamicom);
        assert_eq!(Model::for_destination(1, false), Model::SnesNtsc);
        assert_eq!(Model::for_destination(2, true), Model::SnesPal);
    }

    #[test]
    fn the_ipl_rom_is_optional_firmware() {
        for profile in profiles() {
            assert_eq!(profile.machine_id.as_str(), "nintendo-snes");
            assert!(profile.firmware.iter().all(|f| f.optional));
        }
        assert!(Model::SnesNtsc.firmware_sources()[0].optional);
    }
}
