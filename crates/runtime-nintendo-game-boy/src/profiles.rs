//! Game Boy family machine catalogue.
//!
//! One model today (`Dmg`); the structure is in place so the second
//! variant (`Cgb`) can land alongside the family-driver lift per
//! [within-family-layering](../../../wiki/decisions/within-family-layering.md).

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};
use machine_nintendo_game_boy::BootProfile;

/// Supported Game Boy models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Original Game Boy with the DMG0 boot-ROM exit profile.
    Dmg0,
    /// Original 1989 Game Boy (DMG-01). Grey-LCD, 4-shade palette,
    /// 4.194304 MHz master clock.
    Dmg,
    /// Game Boy Pocket skipped-boot profile.
    Mgb,
    /// Super Game Boy skipped-boot profile.
    Sgb,
    /// Super Game Boy 2 skipped-boot profile.
    Sgb2,
}

impl Model {
    /// Stable model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Dmg0 => "nintendo-game-boy-dmg0",
            Self::Dmg => "nintendo-game-boy-dmg",
            Self::Mgb => "nintendo-game-boy-mgb",
            Self::Sgb => "nintendo-super-game-boy",
            Self::Sgb2 => "nintendo-super-game-boy-2",
        }
    }

    /// Stable profile identifier.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Dmg0 => "nintendo-game-boy-dmg0",
            Self::Dmg => "nintendo-game-boy-dmg",
            Self::Mgb => "nintendo-game-boy-mgb",
            Self::Sgb => "nintendo-super-game-boy",
            Self::Sgb2 => "nintendo-super-game-boy-2",
        }
    }

    /// User-facing display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Dmg0 => "Nintendo Game Boy (DMG0 boot profile)",
            Self::Dmg => "Nintendo Game Boy (DMG)",
            Self::Mgb => "Nintendo Game Boy Pocket (MGB)",
            Self::Sgb => "Nintendo Super Game Boy",
            Self::Sgb2 => "Nintendo Super Game Boy 2",
        }
    }

    /// Year of original release.
    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::Dmg0 | Self::Dmg => 1989,
            Self::Mgb => 1996,
            Self::Sgb => 1994,
            Self::Sgb2 => 1998,
        }
    }

    /// Skipped-boot ROM exit profile used by the machine layer.
    #[must_use]
    pub const fn boot_profile(self) -> BootProfile {
        match self {
            Self::Dmg0 => BootProfile::Dmg0,
            Self::Dmg => BootProfile::DmgAbc,
            Self::Mgb => BootProfile::Mgb,
            Self::Sgb => BootProfile::Sgb,
            Self::Sgb2 => BootProfile::Sgb2,
        }
    }
}

/// Returns the full Game Boy family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Dmg0),
        profile_for(Model::Dmg),
        profile_for(Model::Mgb),
        profile_for(Model::Sgb),
        profile_for(Model::Sgb2),
    ]
}

/// Returns the metadata for one Game Boy model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("nintendo-game-boy"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::GameBoy,
        // The DMG isn't really PAL/NTSC — the LCD is its own
        // refresh standard. `Other` is the closest fit.
        region: Region::Other,
        support_tier: SupportTier::Boots,
        release_year: model.release_year(),
        summary: format!(
            "{}: SM83 CPU at 4.194 MHz, 160×144 4-shade LCD, 4-channel audio. Current runtime enters cartridges from the selected post-boot-ROM register state; boot ROM execution will land separately.",
            model.display_name()
        )
        .into(),
        clock: ClockDesc::new(
            "master-cycle",
            ClockRate::from_hz(common_nintendo_game_boy::DMG_MASTER_HZ.into()),
        ),
        firmware: vec![],
        media_slots: vec![MediaSlot::new(
            "cartridge",
            "Cartridge",
            MediaKind::Cartridge,
            false,
            WritebackPolicy::InMemoryOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("keyboard-matrix"),
            known_capability("scripted-input"),
            known_capability("snapshot-export"),
            known_capability("snapshot-import"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmg_profile_is_honest_about_current_boot_scope() {
        let profile = profile_for(Model::Dmg);
        assert!(profile.firmware.is_empty());
        assert_eq!(profile.media_slots.len(), 1);
        assert_eq!(profile.media_slots[0].id.as_ref(), "cartridge");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Cartridge);
    }
}
