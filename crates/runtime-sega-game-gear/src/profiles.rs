//! Sega Game Gear profile catalogue.

use crate::SmsRuntime;
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, WritebackPolicy, known_capability,
};
use runtime_sega_master_system_class::{SmsModel, SmsVariant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Sega Game Gear. The handheld shipped in one hardware configuration;
    /// unlike its console sibling there is no PAL variant to select, because
    /// an LCD has no broadcast standard to match.
    GameGear,
}

impl Model {
    pub const ALL: [Self; 1] = [Self::GameGear];
    pub const VARIANT_IDS: [&'static str; 1] = ["game-gear"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::GameGear => "game-gear",
        }
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "gg" => Some(Self::GameGear),
            _ => Self::ALL
                .into_iter()
                .find(|model| model.variant_id() == id || model.profile_id() == id),
        }
    }

    /// Existing native host frame budget, in CPU clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        228 * 262
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::GameGear => "sega-game-gear",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::GameGear => "Sega Game Gear",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::GameGear => Region::Ntsc,
        }
    }

    #[must_use]
    pub const fn variant(self) -> SmsVariant {
        match self {
            Self::GameGear => SmsVariant::GameGear,
        }
    }
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sega-game-gear"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        // 1990, not the Master System's 1985. The shared profile this crate
        // was extracted from gave every model the console's year.
        release_year: 1990,
        summary: "Sega Game Gear — Z80A + Sega VDP (160×144 LCD crop) + SN76489 stereo, 8 KB RAM."
            .into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware: vec![],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            true,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("controller-input"),
            known_capability("scripted-input"),
        ]),
    }
}

/// A runtime with no cartridge inserted.
///
/// Convenience constructor for this system's concrete runtime alias.
#[must_use]
pub fn blank(model: Model) -> SmsRuntime {
    SmsRuntime::blank(model)
}

/// A runtime with `cart_rom` inserted.
#[must_use]
pub fn with_cartridge(model: Model, cart_rom: Vec<u8>) -> SmsRuntime {
    SmsRuntime::new(model, cart_rom)
}

impl SmsModel for Model {
    const VARIANT_IDS: &'static [&'static str] = &Self::VARIANT_IDS;
    fn from_variant_id(id: &str) -> Option<Self> {
        Self::from_variant_id(id)
    }
    fn variant_id(self) -> &'static str {
        self.variant_id()
    }
    fn model_id(self) -> &'static str {
        self.model_id()
    }
    fn profile(self) -> MachineProfile {
        profile_for(self)
    }
    fn variant(self) -> SmsVariant {
        self.variant()
    }
    fn frame_ticks(self) -> u64 {
        self.frame_ticks()
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

    /// The whole point of #998: one crate, one machine, and its id written
    /// as a literal where a scan of the workspace can see it. A named
    /// constant would read better and defeat the scan just as thoroughly as
    /// the variable this crate was split to remove.
    #[test]
    fn every_profile_declares_the_same_single_machine() {
        for profile in profiles() {
            assert_eq!(profile.machine_id.as_str(), "sega-game-gear");
        }
    }

    /// The Game Gear drives the cropped-LCD variant, not a console one.
    #[test]
    fn the_runtime_drives_the_game_gear_variant() {
        assert_eq!(Model::GameGear.variant(), SmsVariant::GameGear);
    }
}
