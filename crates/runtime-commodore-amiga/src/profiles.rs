//! Commodore Amiga family profile catalogue.

use commodore_agnus_ocs::{NTSC_CCKS_PER_FRAME, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME};
use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};
use gvp_a530::{A530Config, A530RamSize};
use machine_commodore_amiga_ocs::RamConfig;
use motorola_68000::CpuModel;

use crate::amiga_model::{Accelerator, AmigaConfig, CpuConfig, CpuKind};

/// Supported Amiga models in the fresh workspace bootstrap.
///
/// Each variant bundles a named RAM layout and a video region. The
/// PAL group covers the common A500 configurations users actually
/// shipped in Europe (50 Hz, 312-line frames). The NTSC group covers
/// the same configurations as sold in North America (60 Hz, 262-line
/// frames with the short/long line alternation modelled in the chip
/// layer per HRM p. 785). Custom layouts outside these presets are
/// available through the runtime variant's `with_ram_config` constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Model {
    /// A1000 OCS PAL (shipping config, 1985): 256 KiB chip RAM,
    /// 64 KiB bootstrap ROM, and writable WOM for Kickstart loaded
    /// from floppy.
    A1000OcsPal,
    /// A1000 OCS NTSC (US shipping config, 1985): same RAM/bootstrap
    /// layout as the PAL A1000; differs only in Agnus region.
    A1000OcsNtsc,
    /// Stock A500 OCS PAL: 512 KiB chip RAM only.
    A500OcsPal,
    /// Stock A500 OCS NTSC.
    A500OcsNtsc,
    /// A500 + A501 trapdoor: 512 KiB chip + 512 KiB slow RAM (PAL).
    A500OcsPalA501,
    /// A500 + A501 trapdoor (NTSC).
    A500OcsNtscA501,
    /// A500+ (Plus) layout: 1 MiB chip RAM, no trapdoor (PAL).
    /// **ECS chipset** — A500+ shipped with ECS Agnus 8375 and ECS
    /// (Super) Denise 8373, plus Kickstart 2.04. Backed by
    /// `AmigaEcs` not `AmigaOcs`.
    A500PlusEcsPal,
    /// A500+ (Plus) layout (NTSC). ECS chipset; Kickstart 2.04.
    A500PlusEcsNtsc,
    /// Maxed A500: 1 MiB chip + 512 KiB slow + 8 MiB Zorro-II fast (PAL).
    A500OcsPalMaxed,
    /// Maxed A500 (NTSC).
    A500OcsNtscMaxed,
    /// A1200 AGA PAL: 2 MiB chip RAM, 68EC020 CPU, Alice + Lisa
    /// chipset, Gayle IDE, Kickstart 3.0 / 3.1. Backed by
    /// `AmigaA1200`.
    A1200AgaPal,
    /// A1200 AGA NTSC.
    A1200AgaNtsc,
    /// A600 ECS PAL: 1 MiB chip RAM, 68000 CPU, ECS Agnus 8375 + ECS
    /// Denise 8373, Gayle (IDE + PCMCIA decode), Kickstart 2.05.
    /// Backed by `AmigaEcs` — the chip stack is identical to the
    /// A500+ (same Agnus + Denise + Paula); A600 distinctive
    /// features (Gayle-driven IDE, PCMCIA slot, smaller form
    /// factor) layer over the same ECS substrate.
    A600EcsPal,
    /// A600 ECS NTSC.
    A600EcsNtsc,
    /// A2000 mixed PAL: ECS Fat Agnus 8372A jumpered for 1 MiB chip
    /// RAM, OCS Denise, 68000 CPU, Zorro-II slots and Kickstart 1.3 /
    /// 2.04. Backed by `AmigaOcs` because the mixed stack retains the
    /// OCS Denise shape. A2000 Rev A (early Agnus 8371, 512 KiB chip)
    /// does not yet have a distinct runtime catalogue entry; the raw
    /// `AmigaOcs::with_ram_config` constructor can select that early
    /// chip without attaching A2000 runtime identity.
    A2000OcsPal,
    /// A2000 OCS NTSC.
    A2000OcsNtsc,
    /// A500 OCS PAL validation configuration with a 40 MHz GVP A530,
    /// 1 MiB accelerator-local RAM, cache disabled, and SCSI autoboot
    /// disabled.
    A500OcsPalGvpA530,
    /// NTSC counterpart of the A500 + GVP A530 validation
    /// configuration.
    A500OcsNtscGvpA530,
}

/// Native PAL frame length in Agnus colour clocks.
pub const A500_PAL_FRAME_CCKS: u64 = PAL_CCKS_PER_LINE as u64 * PAL_LINES_PER_FRAME as u64;

/// Native PAL frame length in machine ticks (master/4 = 2 per CCK).
pub const A500_PAL_FRAME_TICKS: u64 = A500_PAL_FRAME_CCKS * 2;

/// PAL Agnus colour-clock rate in Hz. PAL master clock = 28.37516
/// MHz; CCK = master / 8.
pub const A500_PAL_CCK_HZ: u64 = 28_375_160 / 8;

/// Native NTSC frame length in Agnus colour clocks. NTSC alternates
/// short (227 CCK) and long (228 CCK) lines per HRM p. 785, totalling
/// 131 × 227 + 131 × 228 = 59,605 CCKs across the 262-line frame.
pub const A500_NTSC_FRAME_CCKS: u64 = NTSC_CCKS_PER_FRAME as u64;

/// Native NTSC frame length in machine ticks (master/4 = 2 per CCK).
/// 59,605 × 2 = 119,210 ticks.
pub const A500_NTSC_FRAME_TICKS: u64 = A500_NTSC_FRAME_CCKS * 2;

/// NTSC Agnus colour-clock rate in Hz. NTSC master clock = 28.63636
/// MHz (4 × the colour subcarrier 3.579545 MHz); CCK = master / 8.
pub const A500_NTSC_CCK_HZ: u64 = 28_636_360 / 8;

impl Model {
    /// Stable machine-local model identifier.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::A1000OcsPal => "commodore-amiga-a1000-ocs-pal",
            Self::A1000OcsNtsc => "commodore-amiga-a1000-ocs-ntsc",
            Self::A500OcsPal => "commodore-amiga-a500-ocs-pal",
            Self::A500OcsNtsc => "commodore-amiga-a500-ocs-ntsc",
            Self::A500OcsPalA501 => "commodore-amiga-a500-ocs-pal-a501",
            Self::A500OcsNtscA501 => "commodore-amiga-a500-ocs-ntsc-a501",
            Self::A500PlusEcsPal => "commodore-amiga-a500-plus-ecs-pal",
            Self::A500PlusEcsNtsc => "commodore-amiga-a500-plus-ecs-ntsc",
            Self::A500OcsPalMaxed => "commodore-amiga-a500-ocs-pal-maxed",
            Self::A500OcsNtscMaxed => "commodore-amiga-a500-ocs-ntsc-maxed",
            Self::A1200AgaPal => "commodore-amiga-a1200-aga-pal",
            Self::A1200AgaNtsc => "commodore-amiga-a1200-aga-ntsc",
            Self::A600EcsPal => "commodore-amiga-a600-ecs-pal",
            Self::A600EcsNtsc => "commodore-amiga-a600-ecs-ntsc",
            Self::A2000OcsPal => "commodore-amiga-a2000-ocs-pal",
            Self::A2000OcsNtsc => "commodore-amiga-a2000-ocs-ntsc",
            Self::A500OcsPalGvpA530 => "commodore-amiga-a500-ocs-pal-gvp-a530-40mhz-1m",
            Self::A500OcsNtscGvpA530 => "commodore-amiga-a500-ocs-ntsc-gvp-a530-40mhz-1m",
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
            Self::A1000OcsPal => "Commodore Amiga 1000 (OCS PAL)",
            Self::A1000OcsNtsc => "Commodore Amiga 1000 (OCS NTSC)",
            Self::A500OcsPal => "Commodore Amiga 500 (OCS PAL)",
            Self::A500OcsNtsc => "Commodore Amiga 500 (OCS NTSC)",
            Self::A500OcsPalA501 => "Commodore Amiga 500 + A501 trapdoor (OCS PAL)",
            Self::A500OcsNtscA501 => "Commodore Amiga 500 + A501 trapdoor (OCS NTSC)",
            Self::A500PlusEcsPal => "Commodore Amiga 500+ (ECS PAL)",
            Self::A500PlusEcsNtsc => "Commodore Amiga 500+ (ECS NTSC)",
            Self::A500OcsPalMaxed => "Commodore Amiga 500 maxed (OCS PAL, 1M+512K+8M)",
            Self::A500OcsNtscMaxed => "Commodore Amiga 500 maxed (OCS NTSC, 1M+512K+8M)",
            Self::A1200AgaPal => "Commodore Amiga 1200 (AGA PAL)",
            Self::A1200AgaNtsc => "Commodore Amiga 1200 (AGA NTSC)",
            Self::A600EcsPal => "Commodore Amiga 600 (ECS PAL)",
            Self::A600EcsNtsc => "Commodore Amiga 600 (ECS NTSC)",
            Self::A2000OcsPal => "Commodore Amiga 2000 (OCS PAL, Fat Agnus 8372A)",
            Self::A2000OcsNtsc => "Commodore Amiga 2000 (OCS NTSC, Fat Agnus 8372A)",
            Self::A500OcsPalGvpA530 => "Commodore Amiga 500 + GVP A530 (OCS PAL, 40 MHz, 1 MiB)",
            Self::A500OcsNtscGvpA530 => "Commodore Amiga 500 + GVP A530 (OCS NTSC, 40 MHz, 1 MiB)",
        }
    }

    /// RAM layout for this model. Use the runtime variant's
    /// `with_ram_config` constructor for a layout outside the preset set.
    /// The PAL/NTSC pairs
    /// share a layout — only Agnus differs.
    #[must_use]
    pub const fn ram_config(self) -> RamConfig {
        match self {
            Self::A1000OcsPal | Self::A1000OcsNtsc => RamConfig {
                chip_kb: 256,
                slow_kb: 0,
                fast_kb: 0,
            },
            Self::A500OcsPal
            | Self::A500OcsNtsc
            | Self::A500OcsPalGvpA530
            | Self::A500OcsNtscGvpA530 => RamConfig::bare(),
            Self::A500OcsPalA501 | Self::A500OcsNtscA501 => RamConfig::a501_trapdoor(),
            Self::A500PlusEcsPal | Self::A500PlusEcsNtsc => RamConfig::a500_plus(),
            Self::A500OcsPalMaxed | Self::A500OcsNtscMaxed => RamConfig::a500_maxed(),
            // Stock A1200: AGA ceiling (2 MiB) of chip RAM, no slow,
            // no fast. Trapdoor accelerators + fast RAM expansions
            // are accelerator config, not stock.
            Self::A1200AgaPal | Self::A1200AgaNtsc => RamConfig {
                chip_kb: (crate::amiga_model::ECS_AGA_CHIP_RAM_BYTES / crate::amiga_model::KIB)
                    as u32,
                slow_kb: 0,
                fast_kb: 0,
            },
            // Stock A600: 1 MiB chip RAM (ECS Agnus 8375 ceiling
            // for stock; expansion to 2 MiB via the A604 trapdoor
            // is reachable through with_ram_config). No slow, no
            // fast Zorro — A600 has no Zorro slots.
            Self::A600EcsPal | Self::A600EcsNtsc => RamConfig::a500_plus(),
            // Canonical A2000 Rev B profile: Fat Agnus 8372A jumpered
            // for its 1 MiB chip-RAM maximum. The base machine shipped
            // with 512 KiB; no slow trapdoor is present, and additional
            // RAM is normally provided by a Zorro-II board.
            Self::A2000OcsPal | Self::A2000OcsNtsc => RamConfig::a500_plus(),
        }
    }

    /// Whether this model uses the A1000 bootstrap-ROM boot path
    /// (64 KiB bootstrap into WOM) or the standard Kickstart boot
    /// path (256 / 512 KiB Kickstart). True for both A1000 PAL and
    /// A1000 NTSC.
    #[must_use]
    pub const fn is_a1000(self) -> bool {
        matches!(self, Self::A1000OcsPal | Self::A1000OcsNtsc)
    }

    /// Whether this OCS-shaped profile installs Fat Agnus 8372A.
    ///
    /// This is a silicon-revision choice, not a deduction from the
    /// configured RAM size: a 512 KiB-populated board can contain an
    /// 8372A, while no RAM expansion can turn an early Agnus into one.
    #[must_use]
    pub const fn uses_fat_agnus_8372a(self) -> bool {
        matches!(
            self,
            Self::A500OcsPalMaxed | Self::A500OcsNtscMaxed | Self::A2000OcsPal | Self::A2000OcsNtsc
        )
    }

    /// Whether this motherboard contains the Gayle IDE and PCMCIA controller.
    #[must_use]
    pub const fn uses_gayle(self) -> bool {
        matches!(
            self,
            Self::A600EcsPal | Self::A600EcsNtsc | Self::A1200AgaPal | Self::A1200AgaNtsc
        )
    }

    /// Whether this model is NTSC. Drives the Agnus region selection
    /// and the runtime frame-tick / CCK-rate constants.
    #[must_use]
    pub const fn is_ntsc(self) -> bool {
        matches!(
            self,
            Self::A1000OcsNtsc
                | Self::A500OcsNtsc
                | Self::A500OcsNtscA501
                | Self::A500PlusEcsNtsc
                | Self::A500OcsNtscMaxed
                | Self::A1200AgaNtsc
                | Self::A600EcsNtsc
                | Self::A2000OcsNtsc
                | Self::A500OcsNtscGvpA530
        )
    }

    /// Whether this model uses the AGA chip stack (`AmigaA1200`).
    /// Drives the third dispatch arm in `AmigaRuntimeKind::new`.
    #[must_use]
    pub const fn is_aga(self) -> bool {
        matches!(self, Self::A1200AgaPal | Self::A1200AgaNtsc)
    }

    /// Whether this model uses the ECS chip stack (`AmigaEcs`).
    /// Drives the dispatch in `AmigaRuntimeKind::new`. A500+ and
    /// A600 share the ECS chipset (Agnus 8375 + Denise 8373);
    /// A3000 joins once Cpu68030 wiring in `AmigaEcs` lands.
    #[must_use]
    pub const fn is_ecs(self) -> bool {
        matches!(
            self,
            Self::A500PlusEcsPal | Self::A500PlusEcsNtsc | Self::A600EcsPal | Self::A600EcsNtsc
        )
    }

    /// Chipset family for this model. Per
    /// [`amiga-machine-catalogue.md`], chipset is the only axis the
    /// kind enum discriminates on; everything else (CPU, memory
    /// layout, region, Kickstart) is configuration.
    ///
    /// [`amiga-machine-catalogue.md`]: ../../../../../knowledge/decisions/amiga-machine-catalogue.md
    #[must_use]
    pub const fn chipset(self) -> crate::amiga_model::ChipsetKind {
        use crate::amiga_model::ChipsetKind;
        match self {
            Self::A1000OcsPal
            | Self::A1000OcsNtsc
            | Self::A500OcsPal
            | Self::A500OcsNtsc
            | Self::A500OcsPalA501
            | Self::A500OcsNtscA501
            | Self::A500OcsPalMaxed
            | Self::A500OcsNtscMaxed
            | Self::A2000OcsPal
            | Self::A2000OcsNtsc
            | Self::A500OcsPalGvpA530
            | Self::A500OcsNtscGvpA530 => ChipsetKind::Ocs,
            Self::A500PlusEcsPal | Self::A500PlusEcsNtsc | Self::A600EcsPal | Self::A600EcsNtsc => {
                ChipsetKind::Ecs
            }
            Self::A1200AgaPal | Self::A1200AgaNtsc => ChipsetKind::Aga,
        }
    }

    /// Active CPU type for this model configuration.
    ///
    /// [`amiga-machine-catalogue.md`]: ../../../../../knowledge/decisions/amiga-machine-catalogue.md
    #[must_use]
    pub const fn cpu(self) -> crate::amiga_model::CpuKind {
        use crate::amiga_model::CpuKind;
        match self {
            Self::A1000OcsPal
            | Self::A1000OcsNtsc
            | Self::A500OcsPal
            | Self::A500OcsNtsc
            | Self::A500OcsPalA501
            | Self::A500OcsNtscA501
            | Self::A500OcsPalMaxed
            | Self::A500OcsNtscMaxed
            | Self::A500PlusEcsPal
            | Self::A500PlusEcsNtsc
            | Self::A600EcsPal
            | Self::A600EcsNtsc
            | Self::A2000OcsPal
            | Self::A2000OcsNtsc => CpuKind::M68000,
            Self::A1200AgaPal | Self::A1200AgaNtsc => CpuKind::M68EC020,
            Self::A500OcsPalGvpA530 | Self::A500OcsNtscGvpA530 => CpuKind::M68EC030,
        }
    }

    /// Canonical immutable construction configuration.
    #[must_use]
    pub const fn config(self) -> AmigaConfig {
        let system_tick_hz = if self.is_ntsc() {
            A500_NTSC_CCK_HZ * 2
        } else {
            A500_PAL_CCK_HZ * 2
        };
        let cpu_clock_hz = match self.cpu() {
            CpuKind::M68000 | CpuKind::M68010 => system_tick_hz,
            CpuKind::M68EC020 => system_tick_hz * 2,
            CpuKind::M68020 => system_tick_hz,
            CpuKind::M68EC030 => 40_000_000,
            CpuKind::M68030 | CpuKind::M68040 | CpuKind::Ac68080 => system_tick_hz,
        };
        let cpu_model = match self {
            Self::A1000OcsPal
            | Self::A1000OcsNtsc
            | Self::A500OcsPal
            | Self::A500OcsNtsc
            | Self::A500OcsPalA501
            | Self::A500OcsNtscA501
            | Self::A500OcsPalMaxed
            | Self::A500OcsNtscMaxed
            | Self::A500PlusEcsPal
            | Self::A500PlusEcsNtsc
            | Self::A600EcsPal
            | Self::A600EcsNtsc
            | Self::A2000OcsPal
            | Self::A2000OcsNtsc => CpuModel::M68000,
            Self::A1200AgaPal | Self::A1200AgaNtsc => CpuModel::M68EC020,
            Self::A500OcsPalGvpA530 | Self::A500OcsNtscGvpA530 => CpuModel::M68EC030,
        };
        let accelerator = match self {
            Self::A500OcsPalGvpA530 | Self::A500OcsNtscGvpA530 => Some(Accelerator::GvpA530(
                A530Config::new(A530RamSize::Mib1, 0)
                    .with_cache_enabled(false)
                    .with_autoboot_enabled(false),
            )),
            _ => None,
        };
        AmigaConfig::new(
            self,
            self.ram_config(),
            CpuConfig::new(cpu_model, cpu_clock_hz),
            accelerator,
        )
    }
}

/// Returns the initial Amiga family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::A1000OcsPal),
        profile_for(Model::A1000OcsNtsc),
        profile_for(Model::A500OcsPal),
        profile_for(Model::A500OcsNtsc),
        profile_for(Model::A500OcsPalA501),
        profile_for(Model::A500OcsNtscA501),
        profile_for(Model::A500PlusEcsPal),
        profile_for(Model::A500PlusEcsNtsc),
        profile_for(Model::A500OcsPalMaxed),
        profile_for(Model::A500OcsNtscMaxed),
        profile_for(Model::A1200AgaPal),
        profile_for(Model::A1200AgaNtsc),
        profile_for(Model::A600EcsPal),
        profile_for(Model::A600EcsNtsc),
        profile_for(Model::A2000OcsPal),
        profile_for(Model::A2000OcsNtsc),
        profile_for(Model::A500OcsPalGvpA530),
        profile_for(Model::A500OcsNtscGvpA530),
    ]
}

/// Returns the profile metadata for one Amiga model.
///
/// PAL and NTSC variants of the same hardware share firmware, media
/// slots, capabilities, and family — they differ in `region`,
/// `clock`, and the descriptive strings. The advertised `clock` is the
/// emulator's *system-tick* rate — two ticks per Agnus colour clock, so
/// 7.159090 MHz on NTSC and 7.093790 MHz on PAL (the stock 68000 clock).
/// It is deliberately the tick rate, not the 3.5 MHz colour clock: the
/// session derives the recording frame rate as `clock.rate /
/// native_frame_ticks`, and `native_frame_ticks` is counted in ticks
/// (`A500_PAL_FRAME_TICKS = CCKS * 2`). Advertising the colour clock here
/// would halve the computed fps (25 instead of 50) and, via the
/// `-shortest` audio mux, truncate recordings to half their frames — the
/// double-buffer "freeze" of issue #470. Every other core follows the same
/// convention (Spectrum advertises its 14 MHz master cycle, not the 3.5 MHz
/// CPU clock).
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    let release_year = match model {
        Model::A1000OcsPal | Model::A1000OcsNtsc => 1985,
        Model::A500OcsPal
        | Model::A500OcsNtsc
        | Model::A500OcsPalA501
        | Model::A500OcsNtscA501
        | Model::A500OcsPalMaxed
        | Model::A500OcsNtscMaxed => 1987,
        Model::A500PlusEcsPal | Model::A500PlusEcsNtsc => 1991,
        Model::A1200AgaPal
        | Model::A1200AgaNtsc
        | Model::A500OcsPalGvpA530
        | Model::A500OcsNtscGvpA530 => 1992,
        Model::A600EcsPal | Model::A600EcsNtsc => 1992,
        // A2000A shipped 1987 alongside the A500; A2000B (Fat Agnus
        // Rev 6.x) is 1989+. We catalogue the Rev B variant here, so
        // 1989 is the closer release year.
        Model::A2000OcsPal | Model::A2000OcsNtsc => 1989,
    };
    let (firmware_id, firmware_name) = if model.is_a1000() {
        (
            "commodore-amiga-a1000-bootstrap-rom",
            "Amiga 1000 bootstrap ROM",
        )
    } else {
        ("commodore-amiga-kickstart-rom", "Amiga Kickstart ROM")
    };
    let region = if model.is_ntsc() {
        Region::Ntsc
    } else {
        Region::Pal
    };
    let cck_hz = if model.is_ntsc() {
        A500_NTSC_CCK_HZ
    } else {
        A500_PAL_CCK_HZ
    };
    // System tick = two ticks per colour clock (see `TICKS_PER_CCK`); this
    // is the unit `native_frame_ticks` counts, so it is the rate the
    // recording-fps derivation needs. See the `profile_for` doc comment.
    let tick_hz = cck_hz.saturating_mul(2);
    MachineProfile {
        machine_id: MachineId::from("commodore-amiga"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Amiga,
        region,
        release_year,
        summary: match model {
            Model::A1000OcsPal => "Amiga 1000 OCS PAL — bootstrap-ROM cold boot into writable WOM, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. Kickstart-to-Workbench disk swaps are scriptable via headless media reloads.".into(),
            Model::A1000OcsNtsc => "Amiga 1000 OCS NTSC — bootstrap-ROM cold boot into writable WOM, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. Boot path matches PAL A1000; Agnus runs on the NTSC clock with the short/long line alternation modelled in the chip layer.".into(),
            Model::A500OcsPal | Model::A500OcsPalA501 | Model::A500OcsPalMaxed => "Amiga OCS-shaped PAL — Kickstart-backed headless boot, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. Snapshots and broader software validation still pending.".into(),
            Model::A500OcsNtsc | Model::A500OcsNtscA501 | Model::A500OcsNtscMaxed => "Amiga OCS-shaped NTSC — Kickstart-backed headless boot at the US 60 Hz field rate, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. NTSC boot validation still pending; structural plumbing is in place via the chip-layer short/long line alternation.".into(),
            Model::A500PlusEcsPal => "Amiga 500 Plus ECS PAL — 68000 + ECS Agnus 8375 / Denise 8373, 1 MiB chip RAM and Kickstart 2.04. The full ECS chip stack supplies programmable timing and enhanced display registers.".into(),
            Model::A500PlusEcsNtsc => "Amiga 500 Plus ECS NTSC — same ECS chip stack as the PAL profile, with NTSC beam timing.".into(),
            Model::A1200AgaPal => "Amiga 1200 AGA PAL — 68EC020 + Alice/Lisa chipset + Gayle, 2 MiB chip RAM, Kickstart 3.0/3.1, 768x576 ARGB framebuffer, Paula-backed stereo runtime audio, DF0 ADF insertion, keyboard input. Workbench boot and six exact AGA Test Kit video cases are registered; broader software compatibility remains under active validation.".into(),
            Model::A1200AgaNtsc => "Amiga 1200 AGA NTSC — 68EC020 + Alice/Lisa chipset + Gayle, 2 MiB chip RAM. NTSC boot validation pending; PAL Agnus path is the active target.".into(),
            Model::A600EcsPal => "Amiga 600 ECS PAL — 68000 + ECS Agnus 8375 / Denise 8373 + Gayle (IDE + PCMCIA decode), 1 MiB chip RAM, Kickstart 2.05. Shares the ECS chip stack with the A500+; A600 form factor and Gayle-driven IDE distinguish it.".into(),
            Model::A600EcsNtsc => "Amiga 600 ECS NTSC — same chip stack as the A600 PAL, NTSC Agnus.".into(),
            Model::A2000OcsPal => "Amiga 2000 mixed PAL — 68000 + ECS Fat Agnus 8372A + OCS Denise + Paula, configured for 1 MiB chip RAM, Kickstart 1.3 / 2.04 and Zorro-II slots. The 8372A path covers identity, RAM ceiling, ten-bit sprite comparators, extended blits, DIWHIGH display-DMA gating and the currently modelled programmable timing registers; additional ECS Agnus behavior remains incomplete. A2000A (early Agnus 8371, 512 KiB chip) does not yet have a distinct runtime model; only the raw `AmigaOcs::with_ram_config` machine constructor can currently select the early chip without A2000 identity.".into(),
            Model::A2000OcsNtsc => "Amiga 2000 mixed NTSC — the same ECS Fat Agnus 8372A + OCS Denise stack as the PAL profile, with NTSC beam timing and the currently modelled 8372A extension registers; additional ECS Agnus behavior remains incomplete.".into(),
            Model::A500OcsPalGvpA530 => "Research validation profile: Amiga 500 OCS PAL with a 40 MHz MC68EC030 GVP A530 and 1 MiB accelerator-local RAM. Cache and SCSI autoboot are disabled so validation does not claim unimplemented behaviour.".into(),
            Model::A500OcsNtscGvpA530 => "Research validation profile: Amiga 500 OCS NTSC with a 40 MHz MC68EC030 GVP A530 and 1 MiB accelerator-local RAM. Cache and SCSI autoboot are disabled so validation does not claim unimplemented behaviour.".into(),
        },
        clock: ClockDesc::new("system-tick", ClockRate::from_hz(tick_hz)),
        firmware: vec![FirmwareRequirement::new(firmware_id, firmware_name, false)],
        media_slots: vec![MediaSlot::new(
            "floppy-0",
            "DF0:",
            MediaKind::Disk,
            false,
            WritebackPolicy::SidecarOnly,
        )],
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
    fn gayle_is_a_motherboard_property_of_a600_and_a1200_profiles() {
        for model in [
            Model::A600EcsPal,
            Model::A600EcsNtsc,
            Model::A1200AgaPal,
            Model::A1200AgaNtsc,
        ] {
            assert!(model.uses_gayle(), "{model:?} should contain Gayle");
        }
        assert!(!Model::A500PlusEcsPal.uses_gayle());
        assert!(!Model::A500PlusEcsNtsc.uses_gayle());
    }

    #[test]
    fn amiga_profile_declares_kickstart_and_df0() {
        let profile = profile_for(Model::A500OcsPal);
        assert_eq!(profile.family, Family::Amiga);
        assert_eq!(profile.region, Region::Pal);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(
            profile.firmware[0].id.as_ref(),
            "commodore-amiga-kickstart-rom"
        );
        assert_eq!(profile.media_slots.len(), 1);
        assert_eq!(profile.media_slots[0].id.as_ref(), "floppy-0");
        assert_eq!(profile.media_slots[0].kind, MediaKind::Disk);
    }

    #[test]
    fn a1000_profile_declares_bootstrap_rom() {
        let profile = profile_for(Model::A1000OcsPal);
        assert_eq!(profile.firmware.len(), 1);
        assert_eq!(
            profile.firmware[0].id.as_ref(),
            "commodore-amiga-a1000-bootstrap-rom"
        );
        assert_eq!(profile.media_slots.len(), 1);
        assert_eq!(profile.media_slots[0].id.as_ref(), "floppy-0");
    }

    #[test]
    fn a530_profiles_keep_the_system_clock_regional() {
        let pal = profile_for(Model::A500OcsPalGvpA530);
        let ntsc = profile_for(Model::A500OcsNtscGvpA530);

        assert_eq!(pal.clock.rate.numerator_hz, A500_PAL_CCK_HZ * 2);
        assert_eq!(ntsc.clock.rate.numerator_hz, A500_NTSC_CCK_HZ * 2);
        assert_eq!(pal.region, Region::Pal);
        assert_eq!(ntsc.region, Region::Ntsc);
        assert_eq!(pal.profile_id.as_str(), Model::A500OcsPalGvpA530.model_id());
        assert_eq!(
            ntsc.profile_id.as_str(),
            Model::A500OcsNtscGvpA530.model_id()
        );
    }
}
