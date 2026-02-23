//! Thin Motorola 68020 wrapper crate.
//!
//! This is a small composition layer over the shared `motorola-68000` core.
//! It pins the configured CPU model to `M68020` while reusing the same core
//! implementation until model-specific behavior is implemented.

use std::ops::{Deref, DerefMut};

pub use motorola_68000::{Cpu68000 as InnerCpu68000, CpuCapabilities, CpuModel};

/// Thin wrapper that constructs the shared 68k core as a 68020 model.
pub struct Cpu68020 {
    inner: InnerCpu68000,
}

impl Cpu68020 {
    /// Create a new 68020 CPU wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerCpu68000::new_with_model(CpuModel::M68020),
        }
    }

    /// Borrow the wrapped shared CPU core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerCpu68000 {
        &self.inner
    }

    /// Mutably borrow the wrapped shared CPU core.
    #[must_use]
    pub fn as_inner_mut(&mut self) -> &mut InnerCpu68000 {
        &mut self.inner
    }

    /// Consume the wrapper and return the shared CPU core.
    #[must_use]
    pub fn into_inner(self) -> InnerCpu68000 {
        self.inner
    }
}

impl Default for Cpu68020 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Cpu68020 {
    type Target = InnerCpu68000;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Cpu68020 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Cpu68020> for InnerCpu68000 {
    fn from(cpu: Cpu68020) -> Self {
        cpu.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::{Cpu68020, CpuModel};

    #[test]
    fn wrapper_sets_68020_model() {
        let cpu = Cpu68020::new();
        assert_eq!(cpu.model(), CpuModel::M68020);
        assert!(cpu.capabilities().movec);
        assert!(cpu.capabilities().vbr);
        assert!(cpu.capabilities().cacr);
    }
}
