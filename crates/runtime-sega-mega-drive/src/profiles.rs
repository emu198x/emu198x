//! Sega Mega Drive / Genesis profile catalogue.

use crate::MegaDriveRuntime;
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, WritebackPolicy, known_capability,
};
use machine_sega_mega_drive::{
    MCLKS_PER_LINE, NTSC_LINES, NTSC_MASTER_HZ, PAL_LINES, PAL_MASTER_HZ,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// The Japanese Mega Drive: domestic, NTSC.
    MegaDriveJapan,
    /// The Genesis, sold in the Americas: overseas, NTSC.
    Genesis,
    /// The European Mega Drive: overseas, PAL.
    MegaDriveEurope,
}

impl Model {
    pub const ALL: [Self; 3] = [Self::MegaDriveJapan, Self::Genesis, Self::MegaDriveEurope];
    pub const VARIANT_IDS: [&'static str; 3] = ["mega-drive-japan", "genesis", "mega-drive-europe"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::MegaDriveJapan => "mega-drive-japan",
            Self::Genesis => "genesis",
            Self::MegaDriveEurope => "mega-drive-europe",
        }
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|model| model.variant_id() == id || model.profile_id() == id)
    }

    /// One frame in master clocks: 262 lines of 3420 on NTSC, 313 on PAL.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::MegaDriveEurope => PAL_LINES * MCLKS_PER_LINE,
            Self::MegaDriveJapan | Self::Genesis => NTSC_LINES * MCLKS_PER_LINE,
        }
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::MegaDriveJapan => "sega-mega-drive-japan",
            Self::Genesis => "sega-genesis",
            Self::MegaDriveEurope => "sega-mega-drive-europe",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::MegaDriveJapan => "Sega Mega Drive (Japan, NTSC)",
            Self::Genesis => "Sega Genesis (NTSC)",
            Self::MegaDriveEurope => "Sega Mega Drive (PAL)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::MegaDriveEurope => Region::Pal,
            Self::MegaDriveJapan | Self::Genesis => Region::Ntsc,
        }
    }

    /// The region the console's version register reports.
    #[must_use]
    pub const fn machine_region(self) -> machine_sega_mega_drive::Region {
        match self {
            Self::MegaDriveJapan => machine_sega_mega_drive::Region::Japan,
            Self::Genesis => machine_sega_mega_drive::Region::Americas,
            Self::MegaDriveEurope => machine_sega_mega_drive::Region::Europe,
        }
    }

    /// The master clock on this model.
    #[must_use]
    pub const fn master_hz(self) -> u64 {
        match self {
            Self::MegaDriveEurope => PAL_MASTER_HZ,
            Self::MegaDriveJapan | Self::Genesis => NTSC_MASTER_HZ,
        }
    }

    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::MegaDriveJapan => 1988,
            Self::Genesis => 1989,
            Self::MegaDriveEurope => 1990,
        }
    }

    /// What distinguishes this model from its siblings: the version
    /// register's region bits, which games read to lock themselves to a
    /// market, and the television standard.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::MegaDriveJapan => {
                "Japanese Sega Mega Drive — 68000 + Z80 + 315-5313 VDP + YM2612 + SN76489, 64 KB RAM, domestic NTSC."
            }
            Self::Genesis => {
                "Sega Genesis — 68000 + Z80 + 315-5313 VDP + YM2612 + SN76489, 64 KB RAM, overseas NTSC."
            }
            Self::MegaDriveEurope => {
                "European Sega Mega Drive — 68000 + Z80 + 315-5313 VDP + YM2612 + SN76489, 64 KB RAM, overseas PAL."
            }
        }
    }
}

/// A runtime with no cartridge inserted.
#[must_use]
pub fn blank(model: Model) -> MegaDriveRuntime {
    MegaDriveRuntime::blank(model)
}

/// A runtime with `cart_rom` inserted.
#[must_use]
pub fn with_cartridge(model: Model, cart_rom: Vec<u8>) -> MegaDriveRuntime {
    MegaDriveRuntime::new(model, cart_rom)
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sega-mega-drive"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: model.release_year(),
        summary: model.summary().into(),
        clock: ClockDesc::new("master-cycle", ClockRate::from_hz(model.master_hz())),
        firmware: vec![],
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
    fn profile_ids_are_unique() {
        let profiles = profiles();
        let mut ids: Vec<&str> = profiles.iter().map(|p| p.profile_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn variant_ids_round_trip() {
        for (model, id) in Model::ALL.into_iter().zip(Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(Model::from_variant_id(id), Some(model));
            assert_eq!(Model::from_variant_id(model.profile_id()), Some(model));
        }
        assert_eq!(Model::from_variant_id("sms-ntsc"), None);
    }

    #[test]
    fn only_the_european_model_is_pal() {
        for model in Model::ALL {
            let pal = model == Model::MegaDriveEurope;
            assert_eq!(profile_for(model).region == Region::Pal, pal);
            assert_eq!(model.machine_region().pal(), pal);
        }
    }

    /// The clock is the master clock, the unit `MachineTime` counts, so a
    /// frame's ticks over the rate is the frame's length.
    #[test]
    fn each_model_runs_on_its_own_master_clock() {
        let ntsc = profile_for(Model::Genesis);
        let pal = profile_for(Model::MegaDriveEurope);
        assert_eq!(ntsc.clock.rate.numerator_hz, 53_693_175);
        assert_eq!(pal.clock.rate.numerator_hz, 53_203_424);
        assert_eq!(Model::Genesis.frame_ticks(), 262 * 3420);
        assert_eq!(Model::MegaDriveEurope.frame_ticks(), 313 * 3420);
        let hz = |model: Model| model.master_hz() as f64 / model.frame_ticks() as f64;
        assert!((hz(Model::Genesis) - 59.92).abs() < 0.01);
        assert!((hz(Model::MegaDriveEurope) - 49.70).abs() < 0.01);
    }

    /// One crate, one machine, its id a literal a scan of the workspace can
    /// see (#998).
    #[test]
    fn every_profile_declares_the_same_single_machine() {
        for profile in profiles() {
            assert_eq!(profile.machine_id.as_str(), "sega-mega-drive");
        }
    }
}
