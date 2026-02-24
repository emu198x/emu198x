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
}

impl AgnusEcs {
    /// Create a new ECS Agnus wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerAgnusOcs::new(),
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
}
