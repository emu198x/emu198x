//! CPU model/capability definitions for the Motorola 68k family.
//!
//! This crate currently implements 68000 execution semantics, but the model
//! metadata lets us gate decode/execute behavior as support for later CPUs is
//! introduced.

/// Selected Motorola 68k CPU model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuModel {
    /// Motorola MC68000.
    M68000,
    /// Motorola MC68010.
    M68010,
    /// Motorola MC68020.
    M68020,
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
}

impl CpuModel {
    /// Static capability set for this CPU model.
    #[must_use]
    pub const fn capabilities(self) -> CpuCapabilities {
        match self {
            Self::M68000 => CpuCapabilities {
                movec: false,
                vbr: false,
                cacr: false,
            },
            Self::M68010 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: false,
            },
            Self::M68020 => CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true,
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
    use super::{CpuCapabilities, CpuModel};

    #[test]
    fn capabilities_match_expected_baseline_models() {
        assert_eq!(
            CpuModel::M68000.capabilities(),
            CpuCapabilities {
                movec: false,
                vbr: false,
                cacr: false
            }
        );
        assert_eq!(
            CpuModel::M68010.capabilities(),
            CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: false
            }
        );
        assert_eq!(
            CpuModel::M68020.capabilities(),
            CpuCapabilities {
                movec: true,
                vbr: true,
                cacr: true
            }
        );
    }
}
