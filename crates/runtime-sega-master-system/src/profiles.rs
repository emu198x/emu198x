//! Sega Master System profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, WritebackPolicy, known_capability,
};
use runtime_sega_master_system_class::{SmsRuntime, SmsVariant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Master System NTSC.
    SmsNtsc,
    /// Master System PAL.
    SmsPal,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::SmsNtsc => "sega-master-system-ntsc",
            Self::SmsPal => "sega-master-system-pal",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::SmsNtsc => "Sega Master System (NTSC)",
            Self::SmsPal => "Sega Master System (PAL)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::SmsPal => Region::Pal,
            Self::SmsNtsc => Region::Ntsc,
        }
    }

    #[must_use]
    pub const fn variant(self) -> SmsVariant {
        match self {
            Self::SmsNtsc => SmsVariant::SmsNtsc,
            Self::SmsPal => SmsVariant::SmsPal,
        }
    }
}

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::SmsNtsc), profile_for(Model::SmsPal)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sega-master-system"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1985,
        summary:
            "Sega Master System — Z80A + Sega VDP + SN76489, 8 KB RAM, Sega mapper cartridge boot."
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
/// A free function rather than an inherent constructor: `SmsRuntime` belongs
/// to the class crate, so this crate cannot hang an `impl` off it.
#[must_use]
pub fn blank(model: Model) -> SmsRuntime {
    SmsRuntime::blank(profile_for(model), model.variant(), model.model_id())
}

/// A runtime with `cart_rom` inserted.
#[must_use]
pub fn with_cartridge(model: Model, cart_rom: Vec<u8>) -> SmsRuntime {
    SmsRuntime::new(
        profile_for(model),
        model.variant(),
        model.model_id(),
        cart_rom,
    )
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
    fn pal_profile_uses_pal_region() {
        let p = profile_for(Model::SmsPal);
        assert_eq!(p.region, Region::Pal);
    }

    /// The whole point of #998: one crate, one machine, and its id written
    /// as a literal where a scan of the workspace can see it. A named
    /// constant would read better and defeat the scan just as thoroughly as
    /// the variable this crate was split to remove.
    #[test]
    fn every_profile_declares_the_same_single_machine() {
        for profile in profiles() {
            assert_eq!(profile.machine_id.as_str(), "sega-master-system");
        }
    }
}
