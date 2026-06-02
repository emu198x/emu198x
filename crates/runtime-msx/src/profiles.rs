//! MSX1 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

/// Supported MSX1 models in the initial parity bootstrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// MSX1 NTSC (60 Hz / 262 lines).
    Msx1Ntsc,
    /// MSX1 PAL (50 Hz / 313 lines).
    Msx1Pal,
}

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Msx1Ntsc => "microsoft-msx1-ntsc",
            Self::Msx1Pal => "microsoft-msx1-pal",
        }
    }

    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    /// User-facing display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Msx1Ntsc => "Microsoft MSX1 (NTSC)",
            Self::Msx1Pal => "Microsoft MSX1 (PAL)",
        }
    }

    /// Region.
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Msx1Ntsc => Region::Ntsc,
            Self::Msx1Pal => Region::Pal,
        }
    }
}

/// Stable BIOS firmware identifier (shared by all MSX1 regional
/// variants — the 32 KB main-ROM dump is the same image).
pub const BIOS_FIRMWARE_ID: &str = "msx1-bios";

/// Returns the initial MSX1 family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Msx1Ntsc), profile_for(Model::Msx1Pal)]
}

/// Returns the profile metadata for one MSX1 model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("microsoft-msx1"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Msx,
        region: model.region(),
        support_tier: SupportTier::Boots,
        release_year: 1983,
        summary: "MSX1 — Z80A + TMS9918A + AY-3-8910 + 8255 PPI, 64 KB RAM, BIOS-driven boot, MegaROM mapper (plain/Konami/KonamiSCC/ASCII8/ASCII16) cartridge support.".into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware: vec![FirmwareRequirement::new(
            BIOS_FIRMWARE_ID,
            "MSX1 main ROM (BIOS + MSX-BASIC 1.0, 32 KB)",
            false,
        )],
        media_slots: vec![
            MediaSlot::new(
                "cartridge-1",
                "Cartridge Slot 1",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
            MediaSlot::new(
                "cartridge-2",
                "Cartridge Slot 2",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
        ],
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
    fn profile_ids_are_unique() {
        let profiles = profiles();
        let mut ids: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn ntsc_profile_declares_bios_firmware() {
        let profile = profile_for(Model::Msx1Ntsc);
        assert_eq!(profile.region, Region::Ntsc);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(profile.firmware[0].id.as_ref(), BIOS_FIRMWARE_ID);
        assert_eq!(profile.media_slots.len(), 2);
    }

    #[test]
    fn pal_profile_uses_pal_region() {
        let profile = profile_for(Model::Msx1Pal);
        assert_eq!(profile.region, Region::Pal);
    }
}
