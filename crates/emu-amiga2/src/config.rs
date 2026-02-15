//! Amiga model configuration and presets.
//!
//! Every field has a sensible default from the model preset. Individual fields
//! can be overridden for accelerator or custom configurations.

/// Amiga model presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmigaModel {
    /// A1000 (OCS, 256K chip, WCS for Kickstart).
    A1000,
    /// A500 (OCS, 512K chip, ROM Kickstart).
    A500,
    /// A500+ (ECS Agnus, 1MB chip).
    A500Plus,
    /// A600 (ECS).
    A600,
    /// A2000 (OCS or ECS).
    A2000,
    /// A1200 (AGA).
    A1200,
    /// Custom: all fields must be set manually.
    Custom,
}

/// Chipset generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chipset {
    /// Original Chip Set.
    Ocs,
    /// Enhanced Chip Set.
    Ecs,
    /// Advanced Graphics Architecture.
    Aga,
}

/// Agnus chip variant (controls chip RAM DMA address range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgnusVariant {
    /// 8361 (NTSC) / 8367 (PAL): 512K chip RAM DMA.
    Agnus8361,
    /// 8370 (NTSC) / 8371 (PAL) Fat Agnus: 1MB chip RAM DMA.
    FatAgnus8371,
    /// 8372A ECS Agnus: 2MB chip RAM DMA.
    Agnus8372,
    /// Alice (AGA): 2MB chip RAM DMA, 64-bit fetch.
    Alice,
}

/// Denise chip variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeniseVariant {
    /// 8362 OCS Denise: 12-bit colour, 6 bitplanes.
    Denise8362,
    /// 8373 ECS Denise: 12-bit colour + EHB, productivity modes.
    SuperDenise8373,
    /// Lisa (AGA): 24-bit colour, 8 bitplanes, HAM8.
    Lisa,
}

/// CPU variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuVariant {
    /// Motorola 68000 (16/32-bit, 16-bit bus).
    M68000,
    /// Motorola 68020.
    M68020,
    /// Motorola 68030.
    M68030,
    /// Motorola 68040.
    M68040,
}

/// Video region (affects timing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// PAL: 312 lines, 28.375160 MHz crystal.
    Pal,
    /// NTSC: 262 lines, 28.636360 MHz crystal.
    Ntsc,
}

/// How the Kickstart is loaded.
#[derive(Debug, Clone)]
pub enum KickstartSource {
    /// ROM (soldered or socketed). Standard for A500+.
    Rom(Vec<u8>),
    /// Writable Control Store (loaded from floppy on A1000).
    Wcs(Vec<u8>),
}

/// Full Amiga configuration.
#[derive(Debug, Clone)]
pub struct AmigaConfig {
    pub model: AmigaModel,
    pub chipset: Chipset,
    pub agnus: AgnusVariant,
    pub denise: DeniseVariant,
    pub cpu: CpuVariant,
    pub region: Region,
    /// Chip RAM size in bytes (must be power of two).
    pub chip_ram_size: usize,
    /// Slow (Ranger) RAM size in bytes.
    pub slow_ram_size: usize,
    /// Fast RAM size in bytes.
    pub fast_ram_size: usize,
    /// Kickstart data and loading method.
    pub kickstart: KickstartSource,
}

impl AmigaConfig {
    /// Create a preset configuration for the given model.
    ///
    /// # Panics
    ///
    /// Panics if `model` is `Custom` (use `AmigaConfig` fields directly).
    #[must_use]
    pub fn preset(model: AmigaModel, kickstart: Vec<u8>) -> Self {
        match model {
            AmigaModel::A1000 => Self {
                model,
                chipset: Chipset::Ocs,
                agnus: AgnusVariant::Agnus8361,
                denise: DeniseVariant::Denise8362,
                cpu: CpuVariant::M68000,
                region: Region::Pal,
                chip_ram_size: 256 * 1024,
                slow_ram_size: 0,
                fast_ram_size: 0,
                kickstart: KickstartSource::Wcs(kickstart),
            },
            AmigaModel::A500 => Self {
                model,
                chipset: Chipset::Ocs,
                agnus: AgnusVariant::Agnus8361,
                denise: DeniseVariant::Denise8362,
                cpu: CpuVariant::M68000,
                region: Region::Pal,
                chip_ram_size: 512 * 1024,
                slow_ram_size: 0,
                fast_ram_size: 0,
                kickstart: KickstartSource::Rom(kickstart),
            },
            AmigaModel::A500Plus => Self {
                model,
                chipset: Chipset::Ecs,
                agnus: AgnusVariant::Agnus8372,
                denise: DeniseVariant::SuperDenise8373,
                cpu: CpuVariant::M68000,
                region: Region::Pal,
                chip_ram_size: 1024 * 1024,
                slow_ram_size: 0,
                fast_ram_size: 0,
                kickstart: KickstartSource::Rom(kickstart),
            },
            AmigaModel::A600 => Self {
                model,
                chipset: Chipset::Ecs,
                agnus: AgnusVariant::Agnus8372,
                denise: DeniseVariant::SuperDenise8373,
                cpu: CpuVariant::M68000,
                region: Region::Pal,
                chip_ram_size: 1024 * 1024,
                slow_ram_size: 0,
                fast_ram_size: 0,
                kickstart: KickstartSource::Rom(kickstart),
            },
            AmigaModel::A2000 => Self {
                model,
                chipset: Chipset::Ocs,
                agnus: AgnusVariant::FatAgnus8371,
                denise: DeniseVariant::Denise8362,
                cpu: CpuVariant::M68000,
                region: Region::Pal,
                chip_ram_size: 1024 * 1024,
                slow_ram_size: 0,
                fast_ram_size: 0,
                kickstart: KickstartSource::Rom(kickstart),
            },
            AmigaModel::A1200 => Self {
                model,
                chipset: Chipset::Aga,
                agnus: AgnusVariant::Alice,
                denise: DeniseVariant::Lisa,
                cpu: CpuVariant::M68020,
                region: Region::Pal,
                chip_ram_size: 2 * 1024 * 1024,
                slow_ram_size: 0,
                fast_ram_size: 0,
                kickstart: KickstartSource::Rom(kickstart),
            },
            AmigaModel::Custom => panic!("Custom model has no preset — set fields directly"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a1000_preset_uses_wcs() {
        let config = AmigaConfig::preset(AmigaModel::A1000, vec![0; 256 * 1024]);
        assert!(matches!(config.kickstart, KickstartSource::Wcs(_)));
        assert_eq!(config.chip_ram_size, 256 * 1024);
        assert_eq!(config.chipset, Chipset::Ocs);
    }

    #[test]
    fn a500_preset_uses_rom() {
        let config = AmigaConfig::preset(AmigaModel::A500, vec![0; 256 * 1024]);
        assert!(matches!(config.kickstart, KickstartSource::Rom(_)));
        assert_eq!(config.chip_ram_size, 512 * 1024);
    }
}
