//! MSX1 family profile catalogue.

//!
//! Two generic presets, one per video region, and real machines. MSX1
//! machines differ in where their RAM and cartridge ports sit, and that is
//! what a real model here records: its slot layout, its region and its year,
//! each from the machine's openMSX configuration
//! (`share/machines/<machine>.xml`). Their BIOSes differ in character set and
//! keyboard too; each model looks for its own dump first and falls back to
//! `msx.rom`.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};
use machine_msx::{MsxLayout, RamRegion, SlotAddress};

/// Supported MSX1 models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// MSX1 NTSC (60 Hz / 262 lines).
    Msx1Ntsc,
    /// MSX1 PAL (50 Hz / 313 lines).
    Msx1Pal,
    /// Sony HB-10P: 64 KB of RAM in slot 3, cartridges in 1 and 2.
    SonyHb10p,
    /// Toshiba HX-10: 64 KB of RAM in slot 2, cartridges in 1 and 3.
    ToshibaHx10,
    /// Mitsubishi ML-F80: 64 KB of RAM in slot 1, cartridges in 2 and 3.
    MitsubishiMlF80,
    /// Yamaha CX5M: 32 KB of RAM beside the BIOS in slot 0.
    YamahaCx5m,
    /// Mitsubishi ML-FX1: 64 KB of RAM in subslot 2 of an expanded slot 3.
    MitsubishiMlFx1,
    /// Sony HB-10: 16 KB of RAM at `$C000` beside the BIOS, NTSC.
    SonyHb10,
}

impl Model {
    /// Every model.
    pub const ALL: [Self; 8] = [
        Self::Msx1Ntsc,
        Self::Msx1Pal,
        Self::SonyHb10p,
        Self::ToshibaHx10,
        Self::MitsubishiMlF80,
        Self::YamahaCx5m,
        Self::MitsubishiMlFx1,
        Self::SonyHb10,
    ];
    /// Stable selection ids shared by launchers, scripts and menus.
    pub const VARIANT_IDS: [&'static str; 8] = [
        "microsoft-msx1-ntsc",
        "microsoft-msx1-pal",
        "sony-hb-10p",
        "toshiba-hx-10",
        "mitsubishi-ml-f80",
        "yamaha-cx5m",
        "mitsubishi-ml-fx1",
        "sony-hb-10",
    ];

    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        match id {
            "microsoft-msx1-ntsc" | "msx1-ntsc" | "ntsc" => Some(Self::Msx1Ntsc),
            "microsoft-msx1-pal" | "msx1-pal" | "pal" => Some(Self::Msx1Pal),
            _ => Self::ALL.into_iter().find(|model| model.model_id() == id),
        }
    }

    /// Existing host frame budget in Z80 clocks.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        match self.region() {
            Region::Pal => 228 * 313,
            _ => 228 * 262,
        }
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        let candidates: &'static [&'static str] = match self {
            Self::Msx1Ntsc | Self::Msx1Pal => &["msx.rom"],
            Self::SonyHb10p => &["hb-10p_basic-bios1.rom", "msx.rom"],
            Self::ToshibaHx10 => &["hx-10_basic-bios1.rom", "msx.rom"],
            Self::MitsubishiMlF80 => &["ml-f80_basic-bios1.rom", "msx.rom"],
            Self::YamahaCx5m => &["cx5m_basic-bios1.rom", "msx.rom"],
            Self::MitsubishiMlFx1 => &["ml-fx1_basic-bios1.rom", "msx.rom"],
            Self::SonyHb10 => &["hb-10_basic-bios1.rom", "msx.rom"],
        };
        vec![
            emu198x_shell::FirmwareSource::required(BIOS_FIRMWARE_ID, candidates)
                .with_env_var("EMU198X_MSX_BIOS"),
        ]
    }

    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Msx1Ntsc => "microsoft-msx1-ntsc",
            Self::Msx1Pal => "microsoft-msx1-pal",
            Self::SonyHb10p => "sony-hb-10p",
            Self::ToshibaHx10 => "toshiba-hx-10",
            Self::MitsubishiMlF80 => "mitsubishi-ml-f80",
            Self::YamahaCx5m => "yamaha-cx5m",
            Self::MitsubishiMlFx1 => "mitsubishi-ml-fx1",
            Self::SonyHb10 => "sony-hb-10",
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
            Self::SonyHb10p => "Sony HB-10P",
            Self::ToshibaHx10 => "Toshiba HX-10",
            Self::MitsubishiMlF80 => "Mitsubishi ML-F80",
            Self::YamahaCx5m => "Yamaha CX5M",
            Self::MitsubishiMlFx1 => "Mitsubishi ML-FX1",
            Self::SonyHb10 => "Sony HB-10",
        }
    }

    /// Region.
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Msx1Ntsc | Self::SonyHb10 => Region::Ntsc,
            Self::Msx1Pal
            | Self::SonyHb10p
            | Self::ToshibaHx10
            | Self::MitsubishiMlF80
            | Self::YamahaCx5m
            | Self::MitsubishiMlFx1 => Region::Pal,
        }
    }

    /// The year the machine went on sale; the standard's for the generic
    /// presets.
    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::Msx1Ntsc | Self::Msx1Pal => 1983,
            Self::ToshibaHx10 | Self::MitsubishiMlF80 | Self::YamahaCx5m => 1984,
            Self::SonyHb10 => 1985,
            Self::SonyHb10p | Self::MitsubishiMlFx1 => 1986,
        }
    }

    /// Where the BIOS, RAM and cartridge ports sit.
    #[must_use]
    pub fn layout(self) -> MsxLayout {
        let ram = |slot, start, end| vec![RamRegion { slot, start, end }];
        let carts = |a, b| [SlotAddress::primary(a), SlotAddress::primary(b)];
        let base = MsxLayout::default();
        match self {
            Self::Msx1Ntsc | Self::Msx1Pal | Self::SonyHb10p => base,
            Self::ToshibaHx10 => MsxLayout {
                ram: ram(SlotAddress::primary(2), 0x0000, 0xFFFF),
                cartridge_ports: carts(1, 3),
                ..base
            },
            Self::MitsubishiMlF80 => MsxLayout {
                ram: ram(SlotAddress::primary(1), 0x0000, 0xFFFF),
                cartridge_ports: carts(2, 3),
                ..base
            },
            Self::YamahaCx5m => MsxLayout {
                ram: ram(SlotAddress::primary(0), 0x8000, 0xFFFF),
                ..base
            },
            Self::MitsubishiMlFx1 => MsxLayout {
                ram: ram(SlotAddress::expanded(3, 2), 0x0000, 0xFFFF),
                expanded: [false, false, false, true],
                ..base
            },
            Self::SonyHb10 => MsxLayout {
                ram: ram(SlotAddress::primary(0), 0xC000, 0xFFFF),
                ..base
            },
        }
    }

    /// RAM fitted, in KB.
    #[must_use]
    pub fn ram_kb(self) -> u32 {
        self.layout()
            .ram
            .iter()
            .map(|region| (u32::from(region.end) - u32::from(region.start) + 1) / 1024)
            .sum()
    }
}

/// Stable BIOS firmware identifier (shared by all MSX1 regional
/// variants — the 32 KB main-ROM dump is the same image).
pub const BIOS_FIRMWARE_ID: &str = "msx1-bios";

/// Returns the initial MSX1 family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
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
        release_year: model.release_year(),
        summary: format!(
            "MSX1 — Z80A + TMS9918A + AY-3-8910 + 8255 PPI, {} KB RAM, BIOS-driven boot, MegaROM mapper (plain/Konami/KonamiSCC/ASCII8/ASCII16) cartridge support.",
            model.ram_kb()
        ).into(),
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
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("variant-switch"),
            known_capability("keyboard-input"),
            known_capability("ay-audio"),
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
    fn every_model_round_trips_its_variant_id() {
        for (model, id) in Model::ALL.into_iter().zip(Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(Model::from_variant_id(id), Some(model));
        }
    }

    #[test]
    fn real_models_carry_their_ram() {
        assert_eq!(Model::ToshibaHx10.ram_kb(), 64);
        assert_eq!(Model::YamahaCx5m.ram_kb(), 32);
        assert_eq!(Model::SonyHb10.ram_kb(), 16);
        assert_eq!(Model::SonyHb10.frame_ticks(), 228 * 262);
        assert_eq!(Model::MitsubishiMlFx1.frame_ticks(), 228 * 313);
    }

    #[test]
    fn pal_profile_uses_pal_region() {
        let profile = profile_for(Model::Msx1Pal);
        assert_eq!(profile.region, Region::Pal);
    }
}
