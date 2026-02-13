//! Blitter stub.
//!
//! Phase 1: accepts register writes, always reports not-busy.

/// Blitter state (stub).
pub struct Blitter {
    pub bltcon0: u16,
    pub bltcon1: u16,
    pub bltafwm: u16,
    pub bltalwm: u16,
    pub bltcpt: u32,
    pub bltbpt: u32,
    pub bltapt: u32,
    pub bltdpt: u32,
    pub bltsize: u16,
    pub bltcmod: u16,
    pub bltbmod: u16,
    pub bltamod: u16,
    pub bltdmod: u16,
    pub bltcdat: u16,
    pub bltbdat: u16,
    pub bltadat: u16,
}

impl Blitter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bltcon0: 0,
            bltcon1: 0,
            bltafwm: 0,
            bltalwm: 0,
            bltcpt: 0,
            bltbpt: 0,
            bltapt: 0,
            bltdpt: 0,
            bltsize: 0,
            bltcmod: 0,
            bltbmod: 0,
            bltamod: 0,
            bltdmod: 0,
            bltcdat: 0,
            bltbdat: 0,
            bltadat: 0,
        }
    }

    /// Blitter is never busy in phase 1.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub const fn is_busy(&self) -> bool {
        false
    }
}

impl Default for Blitter {
    fn default() -> Self {
        Self::new()
    }
}
