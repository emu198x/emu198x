//! CPU model/capability definitions for the Motorola 68k family.
//!
//! Each model in the 680x0 family has different capabilities (FPU, MMU,
//! caches) and instruction timing characteristics. The [`TimingClass`]
//! groups models that share the same instruction execution timing, while
//! [`CpuCapabilities`] tracks which optional features are present.

/// Selected Motorola 680x0 CPU model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuModel {
    /// MC68000 — original 16/32-bit CPU.
    M68000,
    /// MC68010 — adds VBR, loop mode.
    M68010,
    /// MC68EC020 — 32-bit, no FPU, no MMU (A1200, CD32).
    M68EC020,
    /// MC68020 — full 32-bit (FPU + MMU coprocessor interface).
    M68020,
    /// MC68EC030 — no FPU, no MMU.
    M68EC030,
    /// MC68LC030 — no FPU, has on-chip MMU.
    M68LC030,
    /// MC68030 — full (FPU coprocessor interface + on-chip MMU).
    M68030,
    /// MC68EC040 — no FPU, no MMU.
    M68EC040,
    /// MC68LC040 — no FPU, has MMU.
    M68LC040,
    /// MC68040 — full (on-chip FPU + MMU).
    M68040,
    /// MC68EC060 — no FPU, no MMU.
    M68EC060,
    /// MC68LC060 — no FPU, has MMU.
    M68LC060,
    /// MC68060 — full (on-chip FPU + MMU, superscalar).
    M68060,
}

/// Instruction timing class. Models within a class share the same
/// instruction execution timing (clock counts), even though they may
/// differ in FPU/MMU availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingClass {
    /// 68000/68010: 4-clock minimum bus cycle, 16-bit ALU, no pipeline.
    M68000,
    /// 68020/68030: 3-clock bus, 32-bit ALU, instruction pipeline.
    M68020,
    /// 68040: deeper pipeline, on-chip FPU, 1-clock effective bus.
    M68040,
    /// 68060: superscalar, branch prediction.
    M68060,
}

/// Capability flags for a specific CPU model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuCapabilities {
    /// `MOVEC` instruction family is available.
    pub movec: bool,
    /// Vector Base Register (`VBR`) is present.
    pub vbr: bool,
    /// Cache control register (`CACR`) is present.
    pub cacr: bool,
    /// 32-bit multiply/divide (`MULL`/`DIVL`) instructions.
    pub mull_divl: bool,
    /// `EXTB.L` sign-extend byte to long.
    pub extb_l: bool,
    /// Bit field instructions (`BFTST`/`BFEXTU`/`BFEXTS`/`BFINS`/`BFSET`/`BFCLR`/`BFCHG`/`BFFFO`).
    pub bitfield: bool,
    /// `CAS`/`CAS2` compare-and-swap instructions.
    pub cas: bool,
    /// FPU instructions available (on-chip or coprocessor interface).
    pub fpu: bool,
    /// MMU (PMMU/ATC) available.
    pub mmu: bool,
    /// Scaled index (×1/×2/×4/×8) in brief extension word (68020+).
    /// On 68000/68010, bits 9-10 of the extension word are "don't care".
    pub scaled_index: bool,
    /// Barrel shifter: shifts execute in constant time (68020+).
    pub barrel_shifter: bool,
    /// On-chip instruction cache (68020+).
    pub instruction_cache: bool,
    /// On-chip data cache (68030+).
    pub data_cache: bool,
    /// Burst fill for cache lines (68030+).
    pub burst_mode: bool,
}

impl CpuModel {
    /// Instruction timing class for this model.
    #[must_use]
    pub const fn timing_class(self) -> TimingClass {
        match self {
            Self::M68000 | Self::M68010 => TimingClass::M68000,
            Self::M68EC020 | Self::M68020 | Self::M68EC030 | Self::M68LC030 | Self::M68030 => {
                TimingClass::M68020
            }
            Self::M68EC040 | Self::M68LC040 | Self::M68040 => TimingClass::M68040,
            Self::M68EC060 | Self::M68LC060 | Self::M68060 => TimingClass::M68060,
        }
    }

    /// Static capability set for this CPU model.
    #[must_use]
    pub const fn capabilities(self) -> CpuCapabilities {
        // Base 68000 — everything off
        const BASE: CpuCapabilities = CpuCapabilities {
            movec: false,
            vbr: false,
            cacr: false,
            mull_divl: false,
            extb_l: false,
            bitfield: false,
            cas: false,
            fpu: false,
            mmu: false,
            scaled_index: false,
            barrel_shifter: false,
            instruction_cache: false,
            data_cache: false,
            burst_mode: false,
        };

        match self {
            Self::M68000 => BASE,

            Self::M68010 => CpuCapabilities {
                movec: true,
                vbr: true,
                ..BASE
            },

            // 68020 family: 32-bit ISA extensions, barrel shifter, I-cache
            Self::M68EC020 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                ..BASE
            },
            Self::M68020 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                fpu: true,
                mmu: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                ..BASE
            },

            // 68030 family: adds data cache, burst mode, on-chip MMU
            Self::M68EC030 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },
            Self::M68LC030 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                mmu: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },
            Self::M68030 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                fpu: true,
                mmu: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },

            // 68040 family: deeper pipeline, on-chip FPU
            Self::M68EC040 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },
            Self::M68LC040 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                mmu: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },
            Self::M68040 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                fpu: true,
                mmu: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },

            // 68060 family: superscalar
            Self::M68EC060 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },
            Self::M68LC060 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                mmu: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },
            Self::M68060 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
                mull_divl: true,
                extb_l: true,
                bitfield: true,
                cas: true,
                fpu: true,
                mmu: true,
                scaled_index: true,
                barrel_shifter: true,
                instruction_cache: true,
                data_cache: true,
                burst_mode: true,
                ..BASE
            },
        }
    }

    /// Convenience helper for decode gating.
    #[must_use]
    pub const fn supports_movec(self) -> bool {
        self.capabilities().movec
    }
}

#[cfg(test)]
mod tests {
    use super::{CpuCapabilities, CpuModel, TimingClass};

    #[test]
    fn timing_classes() {
        assert_eq!(CpuModel::M68000.timing_class(), TimingClass::M68000);
        assert_eq!(CpuModel::M68010.timing_class(), TimingClass::M68000);
        assert_eq!(CpuModel::M68EC020.timing_class(), TimingClass::M68020);
        assert_eq!(CpuModel::M68020.timing_class(), TimingClass::M68020);
        assert_eq!(CpuModel::M68EC030.timing_class(), TimingClass::M68020);
        assert_eq!(CpuModel::M68LC030.timing_class(), TimingClass::M68020);
        assert_eq!(CpuModel::M68030.timing_class(), TimingClass::M68020);
        assert_eq!(CpuModel::M68EC040.timing_class(), TimingClass::M68040);
        assert_eq!(CpuModel::M68LC040.timing_class(), TimingClass::M68040);
        assert_eq!(CpuModel::M68040.timing_class(), TimingClass::M68040);
        assert_eq!(CpuModel::M68EC060.timing_class(), TimingClass::M68060);
        assert_eq!(CpuModel::M68LC060.timing_class(), TimingClass::M68060);
        assert_eq!(CpuModel::M68060.timing_class(), TimingClass::M68060);
    }

    #[test]
    fn m68000_has_no_extensions() {
        let c = CpuModel::M68000.capabilities();
        assert!(!c.movec);
        assert!(!c.fpu);
        assert!(!c.mmu);
        assert!(!c.barrel_shifter);
        assert!(!c.instruction_cache);
    }

    #[test]
    fn m68ec020_has_no_fpu_or_mmu() {
        let c = CpuModel::M68EC020.capabilities();
        assert!(c.mull_divl);
        assert!(c.bitfield);
        assert!(c.barrel_shifter);
        assert!(c.instruction_cache);
        assert!(!c.fpu);
        assert!(!c.mmu);
        assert!(!c.data_cache);
    }

    #[test]
    fn m68020_has_fpu_and_mmu() {
        let c = CpuModel::M68020.capabilities();
        assert!(c.fpu);
        assert!(c.mmu);
    }

    #[test]
    fn m68030_has_data_cache_and_burst() {
        let c = CpuModel::M68030.capabilities();
        assert!(c.data_cache);
        assert!(c.burst_mode);
        assert!(c.fpu);
        assert!(c.mmu);
    }

    #[test]
    fn ec_lc_variants_differ_only_in_fpu_mmu() {
        let ec = CpuModel::M68EC030.capabilities();
        let lc = CpuModel::M68LC030.capabilities();
        let full = CpuModel::M68030.capabilities();

        assert!(!ec.fpu && !ec.mmu);
        assert!(!lc.fpu && lc.mmu);
        assert!(full.fpu && full.mmu);

        // All share the same ISA extensions
        assert_eq!(ec.mull_divl, lc.mull_divl);
        assert_eq!(lc.mull_divl, full.mull_divl);
        assert_eq!(ec.barrel_shifter, full.barrel_shifter);
    }
}
