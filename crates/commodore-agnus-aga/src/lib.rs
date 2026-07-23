//! Commodore Alice (AGA Agnus) — thin wrapper over the ECS Agnus that
//! adds the AGA-only `FMODE` register and the bus-plan extensions for
//! 8-bitplane lowres mode.
//!
//! Alice's silicon-level deltas vs ECS Agnus 8375:
//!
//! - **FMODE** ($1FC) controls 16 / 32 / 64-bit DMA fetch widths for
//!   bitplane + sprite data. OCS / ECS hard-code these to 16-bit.
//! - **8 bitplanes in lowres** (vs ECS 6). The DDF fetch table has two
//!   slots free in OCS/ECS lowres that Alice fills with BPL7 / BPL8.
//! - **Wider sprite DMA** when FMODE bits 3..2 are non-zero.
//!
//! All ECS-and-below behaviour is delegated through `Deref` to
//! `AgnusEcs`. Only AGA-specific state lives on this wrapper.
//!
//! FMODE drives real fetch behaviour: bitplane DMA fetches 1/2/4 words
//! per slot at 16/32/64-bit widths (validated against a live Workbench
//! 3.1 modulo oracle), sprite DMA widens to match (`spr_fetch_width`,
//! #95/#99), and the SHRES column of the fetch cadence is selected for
//! superhires (#469). The 8-plane lowres fetch order fills the two slots
//! OCS/ECS leave idle (#99). The `fmode` register lives on this wrapper
//! (silicon-correct) and is propagated onto the inner OCS Agnus, whose
//! shared fetch loop reads it.
//!
//! Adapted from `Emu198x-Oldest/crates/commodore-agnus-aga/` — the
//! donor crate's structure carried over directly, but `fmode` moved
//! from the OCS Agnus (incorrectly chipset-layered in the donor) onto
//! this wrapper where it belongs by silicon.

use std::ops::{Deref, DerefMut};

pub use commodore_agnus_ecs::{
    AgnusEcs as InnerAgnusEcs, BEAMCON0_BLANKEN, BEAMCON0_CSCBEN, BEAMCON0_CSYTRUE, BEAMCON0_DUAL,
    BEAMCON0_HARDDIS, BEAMCON0_HSYTRUE, BEAMCON0_LOLDIS, BEAMCON0_LPENDIS, BEAMCON0_PAL,
    BEAMCON0_VARBEAMEN, BEAMCON0_VARCSYEN, BEAMCON0_VARHSYEN, BEAMCON0_VARVBEN, BEAMCON0_VARVSYEN,
    BEAMCON0_VSYTRUE, BlitterDmaOp, CckBusPlan, Copper, CopperState, HIRES_DDF_TO_PLANE,
    LOWRES_DDF_TO_PLANE, LOWRES_DDF_TO_PLANE_AGA, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME,
    PaulaReturnProgressPolicy, SlotOwner,
};

/// AGA Alice — wraps `AgnusEcs` with the FMODE register and AGA-only
/// bus arbitration extensions.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AgnusAga {
    inner: InnerAgnusEcs,
    /// FMODE ($1FC). Bits 1..0 select bitplane DMA fetch width;
    /// bits 3..2 select sprite DMA fetch width. Bit 14 (BPL32) and
    /// bit 15 (BSCAN2) carry additional AGA-only configuration.
    pub fmode: u16,
}

impl AgnusAga {
    /// Construct a fresh Alice with FMODE cleared (default to 16-bit
    /// fetches, matching OCS / ECS behaviour until KS writes FMODE).
    #[must_use]
    pub fn new() -> Self {
        let mut inner = InnerAgnusEcs::new();
        inner.max_bitplanes = 8;
        Self { inner, fmode: 0 }
    }

    /// Promote an existing ECS Agnus to AGA Alice. Carries inner state
    /// across, raises the hardware bitplane capacity to eight, and starts
    /// FMODE at 0.
    #[must_use]
    pub fn from_ecs(mut inner: InnerAgnusEcs) -> Self {
        inner.max_bitplanes = 8;
        Self { inner, fmode: 0 }
    }

    /// Borrow the wrapped ECS Agnus core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerAgnusEcs {
        &self.inner
    }

    /// Mutably borrow the wrapped ECS Agnus core.
    pub fn as_inner_mut(&mut self) -> &mut InnerAgnusEcs {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped ECS Agnus core.
    #[must_use]
    pub fn into_inner(self) -> InnerAgnusEcs {
        self.inner
    }

    /// Bitplane DMA fetch width in 16-bit words, derived from FMODE
    /// bits 1..0. Returns 1 (16-bit), 2 (32-bit), or 4 (64-bit).
    #[must_use]
    pub const fn bpl_fetch_width(&self) -> u8 {
        match self.fmode & 0x0003 {
            0 => 1,
            1 | 2 => 2,
            _ => 4,
        }
    }

    /// Sprite DMA fetch width in 16-bit words, derived from FMODE
    /// bits 3..2. Returns 1 (16-bit), 2 (32-bit), or 4 (64-bit).
    #[must_use]
    pub const fn spr_fetch_width(&self) -> u8 {
        match (self.fmode >> 2) & 0x0003 {
            0 => 1,
            1 | 2 => 2,
            _ => 4,
        }
    }
}

impl Default for AgnusAga {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for AgnusAga {
    type Target = InnerAgnusEcs;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for AgnusAga {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<AgnusAga> for InnerAgnusEcs {
    fn from(agnus: AgnusAga) -> Self {
        agnus.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::{AgnusAga, InnerAgnusEcs};

    #[test]
    fn new_starts_with_fmode_cleared() {
        let agnus = AgnusAga::new();
        assert_eq!(agnus.fmode, 0);
        assert_eq!(agnus.bpl_fetch_width(), 1);
        assert_eq!(agnus.spr_fetch_width(), 1);
    }

    #[test]
    fn bpl_fetch_width_decodes_fmode_low_bits() {
        let mut agnus = AgnusAga::new();
        for (fmode, expected) in [(0x0000, 1), (0x0001, 2), (0x0002, 2), (0x0003, 4)] {
            agnus.fmode = fmode;
            assert_eq!(agnus.bpl_fetch_width(), expected, "FMODE={fmode:#06x}");
        }
    }

    #[test]
    fn spr_fetch_width_decodes_fmode_upper_bits() {
        let mut agnus = AgnusAga::new();
        for (fmode, expected) in [(0x0000, 1), (0x0004, 2), (0x0008, 2), (0x000C, 4)] {
            agnus.fmode = fmode;
            assert_eq!(agnus.spr_fetch_width(), expected, "FMODE={fmode:#06x}");
        }
    }

    #[test]
    fn new_supports_eight_bitplanes() {
        let mut agnus = AgnusAga::new();
        agnus.bplcon0 = 0x0010; // BPU3 selects eight planes on AGA.

        assert_eq!(agnus.max_bitplanes, 8);
        assert_eq!(agnus.num_bitplanes(), 8);
    }

    #[test]
    fn from_ecs_promotes_bitplane_capacity_to_eight() {
        let mut ecs = InnerAgnusEcs::new();
        ecs.max_bitplanes = 6;

        let mut agnus = AgnusAga::from_ecs(ecs);
        agnus.bplcon0 = 0x0010;

        assert_eq!(agnus.max_bitplanes, 8);
        assert_eq!(agnus.num_bitplanes(), 8);
    }

    #[test]
    fn deref_exposes_inner_ecs_registers() {
        let mut agnus = AgnusAga::new();
        // The wrapped ECS Agnus carries dmacon / bplcon0 / max_bitplanes
        // through to OCS Agnus. Deref should let us reach them.
        agnus.dmacon = 0x0200;
        agnus.bplcon0 = 0x0010;
        assert_eq!(agnus.dmacon, 0x0200);
        assert_eq!(agnus.bplcon0, 0x0010);
        assert_eq!(agnus.max_bitplanes, 8);
    }
}
