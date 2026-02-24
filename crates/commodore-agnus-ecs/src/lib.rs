//! Thin Commodore Super Agnus (ECS) wrapper crate.
//!
//! This crate starts the ECS path as a composition layer over the existing OCS
//! Agnus implementation. It preserves current behavior while giving us a place
//! to add ECS-specific DMA/register/timing deltas incrementally.

use std::ops::{Deref, DerefMut};

pub use commodore_agnus_ocs::Agnus as InnerAgnusOcs;
pub use commodore_agnus_ocs::{
    BlitterDmaOp, CckBusPlan, Copper, CopperState, LOWRES_DDF_TO_PLANE, PAL_CCKS_PER_LINE,
    PAL_LINES_PER_FRAME, PaulaReturnProgressPolicy, SlotOwner,
};

/// Thin ECS wrapper that currently reuses the OCS Agnus implementation.
pub struct AgnusEcs {
    inner: InnerAgnusOcs,
    beamcon0: u16,
    diwhigh: u16,
}

impl AgnusEcs {
    /// Create a new ECS Agnus wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerAgnusOcs::new(),
            beamcon0: 0,
            diwhigh: 0,
        }
    }

    /// Wrap an existing OCS Agnus core while starting ECS extension registers
    /// from reset state. Useful for behavior-identical OCS paths that route
    /// through the ECS wrapper constructor during Phase 3 bring-up.
    #[must_use]
    pub fn from_ocs(inner: InnerAgnusOcs) -> Self {
        Self {
            inner,
            beamcon0: 0,
            diwhigh: 0,
        }
    }

    /// Borrow the wrapped OCS Agnus core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerAgnusOcs {
        &self.inner
    }

    /// Mutably borrow the wrapped OCS Agnus core.
    #[must_use]
    pub fn as_inner_mut(&mut self) -> &mut InnerAgnusOcs {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped OCS Agnus core.
    #[must_use]
    pub fn into_inner(self) -> InnerAgnusOcs {
        self.inner
    }

    /// ECS `BEAMCON0` latch (register semantics are not fully modeled yet).
    #[must_use]
    pub const fn beamcon0(&self) -> u16 {
        self.beamcon0
    }

    /// Store ECS `BEAMCON0` for later timing/beam model work.
    pub fn write_beamcon0(&mut self, val: u16) {
        self.beamcon0 = val;
    }

    /// ECS `DIWHIGH` latch (used by ECS display window extensions).
    #[must_use]
    pub const fn diwhigh(&self) -> u16 {
        self.diwhigh
    }

    /// Store ECS `DIWHIGH` for later extended DIW timing/composition work.
    pub fn write_diwhigh(&mut self, val: u16) {
        self.diwhigh = val;
    }
}

impl Default for AgnusEcs {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for AgnusEcs {
    type Target = InnerAgnusOcs;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for AgnusEcs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<AgnusEcs> for InnerAgnusOcs {
    fn from(agnus: AgnusEcs) -> Self {
        agnus.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::AgnusEcs;

    #[test]
    fn wrapper_uses_ocs_baseline_state_for_now() {
        let mut agnus = AgnusEcs::new();
        assert_eq!(agnus.vpos, 0);
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.dmacon, 0);

        agnus.tick_cck();
        assert_eq!(agnus.vpos, 0);
        assert_eq!(agnus.hpos, 1);
    }

    #[test]
    fn ecs_register_latches_are_independent_of_ocs_core_state() {
        let mut agnus = AgnusEcs::new();
        assert_eq!(agnus.beamcon0(), 0);
        assert_eq!(agnus.diwhigh(), 0);

        agnus.write_beamcon0(0x0020);
        agnus.write_diwhigh(0xA5A5);

        assert_eq!(agnus.beamcon0(), 0x0020);
        assert_eq!(agnus.diwhigh(), 0xA5A5);
        assert_eq!(agnus.diwstrt, 0);
        assert_eq!(agnus.diwstop, 0);
    }
}
