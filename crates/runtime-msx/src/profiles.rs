//! MSX family profile catalogue: MSX1 and MSX2.

//!
//! Two generic presets per generation, one per video region, and real
//! machines. MSX machines differ in where their RAM and cartridge ports sit,
//! and that is what a real model here records: its slot layout, its region
//! and its year, each from the machine's openMSX configuration
//! (`share/machines/<machine>.xml`). Their BIOSes differ in character set and
//! keyboard too; each model looks for its own dump first and falls back to
//! `msx.rom`, or for an MSX2 `msx2.rom` and `msx2ext.rom`.
//!
//! An MSX2 adds a SUB-ROM, which is firmware of its own, and on most
//! European machines a memory mapper. The generic MSX2 presets take
//! C-BIOS's layout, which most MSX2s share: the SUB-ROM in subslot 0 and a
//! 128 KB mapper in subslot 2 of an expanded slot 3. Built-in software the
//! real machines carry in other subslots (the VG-8235's disk ROM, the
//! HB-F1's and FS-A1's firmware menus) is not modelled; those subslots read
//! as empty.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};
use machine_msx::{MemoryMapper, MsxLayout, RamRegion, SlotAddress};

/// Supported MSX models.
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
    /// MSX2 NTSC: C-BIOS's layout, a 128 KB mapper.
    Msx2Ntsc,
    /// MSX2 PAL: C-BIOS's layout, a 128 KB mapper.
    Msx2Pal,
    /// Philips VG-8235: the European MSX2, a 128 KB mapper in subslot 3-2.
    PhilipsVg8235,
    /// Sony HB-F1: a Japanese MSX2 with 64 KB of plain RAM in subslot 3-3.
    SonyHbF1,
    /// Panasonic FS-A1: a Japanese MSX2 with 64 KB of plain RAM in subslot
    /// 3-0 and the SUB-ROM in 3-1.
    PanasonicFsA1,
}

impl Model {
    /// Every model.
    pub const ALL: [Self; 13] = [
        Self::Msx1Ntsc,
        Self::Msx1Pal,
        Self::SonyHb10p,
        Self::ToshibaHx10,
        Self::MitsubishiMlF80,
        Self::YamahaCx5m,
        Self::MitsubishiMlFx1,
        Self::SonyHb10,
        Self::Msx2Ntsc,
        Self::Msx2Pal,
        Self::PhilipsVg8235,
        Self::SonyHbF1,
        Self::PanasonicFsA1,
    ];
    /// Stable selection ids shared by launchers, scripts and menus.
    pub const VARIANT_IDS: [&'static str; 13] = [
        "microsoft-msx1-ntsc",
        "microsoft-msx1-pal",
        "sony-hb-10p",
        "toshiba-hx-10",
        "mitsubishi-ml-f80",
        "yamaha-cx5m",
        "mitsubishi-ml-fx1",
        "sony-hb-10",
        "microsoft-msx2-ntsc",
        "microsoft-msx2-pal",
        "philips-vg-8235",
        "sony-hb-f1",
        "panasonic-fs-a1",
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
            "msx2-ntsc" => Some(Self::Msx2Ntsc),
            "msx2-pal" => Some(Self::Msx2Pal),
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

    /// Whether this is an MSX2, with a V9938, a SUB-ROM and a clock.
    #[must_use]
    pub const fn is_msx2(self) -> bool {
        matches!(
            self,
            Self::Msx2Ntsc
                | Self::Msx2Pal
                | Self::PhilipsVg8235
                | Self::SonyHbF1
                | Self::PanasonicFsA1
        )
    }

    #[must_use]
    pub fn firmware_sources(self) -> Vec<emu198x_shell::FirmwareSource> {
        let sub_rom: &'static [&'static str] = match self {
            Self::PhilipsVg8235 => &["vg8235-00_msx2sub.rom", "msx2ext.rom"],
            Self::SonyHbF1 => &["hb-f1_msx2sub.rom", "msx2ext.rom"],
            Self::PanasonicFsA1 => &["fs-a1_msx2sub.rom", "msx2ext.rom"],
            _ => &["msx2ext.rom"],
        };
        let candidates: &'static [&'static str] = match self {
            Self::Msx1Ntsc | Self::Msx1Pal => &["msx.rom"],
            Self::SonyHb10p => &["hb-10p_basic-bios1.rom", "msx.rom"],
            Self::ToshibaHx10 => &["hx-10_basic-bios1.rom", "msx.rom"],
            Self::MitsubishiMlF80 => &["ml-f80_basic-bios1.rom", "msx.rom"],
            Self::YamahaCx5m => &["cx5m_basic-bios1.rom", "msx.rom"],
            Self::MitsubishiMlFx1 => &["ml-fx1_basic-bios1.rom", "msx.rom"],
            Self::SonyHb10 => &["hb-10_basic-bios1.rom", "msx.rom"],
            Self::Msx2Ntsc | Self::Msx2Pal => &["msx2.rom"],
            Self::PhilipsVg8235 => &["vg8235-00_basic-bios2.rom", "msx2.rom"],
            Self::SonyHbF1 => &["hb-f1_basic-bios2.rom", "msx2.rom"],
            Self::PanasonicFsA1 => &["fs-a1_basic-bios2.rom", "msx2.rom"],
        };
        let mut sources = vec![
            emu198x_shell::FirmwareSource::required(BIOS_FIRMWARE_ID, candidates)
                .with_env_var("EMU198X_MSX_BIOS"),
        ];
        if self.is_msx2() {
            sources.push(
                emu198x_shell::FirmwareSource::required(SUB_ROM_FIRMWARE_ID, sub_rom)
                    .with_env_var("EMU198X_MSX_SUB_ROM"),
            );
        }
        sources
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
            Self::Msx2Ntsc => "microsoft-msx2-ntsc",
            Self::Msx2Pal => "microsoft-msx2-pal",
            Self::PhilipsVg8235 => "philips-vg-8235",
            Self::SonyHbF1 => "sony-hb-f1",
            Self::PanasonicFsA1 => "panasonic-fs-a1",
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
            Self::Msx2Ntsc => "Microsoft MSX2 (NTSC)",
            Self::Msx2Pal => "Microsoft MSX2 (PAL)",
            Self::PhilipsVg8235 => "Philips VG-8235",
            Self::SonyHbF1 => "Sony HB-F1",
            Self::PanasonicFsA1 => "Panasonic FS-A1",
        }
    }

    /// Region.
    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::Msx1Ntsc
            | Self::SonyHb10
            | Self::Msx2Ntsc
            | Self::SonyHbF1
            | Self::PanasonicFsA1 => Region::Ntsc,
            Self::Msx1Pal
            | Self::SonyHb10p
            | Self::ToshibaHx10
            | Self::MitsubishiMlF80
            | Self::YamahaCx5m
            | Self::MitsubishiMlFx1
            | Self::Msx2Pal
            | Self::PhilipsVg8235 => Region::Pal,
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
            Self::Msx2Ntsc | Self::Msx2Pal => 1985,
            Self::SonyHb10p
            | Self::MitsubishiMlFx1
            | Self::PhilipsVg8235
            | Self::SonyHbF1
            | Self::PanasonicFsA1 => 1986,
        }
    }

    /// Where the BIOS, RAM and cartridge ports sit.
    #[must_use]
    pub fn layout(self) -> MsxLayout {
        let ram = |slot, start, end| vec![RamRegion { slot, start, end }];
        let carts = |a, b| [SlotAddress::primary(a), SlotAddress::primary(b)];
        let base = MsxLayout::default();
        // Every MSX2 here expands slot 3 and keeps its cartridges in 1 and 2.
        let msx2 = |sub_rom: u8, ram: Vec<RamRegion>, mapper: Option<MemoryMapper>| MsxLayout {
            sub_rom: Some(SlotAddress::expanded(3, sub_rom)),
            ram,
            mapper,
            expanded: [false, false, false, true],
            ..MsxLayout::default()
        };
        let mapper_128k = Some(MemoryMapper {
            slot: SlotAddress::expanded(3, 2),
            size_kb: 128,
        });
        match self {
            Self::Msx2Ntsc | Self::Msx2Pal | Self::PhilipsVg8235 => {
                msx2(0, Vec::new(), mapper_128k)
            }
            Self::SonyHbF1 => msx2(0, ram(SlotAddress::expanded(3, 3), 0x0000, 0xFFFF), None),
            Self::PanasonicFsA1 => msx2(1, ram(SlotAddress::expanded(3, 0), 0x0000, 0xFFFF), None),
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
        let layout = self.layout();
        let plain: u32 = layout
            .ram
            .iter()
            .map(|region| (u32::from(region.end) - u32::from(region.start) + 1) / 1024)
            .sum();
        plain + layout.mapper.map_or(0, |mapper| u32::from(mapper.size_kb))
    }
}

/// Stable main-ROM firmware identifier, shared by every model, MSX2 ones
/// included: the slot is the same 32 KB main ROM, whatever generation of
/// BASIC it holds.
pub const BIOS_FIRMWARE_ID: &str = "msx1-bios";

/// Stable SUB-ROM firmware identifier, for the MSX2 models.
pub const SUB_ROM_FIRMWARE_ID: &str = "msx2-sub-rom";

/// Returns the MSX family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

/// Returns the profile metadata for one MSX model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let summary = if model.is_msx2() {
        format!(
            "MSX2 — Z80A + V9938 (128 KB VRAM) + AY-3-8910 + 8255 PPI + RP5C01 clock, {} KB RAM{}, BIOS and SUB-ROM boot, MegaROM mapper (plain/Konami/KonamiSCC/ASCII8/ASCII16) cartridge support.",
            model.ram_kb(),
            if model.layout().mapper.is_some() {
                " in a memory mapper"
            } else {
                ""
            },
        )
    } else {
        format!(
            "MSX1 — Z80A + TMS9918A + AY-3-8910 + 8255 PPI, {} KB RAM, BIOS-driven boot, MegaROM mapper (plain/Konami/KonamiSCC/ASCII8/ASCII16) cartridge support.",
            model.ram_kb()
        )
    };
    let mut firmware = vec![FirmwareRequirement::new(
        BIOS_FIRMWARE_ID,
        if model.is_msx2() {
            "MSX2 main ROM (BIOS + MSX-BASIC 2.0, 32 KB)"
        } else {
            "MSX1 main ROM (BIOS + MSX-BASIC 1.0, 32 KB)"
        },
        false,
    )];
    if model.is_msx2() {
        firmware.push(FirmwareRequirement::new(
            SUB_ROM_FIRMWARE_ID,
            "MSX2 SUB-ROM (BIOS extensions, 16 KB)",
            false,
        ));
    }
    MachineProfile {
        machine_id: MachineId::from("microsoft-msx1"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Msx,
        region: model.region(),
        release_year: model.release_year(),
        summary: summary.into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
        firmware,
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
    fn msx2_models_need_a_sub_rom() {
        for model in Model::ALL {
            let profile = profile_for(model);
            let sources = model.firmware_sources();
            if model.is_msx2() {
                assert_eq!(profile.firmware.len(), 2, "{model:?}");
                assert_eq!(profile.firmware[1].id.as_ref(), SUB_ROM_FIRMWARE_ID);
                assert_eq!(sources.len(), 2, "{model:?}");
                assert!(model.layout().sub_rom.is_some(), "{model:?}");
                assert!(profile.summary.starts_with("MSX2"));
            } else {
                assert_eq!(profile.firmware.len(), 1, "{model:?}");
                assert_eq!(sources.len(), 1, "{model:?}");
                assert!(model.layout().sub_rom.is_none(), "{model:?}");
            }
        }
    }

    #[test]
    fn msx2_models_carry_their_layouts() {
        assert_eq!(Model::PhilipsVg8235.ram_kb(), 128);
        assert_eq!(Model::PhilipsVg8235.region(), Region::Pal);
        assert_eq!(Model::SonyHbF1.ram_kb(), 64);
        assert_eq!(Model::SonyHbF1.layout().mapper, None);
        assert_eq!(
            Model::PanasonicFsA1.layout().sub_rom,
            Some(SlotAddress::expanded(3, 1))
        );
        assert_eq!(Model::PanasonicFsA1.frame_ticks(), 228 * 262);
        assert_eq!(Model::from_variant_id("msx2-pal"), Some(Model::Msx2Pal));
        assert_eq!(Model::from_variant_id("msx2-ntsc"), Some(Model::Msx2Ntsc));
        assert!(profile_for(Model::SonyHbF1).summary.contains("64 KB RAM,"));
        assert!(
            profile_for(Model::Msx2Pal)
                .summary
                .contains("memory mapper")
        );
    }

    #[test]
    fn pal_profile_uses_pal_region() {
        let profile = profile_for(Model::Msx1Pal);
        assert_eq!(profile.region, Region::Pal);
    }
}
