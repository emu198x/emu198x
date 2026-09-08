//! VIC-20 family profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// VIC-20 NTSC (6560 VIC).
    Vic20Ntsc,
    /// VIC-20 PAL / VC-20 (6561 VIC).
    Vic20Pal,
}

impl Model {
    /// All existing regional presets.
    pub const ALL: [Self; 2] = [Self::Vic20Ntsc, Self::Vic20Pal];
    /// Stable selection ids shared by launchers, scripts and menus.
    pub const VARIANT_IDS: [&'static str; 2] = ["commodore-vic-20-ntsc", "commodore-vic-20-pal"];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "commodore-vic-20-ntsc" | "vic20-ntsc" | "ntsc" => Some(Self::Vic20Ntsc),
            "commodore-vic-20-pal" | "vic20-pal" | "pal" => Some(Self::Vic20Pal),
            _ => None,
        }
    }

    /// Existing host frame budget in CPU cycles.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Vic20Ntsc => 65 * 261,
            Self::Vic20Pal => 71 * 312,
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        use emu198x_shell::FirmwareSource;
        vec![
            FirmwareSource::required(KERNAL_FIRMWARE_ID, &["kernal.rom"])
                .with_env_var("EMU198X_VIC20_KERNAL"),
            FirmwareSource::required(BASIC_FIRMWARE_ID, &["basic.rom"])
                .with_env_var("EMU198X_VIC20_BASIC"),
            FirmwareSource::required(CHAR_FIRMWARE_ID, &["chargen.rom"])
                .with_env_var("EMU198X_VIC20_CHAR"),
        ]
    }

    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Vic20Ntsc => "commodore-vic-20-ntsc",
            Self::Vic20Pal => "commodore-vic-20-pal",
        }
    }
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Vic20Ntsc => "Commodore VIC-20 (NTSC)",
            Self::Vic20Pal => "Commodore VIC-20 (PAL)",
        }
    }
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Vic20Ntsc => Region::Ntsc,
            Self::Vic20Pal => Region::Pal,
        }
    }
}

pub const KERNAL_FIRMWARE_ID: &str = "commodore-vic-20-kernal";
pub const BASIC_FIRMWARE_ID: &str = "commodore-vic-20-basic";
pub const CHAR_FIRMWARE_ID: &str = "commodore-vic-20-char";

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![profile_for(Model::Vic20Ntsc), profile_for(Model::Vic20Pal)]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("commodore-vic-20"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1981,
        summary: "Commodore VIC-20 / VC-20 — 6502 + MOS VIC-I (6560/6561) + 5 KB RAM, KERNAL + BASIC + character ROMs, cassette / cartridge.".into(),
        clock: ClockDesc::new("cpu-cycle", ClockRate::from_hz(1_022_727)),
        firmware: vec![
            FirmwareRequirement::new(KERNAL_FIRMWARE_ID, "VIC-20 KERNAL ROM (8 KB)", false),
            FirmwareRequirement::new(BASIC_FIRMWARE_ID, "VIC-20 BASIC ROM (8 KB)", false),
            FirmwareRequirement::new(CHAR_FIRMWARE_ID, "VIC-20 character ROM (4 KB)", false),
        ],
        media_slots: vec![
            MediaSlot::new(
                "cartridge-1",
                "Cartridge Slot",
                MediaKind::Cartridge,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
            MediaSlot::new(
                "program-1",
                "Program (.prg)",
                MediaKind::Program,
                false,
                WritebackPolicy::InMemoryOnly,
            ),
        ],
        capabilities: CapabilitySet::with_all([
            known_capability("variant-switch"),
            known_capability("keyboard-input"),
            known_capability("scripted-input"),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_declares_three_roms() {
        let p = profile_for(Model::Vic20Ntsc);
        assert_eq!(p.firmware.len(), 3);
    }

    #[test]
    fn profile_declares_the_standard_program_slot() {
        let p = profile_for(Model::Vic20Ntsc);
        assert!(
            p.media_slots
                .iter()
                .any(|slot| slot.id == "program-1" && slot.kind == MediaKind::Program)
        );
    }
}
