//! Thin Commodore Super Denise (ECS) wrapper crate.
//!
//! This crate starts the ECS video path by wrapping `commodore-denise-ocs`.
//! ECS-specific Denise behavior (e.g. ECS display-mode extensions) can be
//! layered in here while preserving the current OCS rendering baseline.

use std::ops::{Deref, DerefMut};

pub use commodore_denise_ocs::DeniseOcs as InnerDeniseOcs;
pub use commodore_denise_ocs::{FB_HEIGHT, FB_WIDTH};

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
    use super::{DeniseEcs, FB_HEIGHT, FB_WIDTH};

    #[test]
    fn wrapper_uses_ocs_framebuffer_and_palette_baseline_for_now() {
        let mut denise = DeniseEcs::new();
        assert_eq!(denise.framebuffer.len(), (FB_WIDTH * FB_HEIGHT) as usize);
        assert_eq!(denise.palette[0], 0);

        denise.set_palette(0, 0x0FFF);
        assert_eq!(denise.palette[0], 0x0FFF);
    }
}
