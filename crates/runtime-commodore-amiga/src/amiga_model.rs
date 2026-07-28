//! Hierarchical Amiga model catalogue and family-wide configuration
//! types.
//!
//! Today the underlying tag is the existing flat
//! [`crate::profiles::Model`] enum; this module exposes it through
//! family-grouped const aliases so call sites read as
//! `amiga_model::a500::PAL` instead of `Model::A500OcsPal`. As new
//! machines arrive per the rollout plan, their constants land under
//! the matching submodule below without touching the flat enum's
//! call-site shape.
//!
//! See [`knowledge/decisions/amiga-machine-catalogue.md`] for the
//! binding decision behind this layout.
//!
//! [`knowledge/decisions/amiga-machine-catalogue.md`]: ../../../../../knowledge/decisions/amiga-machine-catalogue.md

pub use crate::profiles::Model;
use gvp_a530::A530Config;
use machine_commodore_amiga_ocs::RamConfig;
use motorola_68000::CpuModel;

// ─── Byte-size convenience constants ────────────────────────────────
//
// Used by the model catalogue and the boot-invariant tests so size
// expressions like `512 * KIB` or `2 * MIB` read at a glance.

/// One kibibyte (1024 bytes).
pub const KIB: usize = 1024;
/// One mebibyte (1024 KiB).
pub const MIB: usize = 1024 * KIB;

// ─── Chip-RAM ceilings per Agnus revision ───────────────────────────
//
// Pedagogically named by the Agnus that gates the ceiling. The chip
// stack itself enforces the actual decode; these constants are for
// catalogue / test readability.

/// Chip RAM ceiling on the first Fatter Agnus 8361 / 8367 (A1000):
/// 512 KiB. The shipping machine includes 256 KiB on the motherboard;
/// the front expansion can supply the other 256 KiB within the same
/// Agnus address space.
pub const FATTER_AGNUS_CHIP_RAM_BYTES: usize = 512 * KIB;

/// Chip RAM ceiling on OCS 8370 (NTSC) / 8371 (PAL early) Agnus
/// shipped in the A500 Rev 3-5 board: 512 KiB. The canonical "stock
/// A500" chip-RAM size.
pub const OCS_AGNUS_CHIP_RAM_BYTES: usize = 512 * KIB;

/// Chip RAM ceiling on ECS Fat Agnus 8372A: 1 MiB. It is commonly
/// paired with OCS Denise in later A500, A2000 and CDTV boards,
/// producing the mixed chip stack catalogued under the OCS-shaped
/// runtime arm.
pub const FAT_AGNUS_CHIP_RAM_BYTES: usize = MIB;

/// Chip RAM ceiling on ECS Super Agnus 8375 and AGA Alice: 2 MiB.
/// A1200, A4000, CD32, and the A3000's Super Agnus all reach this
/// ceiling. AGA splits the address space across 32-bit fetches but
/// the chip-RAM total stays at 2 MiB.
pub const ECS_AGA_CHIP_RAM_BYTES: usize = 2 * MIB;

/// Chipset axis discriminant.
///
/// The only structural axis on `AmigaRuntimeKind`. Per
/// [`amiga-machine-catalogue.md`], machine-level variation
/// (CPU stock revision, memory layout, board revision, Kickstart,
/// region) is configuration; only the chipset changes the chip
/// stack's structural shape.
///
/// ECS Fat Agnus 8372A goes under [`Self::Ocs`] when paired with OCS
/// Denise: the runtime arm describes the mixed stack's structural
/// shape, not the Agnus revision in isolation. ECS Agnus 8372B / 8375
/// paired with ECS Denise 8373 lives under [`Self::Ecs`]. AGA Alice +
/// Lisa lives under [`Self::Aga`]. Vampire SAGA will land as a fourth
/// variant when implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ChipsetKind {
    /// OCS-shaped chipset stack: early OCS Agnus, or ECS 8372A in the
    /// mixed configuration, paired with OCS Denise + Paula 8364.
    /// Covers A1000, A500 (all revisions), A2000 and CDTV.
    Ocs,
    /// ECS chipset (8372B / 8375 Agnus + ECS Denise 8373 + Paula
    /// 8364). Covers A500+, A600, A3000.
    Ecs,
    /// AGA chipset (Alice + Lisa + Paula 8364). Covers A1200, A4000,
    /// CD32.
    Aga,
}

/// Catalogue CPU identity for a given Amiga model.
///
/// The catalogue keeps this broader identity separate from [`CpuModel`] so
/// profile metadata can eventually represent compatible processors outside
/// Motorola's 680x0 line, such as the Apollo AC68080. Executable Motorola
/// configurations use [`CpuConfig`], whose model is the canonical
/// [`CpuModel`] shared by the processor implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CpuKind {
    // Keep the first five variants in their original order. Serde's default
    // externally tagged binary representation encodes enum positions, so new
    // catalogue identities must be appended rather than inserted.
    /// Motorola 68000 (A1000, A500, A500+, A600, A2000, CDTV).
    M68000,
    /// Motorola 68EC020 (A1200, CD32).
    M68EC020,
    /// Motorola 68030 (A3000, A4000/030).
    M68030,
    /// Motorola 68040 (A4000/040, A4000T).
    M68040,
    /// Apollo AC68080 (Vampire FPGA — future).
    Ac68080,
    /// Motorola 68010 (supported by replacement-CPU configurations).
    M68010,
    /// Motorola 68020 (accelerators and processor replacement boards).
    M68020,
    /// Motorola 68EC030, without an on-chip MMU (GVP A530).
    M68EC030,
}

/// Processor selection and input-clock rate for one machine configuration.
///
/// The clock rate describes processor edges, not the Amiga system-tick rate
/// advertised by [`crate::profile_for`]. Keeping the two values separate is
/// required for accelerators whose CPU is asynchronous to the motherboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CpuConfig {
    model: CpuModel,
    clock_hz: u64,
}

impl CpuConfig {
    /// Construct an immutable processor configuration.
    #[must_use]
    pub const fn new(model: CpuModel, clock_hz: u64) -> Self {
        Self { model, clock_hz }
    }

    /// Configured processor model.
    #[must_use]
    pub const fn model(self) -> CpuModel {
        self.model
    }

    /// Processor input-clock rate in hertz.
    #[must_use]
    pub const fn clock_hz(self) -> u64 {
        self.clock_hz
    }
}

/// Accelerator board override layer.
///
/// Per [`amiga-machine-catalogue.md`], accelerator boards (Blizzard,
/// GVP A530, Phase 5 PPC, Apollo Vampire, PiStorm) slot in as an
/// **optional override** on top of the stock chipset + CPU. They
/// don't replace the chipset — only the CPU's bus mastery, with
/// optional Fast RAM, storage, MMU, and graphics additions.
///
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Accelerator {
    /// GVP A530 side expansion: MC68EC030 and accelerator-local RAM.
    ///
    /// The configuration records the SCSI-autoboot jumper, but the
    /// controller and boot-ROM functions are not implemented.
    GvpA530(A530Config),
    // Reserved:
    //   BlizzardII        — A500 sidecar 68030
    //   Blizzard1230      — A1200 trapdoor 68030
    //   BlizzardPpc       — A1200 trapdoor 68060 + PowerPC 603/604
    //   Vampire { … }     — Apollo FPGA AC68080 + SAGA + RTG
    //   PiStorm { … }     — Raspberry Pi-backed 68k + RTG
}

/// Canonical construction configuration for one Amiga runtime.
///
/// The value records immutable construction intent. Mutable execution state
/// such as CPU clock phase, registers, RAM contents, and Autoconfig progress
/// belongs to the machine snapshot rather than this configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AmigaConfig {
    model: Model,
    ram: RamConfig,
    cpu: CpuConfig,
    accelerator: Option<Accelerator>,
}

impl AmigaConfig {
    /// Construct a complete machine configuration.
    #[must_use]
    pub const fn new(
        model: Model,
        ram: RamConfig,
        cpu: CpuConfig,
        accelerator: Option<Accelerator>,
    ) -> Self {
        Self {
            model,
            ram,
            cpu,
            accelerator,
        }
    }

    /// Catalogue model represented by this configuration.
    #[must_use]
    pub const fn model(self) -> Model {
        self.model
    }

    /// Motherboard and generic Zorro-II RAM configuration.
    #[must_use]
    pub const fn ram(self) -> RamConfig {
        self.ram
    }

    /// Active processor configuration.
    #[must_use]
    pub const fn cpu(self) -> CpuConfig {
        self.cpu
    }

    /// Optional accelerator configuration.
    #[must_use]
    pub const fn accelerator(self) -> Option<Accelerator> {
        self.accelerator
    }

    /// Return this configuration with a caller-supplied generic RAM layout.
    ///
    /// Accelerator-local RAM is unaffected.
    #[must_use]
    pub const fn with_ram(mut self, ram: RamConfig) -> Self {
        self.ram = ram;
        self
    }
}

// ─── Hierarchical const aliases ─────────────────────────────────────
//
// Each submodule represents one machine family (per the Amiga
// rollout-plan zoo); each const is one shipping config of that
// machine. New machines / configs land here as constants rather
// than as new Rust types per the catalogue decision.

/// A1000 — the original 1985 wedge.
pub mod a1000 {
    use super::Model;
    /// A1000 OCS PAL (1985).
    pub const PAL: Model = Model::A1000OcsPal;
    /// A1000 OCS NTSC (1985, US shipping config).
    pub const NTSC: Model = Model::A1000OcsNtsc;
}

/// A500 — the 1987 home-computer mass-market Amiga.
pub mod a500 {
    use super::Model;
    /// Stock A500 OCS PAL: 512 KiB chip RAM, no trapdoor expansion.
    pub const PAL: Model = Model::A500OcsPal;
    /// Stock A500 OCS NTSC.
    pub const NTSC: Model = Model::A500OcsNtsc;
    /// A500 + A501 trapdoor: 512 KiB chip + 512 KiB slow (PAL).
    pub const A501_PAL: Model = Model::A500OcsPalA501;
    /// A500 + A501 trapdoor (NTSC).
    pub const A501_NTSC: Model = Model::A500OcsNtscA501;
    /// Maxed A500: 1 MiB chip + 512 KiB slow + 8 MiB Zorro-II fast (PAL).
    pub const MAXED_PAL: Model = Model::A500OcsPalMaxed;
    /// Maxed A500 (NTSC).
    pub const MAXED_NTSC: Model = Model::A500OcsNtscMaxed;
    /// A500 OCS PAL validation profile with a 40 MHz GVP A530.
    pub const GVP_A530_PAL: Model = Model::A500OcsPalGvpA530;
    /// A500 OCS NTSC validation profile with a 40 MHz GVP A530.
    pub const GVP_A530_NTSC: Model = Model::A500OcsNtscGvpA530;
}

/// A500+ — the 1991 ECS refresh of the A500.
pub mod a500plus {
    use super::Model;
    /// A500+ ECS PAL: 1 MiB chip RAM, ECS Agnus 8375 + ECS Denise
    /// 8373, Kickstart 2.04.
    pub const PAL: Model = Model::A500PlusEcsPal;
    /// A500+ ECS NTSC.
    pub const NTSC: Model = Model::A500PlusEcsNtsc;
}

/// A1200 — the 1992 AGA flagship of the wedge form factor.
pub mod a1200 {
    use super::Model;
    /// A1200 AGA PAL: 68EC020, Alice + Lisa chipset, Gayle IDE,
    /// 2 MiB chip RAM, Kickstart 3.0 / 3.1.
    pub const PAL: Model = Model::A1200AgaPal;
    /// A1200 AGA NTSC.
    pub const NTSC: Model = Model::A1200AgaNtsc;
}

/// A600 — the 1992 ECS refresh in a smaller form factor than the A500.
pub mod a600 {
    use super::Model;
    /// A600 ECS PAL: 68000, ECS Agnus 8375 + ECS Denise 8373, Gayle
    /// (IDE + PCMCIA decode), 1 MiB chip RAM, Kickstart 2.05.
    pub const PAL: Model = Model::A600EcsPal;
    /// A600 ECS NTSC.
    pub const NTSC: Model = Model::A600EcsNtsc;
}

/// A2000 — the 1987 expandable tower (Rev B with Fat Agnus 8372A
/// is the canonical config we catalogue).
pub mod a2000 {
    use super::Model;
    /// A2000 Rev B OCS PAL: 68000, Fat Agnus 8372A + OCS Denise,
    /// 1 MiB chip RAM, Kickstart 1.3 / 2.04, Zorro-II slots.
    pub const PAL: Model = Model::A2000OcsPal;
    /// A2000 Rev B OCS NTSC.
    pub const NTSC: Model = Model::A2000OcsNtsc;
}

// ─── Unimplemented families ────────────────────────────────────────
//
// Remaining rollout-plan families include CDTV (OCS + CD peripheral),
// A4000/030 (AGA + 68030), CD32 (AGA + Akiko), A3000 (ECS + 68030),
// and A4000/040. These comments reserve catalogue shape only; they do
// not create supported profiles. When each machine's required chip stack
// and board devices land, its constants are added here as new submodules:
//
//   pub mod cdtv     { pub const PAL: Model = …; pub const NTSC: Model = …; }
//   pub mod a4000    { pub const A030_PAL: Model = …; pub const A040_PAL: Model = …; }
//   pub mod cd32     { pub const PAL: Model = …; pub const NTSC: Model = …; }
//   pub mod a3000    { pub const DESKTOP_PAL: Model = …; pub const TOWER_PAL: Model = …; }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchical_aliases_round_trip_to_flat_model() {
        assert_eq!(a1000::PAL, Model::A1000OcsPal);
        assert_eq!(a1000::NTSC, Model::A1000OcsNtsc);
        assert_eq!(a500::PAL, Model::A500OcsPal);
        assert_eq!(a500::A501_PAL, Model::A500OcsPalA501);
        assert_eq!(a500::MAXED_NTSC, Model::A500OcsNtscMaxed);
        assert_eq!(a500::GVP_A530_PAL, Model::A500OcsPalGvpA530);
        assert_eq!(a500::GVP_A530_NTSC, Model::A500OcsNtscGvpA530);
        assert_eq!(a500plus::PAL, Model::A500PlusEcsPal);
        assert_eq!(a600::PAL, Model::A600EcsPal);
        assert_eq!(a600::NTSC, Model::A600EcsNtsc);
        assert_eq!(a1200::PAL, Model::A1200AgaPal);
        assert_eq!(a1200::NTSC, Model::A1200AgaNtsc);
        assert_eq!(a2000::PAL, Model::A2000OcsPal);
        assert_eq!(a2000::NTSC, Model::A2000OcsNtsc);
    }

    #[test]
    fn chipset_kind_matches_flat_model_predicates() {
        assert_eq!(a1000::PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a1000::NTSC.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500::PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500::A501_PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500::MAXED_NTSC.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500::GVP_A530_PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500plus::PAL.chipset(), ChipsetKind::Ecs);
        assert_eq!(a500plus::NTSC.chipset(), ChipsetKind::Ecs);
        assert_eq!(a600::PAL.chipset(), ChipsetKind::Ecs);
        assert_eq!(a600::NTSC.chipset(), ChipsetKind::Ecs);
        assert_eq!(a1200::PAL.chipset(), ChipsetKind::Aga);
        assert_eq!(a1200::NTSC.chipset(), ChipsetKind::Aga);
        assert_eq!(a2000::PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a2000::NTSC.chipset(), ChipsetKind::Ocs);
    }

    #[test]
    fn cpu_kind_partitions_by_chipset() {
        // Stock OCS + ECS models use a 68000; AGA models use a 68EC020.
        // Accelerator validation profiles report their active CPU.
        // As A3000 / A4000 land (still under ECS / AGA but with 68030
        // / 68040 stock CPUs), this test grows.
        for model in [
            a1000::PAL,
            a1000::NTSC,
            a500::PAL,
            a500::NTSC,
            a500::A501_PAL,
            a500::A501_NTSC,
            a500::MAXED_PAL,
            a500::MAXED_NTSC,
            a500plus::PAL,
            a500plus::NTSC,
            a600::PAL,
            a600::NTSC,
            a2000::PAL,
            a2000::NTSC,
        ] {
            assert_eq!(model.cpu(), CpuKind::M68000, "{model:?} should be 68000");
        }
        for model in [a1200::PAL, a1200::NTSC] {
            assert_eq!(
                model.cpu(),
                CpuKind::M68EC020,
                "{model:?} should be 68EC020"
            );
        }
        for model in [a500::GVP_A530_PAL, a500::GVP_A530_NTSC] {
            assert_eq!(
                model.cpu(),
                CpuKind::M68EC030,
                "{model:?} should be 68EC030"
            );
        }
    }

    #[test]
    fn a530_validation_configuration_is_explicit_and_non_factory() {
        let config = a500::GVP_A530_PAL.config();
        assert_eq!(config.ram(), machine_commodore_amiga_ocs::RamConfig::bare());
        assert_eq!(config.cpu(), CpuConfig::new(CpuModel::M68EC030, 40_000_000));
        let Some(Accelerator::GvpA530(a530)) = config.accelerator() else {
            panic!("A530 profile must carry its accelerator configuration");
        };
        assert_eq!(a530.ram_size().kib(), 1024);
        assert!(!a530.cache_enabled());
        assert!(!a530.autoboot_enabled());
    }

    #[test]
    fn cpu_kind_preserves_original_serialized_discriminants() {
        for (kind, expected) in [
            (CpuKind::M68000, 0),
            (CpuKind::M68EC020, 1),
            (CpuKind::M68030, 2),
            (CpuKind::M68040, 3),
            (CpuKind::Ac68080, 4),
            (CpuKind::M68010, 5),
            (CpuKind::M68020, 6),
            (CpuKind::M68EC030, 7),
        ] {
            assert_eq!(
                postcard::to_allocvec(&kind).expect("serialize CPU kind"),
                vec![expected],
                "{kind:?} must retain its append-only binary discriminant"
            );
        }
    }
}
