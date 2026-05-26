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

/// Chipset axis discriminant.
///
/// The only structural axis on `AmigaRuntimeKind`. Per
/// [`amiga-machine-catalogue.md`], machine-level variation
/// (CPU stock revision, memory layout, board revision, Kickstart,
/// region) is configuration; only the chipset changes the chip
/// stack's structural shape.
///
/// Fat Agnus 8372A goes under [`Self::Ocs`] — it is paired with OCS
/// Denise, so the chip-stack shape is OCS even though the chip-RAM
/// ceiling moves to 1 MB. ECS Agnus 8372B / 8375 paired with ECS
/// Denise 8373 lives under [`Self::Ecs`]. AGA Alice + Lisa under
/// [`Self::Aga`]. Vampire SAGA will land as a fourth variant when
/// implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ChipsetKind {
    /// OCS chipset (8361 / 8367 / 8370 / 8371 / 8372A Agnus + OCS
    /// Denise + Paula 8364). Covers A1000, A500 (all revisions),
    /// A2000, CDTV.
    Ocs,
    /// ECS chipset (8372B / 8375 Agnus + ECS Denise 8373 + Paula
    /// 8364). Covers A500+, A600, A3000.
    Ecs,
    /// AGA chipset (Alice + Lisa + Paula 8364). Covers A1200, A4000,
    /// CD32.
    Aga,
}

/// Stock CPU type for a given Amiga model.
///
/// Per [`amiga-machine-catalogue.md`]:
///
/// > Stock CPU is hardcoded per chipset (`Cpu68000` for OCS / ECS,
/// > `Cpu68EC020` for AGA); accelerator boards layer on top as
/// > `Option<Accelerator>`.
///
/// The enum exists so curriculum tools and profile metadata can
/// surface "this machine ships with a 68000" without reaching into
/// the chip-stack core. Per-instruction dispatch in the runtime
/// does **not** match on this enum — the stock CPU type is fixed
/// statically per chipset core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CpuKind {
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
}

/// Accelerator board override layer.
///
/// Per [`amiga-machine-catalogue.md`], accelerator boards (Blizzard,
/// GVP A530, Phase 5 PPC, Apollo Vampire, PiStorm) slot in as an
/// **optional override** on top of the stock chipset + CPU. They
/// don't replace the chipset — only the CPU's bus mastery, with
/// optional Fast RAM, storage, MMU, and graphics additions.
///
/// **No variants are implemented today.** The enum exists so the
/// bus-dispatch hook can be locked in ahead of the first accelerator
/// implementation (Vampire AC68080 or PiStorm work). When the first
/// accelerator lands, its variant is added here and the bus
/// dispatch in the chip cores starts checking
/// `Option<Accelerator>::Some(...)` for an override CPU + memory
/// map.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Accelerator {
    // Reserved:
    //   GvpA530           — A500 trapdoor 68030 + SCSI + RAM
    //   BlizzardII        — A500 sidecar 68030
    //   Blizzard1230      — A1200 trapdoor 68030
    //   BlizzardPpc       — A1200 trapdoor 68060 + PowerPC 603/604
    //   Vampire { … }     — Apollo FPGA AC68080 + SAGA + RTG
    //   PiStorm { … }     — Raspberry Pi-backed 68k + RTG
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

// ─── Future families ────────────────────────────────────────────────
//
// Per the rollout plan, the next machines to land are A1200 (AGA),
// A600 (ECS, reuses Gayle from A1200 extraction), CDTV (OCS + CD
// peripheral), A4000/030 (AGA + 68030), CD32 (AGA + Akiko),
// A3000 (ECS + 68030), and A4000/040. When each machine's chip
// stack lands, its constants are added here as new submodules:
//
//   pub mod a1200    { pub const PAL: Model = …; pub const NTSC: Model = …; }
//   pub mod a600     { pub const PAL: Model = …; pub const HD_PAL: Model = …; }
//   pub mod cdtv     { pub const PAL: Model = …; pub const NTSC: Model = …; }
//   pub mod a4000    { pub const A030_PAL: Model = …; pub const A040_PAL: Model = …; }
//   pub mod cd32     { pub const PAL: Model = …; pub const NTSC: Model = …; }
//   pub mod a3000    { pub const DESKTOP_PAL: Model = …; pub const TOWER_PAL: Model = …; }
//   pub mod a2000    { pub const REV_A_PAL: Model = …; pub const REV_B_PAL: Model = …; }

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
        assert_eq!(a500plus::PAL, Model::A500PlusEcsPal);
        assert_eq!(a1200::PAL, Model::A1200AgaPal);
        assert_eq!(a1200::NTSC, Model::A1200AgaNtsc);
    }

    #[test]
    fn chipset_kind_matches_flat_model_predicates() {
        assert_eq!(a1000::PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a1000::NTSC.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500::PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500::A501_PAL.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500::MAXED_NTSC.chipset(), ChipsetKind::Ocs);
        assert_eq!(a500plus::PAL.chipset(), ChipsetKind::Ecs);
        assert_eq!(a500plus::NTSC.chipset(), ChipsetKind::Ecs);
        assert_eq!(a1200::PAL.chipset(), ChipsetKind::Aga);
        assert_eq!(a1200::NTSC.chipset(), ChipsetKind::Aga);
    }

    #[test]
    fn cpu_kind_partitions_by_chipset() {
        // OCS + ECS models ship stock 68000; AGA models ship 68EC020.
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
        ] {
            assert_eq!(model.cpu(), CpuKind::M68000, "{model:?} should be 68000");
        }
        for model in [a1200::PAL, a1200::NTSC] {
            assert_eq!(model.cpu(), CpuKind::M68EC020, "{model:?} should be 68EC020");
        }
    }

    #[test]
    fn accelerator_enum_has_no_variants_yet() {
        // The Accelerator type exists as a future-proofing axis but
        // no variants are implemented. Once a variant lands, this
        // test is replaced with a positive existence check.
        //
        // `Option<Accelerator>` is always `None` today because the
        // empty enum can't be constructed; this is asserted via the
        // type-level invariant rather than a runtime check.
        fn _accelerator_is_uninhabited(a: Accelerator) -> ! {
            match a {}
        }
    }
}
