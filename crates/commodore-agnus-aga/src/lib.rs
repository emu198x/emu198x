//! Commodore Alice (AGA Agnus) — wraps ECS Agnus with AGA DMA extensions.
//!
//! Alice adds wider DMA fetches (FMODE) and 8-bitplane lowres support to the
//! existing ECS/OCS bus arbitration. This crate follows the same Deref
//! composition pattern as `commodore-agnus-ecs`.

use std::ops::{Deref, DerefMut};

pub use commodore_agnus_ecs::AgnusEcs as InnerAgnusEcs;
pub use commodore_agnus_ecs::{
    BlitterDmaOp, CckBusPlan, Copper, CopperState, HIRES_DDF_TO_PLANE, LOWRES_DDF_TO_PLANE,
    PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME, PaulaReturnProgressPolicy, SlotOwner,
};

// Re-export ECS constants for machine-amiga convenience.
pub use commodore_agnus_ecs::{
    BEAMCON0_BLANKEN, BEAMCON0_CSCBEN, BEAMCON0_CSYTRUE, BEAMCON0_HARDDIS, BEAMCON0_HSYTRUE,
    BEAMCON0_VARBEAMEN, BEAMCON0_VARCSYEN, BEAMCON0_VARHSYEN, BEAMCON0_VARVBEN,
    BEAMCON0_VARVSYEN, BEAMCON0_VSYTRUE,
};

/// AGA lowres bitplane fetch order: adds BPL7 and BPL8 to the two free slots.
pub const LOWRES_DDF_TO_PLANE_AGA: [Option<u8>; 8] = [
    Some(6), // 0: BPL7 (was free in OCS)
    Some(3), // 1: BPL4
    Some(5), // 2: BPL6
    Some(1), // 3: BPL2
    Some(7), // 4: BPL8 (was free in OCS)
    Some(2), // 5: BPL3
    Some(4), // 6: BPL5
    Some(0), // 7: BPL1 (triggers shift register load)
];

/// AGA Alice wrapper around the ECS Agnus core.
#[derive(Clone)]
pub struct AgnusAga {
    inner: InnerAgnusEcs,
}

impl AgnusAga {
    /// Create a new AGA Agnus wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerAgnusEcs::new(),
        }
    }

    /// Wrap an existing ECS Agnus core.
    #[must_use]
    pub fn from_ecs(inner: InnerAgnusEcs) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped ECS Agnus core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerAgnusEcs {
        &self.inner
    }

    /// Mutably borrow the wrapped ECS Agnus core.
    #[must_use]
    pub fn as_inner_mut(&mut self) -> &mut InnerAgnusEcs {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped ECS Agnus core.
    #[must_use]
    pub fn into_inner(self) -> InnerAgnusEcs {
        self.inner
    }

    /// Bitplane DMA fetch width based on FMODE bits 1-0.
    ///
    /// Returns 1 (16-bit), 2 (32-bit), or 4 (64-bit). For OCS/ECS configs
    /// FMODE is always 0, so this returns 1.
    #[must_use]
    pub fn bpl_fetch_width(&self) -> u8 {
        match self.fmode & 3 {
            0 => 1,
            1 | 2 => 2,
            3 => 4,
            _ => unreachable!(),
        }
    }

    /// Sprite DMA fetch width based on FMODE bits 3-2.
    ///
    /// Returns 1 (16-bit), 2 (32-bit), or 4 (64-bit). For OCS/ECS configs
    /// FMODE is always 0, so this returns 1.
    #[must_use]
    pub fn spr_fetch_width(&self) -> u8 {
        match (self.fmode >> 2) & 3 {
            0 => 1,
            1 | 2 => 2,
            3 => 4,
            _ => unreachable!(),
        }
    }

    /// AGA-aware bus plan that adds BPL7/BPL8 lowres slots when >6 planes active.
    ///
    /// For OCS/ECS (max_bitplanes=6) or hires, delegates entirely to the ECS plan.
    /// In AGA lowres with 7-8 planes, the two free slots in the 8-CCK group
    /// (positions 0 and 4) become BPL7 and BPL8 fetches.
    #[must_use]
    pub fn cck_bus_plan(&self) -> CckBusPlan {
        let mut plan = self.inner.cck_bus_plan();

        // Only patch when AGA lowres with >6 planes and the slot was not
        // already assigned to a bitplane.
        let num_bpl = self.num_bitplanes();
        if num_bpl <= 6 || plan.bitplane_dma_fetch_plane.is_some() {
            return plan;
        }

        // Check if we're in the bitplane DMA fetch window and in lowres.
        let hires = (self.bplcon0 & 0x8000) != 0;
        if hires || !self.dma_enabled(0x0100) {
            return plan;
        }

        let ddfstrt = self.ddfstrt;
        let ddfstop = self.ddfstop;
        let hpos = self.hpos;
        if hpos < ddfstrt {
            return plan;
        }

        let fetchunit: u32 = 8;
        let ddf_span = u32::from(ddfstop.saturating_sub(ddfstrt));
        let blocks = (ddf_span + fetchunit - 1) / fetchunit + 1;
        let fetch_window_end = u32::from(ddfstrt) + blocks * fetchunit - 1;
        if u32::from(hpos) > fetch_window_end {
            return plan;
        }

        let pos_in_group = ((hpos - ddfstrt) % 8) as usize;
        if let Some(plane) = LOWRES_DDF_TO_PLANE_AGA[pos_in_group].filter(|&p| p < num_bpl) {
            // OCS LOWRES_DDF_TO_PLANE had None at positions 0 and 4;
            // AGA fills them with BPL7 (6) and BPL8 (7).
            if plane >= 6 {
                plan.slot_owner = SlotOwner::Bitplane(plane);
                plan.bitplane_dma_fetch_plane = Some(plane);
                plan.copper_dma_slot_granted = false;
                plan.cpu_chip_bus_granted = false;
                plan.blitter_chip_bus_granted = false;
                plan.blitter_dma_progress_granted = false;
                plan.paula_return_progress_policy = PaulaReturnProgressPolicy::Stall;
            }
        }

        plan
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
