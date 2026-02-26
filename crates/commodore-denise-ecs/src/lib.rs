//! Thin Commodore Super Denise (ECS) wrapper crate.
//!
//! This crate starts the ECS video path by wrapping `commodore-denise-ocs`.
//! ECS-specific Denise behavior (e.g. ECS display-mode extensions) can be
//! layered in here while preserving the current OCS rendering baseline.

use std::ops::{Deref, DerefMut};

pub use commodore_denise_ocs::DeniseOcs as InnerDeniseOcs;

/// Thin ECS wrapper that currently reuses the OCS Denise implementation.
pub struct DeniseEcs {
    inner: InnerDeniseOcs,
}

impl DeniseEcs {
    /// Create a new ECS Denise wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerDeniseOcs::new(),
        }
    }

    /// Wrap an existing OCS Denise core for behavior-identical OCS/ECS
    /// constructor routing during the early ECS bring-up phase.
    #[must_use]
    pub fn from_ocs(inner: InnerDeniseOcs) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped OCS Denise core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerDeniseOcs {
        &self.inner
    }

    /// Mutably borrow the wrapped OCS Denise core.
    #[must_use]
    pub fn as_inner_mut(&mut self) -> &mut InnerDeniseOcs {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped OCS Denise core.
    #[must_use]
    pub fn into_inner(self) -> InnerDeniseOcs {
        self.inner
    }
}

impl Default for DeniseEcs {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for DeniseEcs {
    type Target = InnerDeniseOcs;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for DeniseEcs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<DeniseEcs> for InnerDeniseOcs {
    fn from(denise: DeniseEcs) -> Self {
        denise.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::{DeniseEcs, InnerDeniseOcs};
    use commodore_denise_ocs::{PAL_RASTER_FB_HEIGHT, RASTER_FB_WIDTH};

    #[test]
    fn wrapper_uses_ocs_raster_framebuffer_and_palette_baseline() {
        let mut denise = DeniseEcs::new();
        assert_eq!(
            denise.framebuffer_raster.len(),
            (RASTER_FB_WIDTH * PAL_RASTER_FB_HEIGHT) as usize
        );
        assert_eq!(denise.palette[0], 0);

        denise.set_palette(0, 0x0FFF);
        assert_eq!(denise.palette[0], 0x0FFF);
    }

    #[test]
    fn from_ocs_preserves_wrapped_core_state() {
        let mut inner = InnerDeniseOcs::new();
        inner.set_palette(1, 0x0123);

        let denise = DeniseEcs::from_ocs(inner);
        assert_eq!(denise.palette[1], 0x0123);
    }
}
