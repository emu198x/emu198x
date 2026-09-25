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

    /// Where to look for this model's KERNAL, most specific first.
    ///
    /// The KERNAL is regional: 901486-06 ships in NTSC machines and 901486-07
    /// in PAL ones (VICE's `vic20model.c` pairs them the same way). They differ
    /// in the screen origin the KERNAL writes to $9000/$9001 at power-on — 5
    /// and 25 on NTSC (Programmer's Reference Guide p.213), 12 and 38 on PAL
    /// — each chosen to centre the 22×23 window on its own chip's raster. The
    /// wrong one boots fine but puts the picture somewhere else: a PAL KERNAL
    /// on a 6560 starts the text 44 pixels in and runs the 22nd column off the
    /// right edge.
    ///
    /// A regional file wins, so a ROM folder holding both serves both models.
    /// `kernal.rom` stays as the fallback for a folder holding only one.
    #[must_use]
    pub const fn kernal_candidates(self) -> &'static [&'static str] {
        match self {
            Self::Vic20Ntsc => &["kernal-ntsc.rom", "kernal.rom"],
            Self::Vic20Pal => &["kernal-pal.rom", "kernal.rom"],
        }
    }

    /// The KERNAL's name where a person picks the file, which says which
    /// region's image this model wants. See [`Self::kernal_candidates`].
    #[must_use]
    pub const fn kernal_display_name(self) -> &'static str {
        match self {
            Self::Vic20Ntsc => "VIC-20 KERNAL ROM, NTSC 901486-06 (8 KB)",
            Self::Vic20Pal => "VIC-20 KERNAL ROM, PAL 901486-07 (8 KB)",
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        use emu198x_shell::FirmwareSource;
        vec![
            FirmwareSource::required(KERNAL_FIRMWARE_ID, self.kernal_candidates())
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
            FirmwareRequirement::new(KERNAL_FIRMWARE_ID, model.kernal_display_name(), false),
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

    /// #1536: both models read the one `kernal.rom`, so an NTSC machine booted
    /// a PAL KERNAL and put its picture off-centre and clipped.
    #[test]
    fn each_model_looks_for_its_own_regional_kernal_first() {
        for (model, regional) in [
            (Model::Vic20Ntsc, "kernal-ntsc.rom"),
            (Model::Vic20Pal, "kernal-pal.rom"),
        ] {
            let sources = model.firmware_sources();
            let kernal = sources
                .iter()
                .find(|source| source.id == KERNAL_FIRMWARE_ID)
                .expect("KERNAL source");
            assert_eq!(kernal.candidates, [regional, "kernal.rom"], "{model:?}");
        }
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
