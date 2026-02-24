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

/// `BEAMCON0` bit enabling programmable beam counter comparator limits.
///
/// This mask is derived from the HRM `BEAMCON0` bit ordering (HARDDIS..HSYTRUE)
/// where `VARBEAMEN` appears after `VARHSYEN`.
pub const BEAMCON0_VARBEAMEN: u16 = 0x0100;
/// `BEAMCON0` bit disabling hardwired horizontal/vertical blanking.
pub const BEAMCON0_HARDDIS: u16 = 0x8000;
/// `BEAMCON0` bit enabling programmable vertical blanking window (`VBSTRT/VBSTOP`).
pub const BEAMCON0_VARVBEN: u16 = 0x2000;
/// `BEAMCON0` bit enabling programmable vertical sync (`VSSTRT/VSSTOP`).
pub const BEAMCON0_VARVSYEN: u16 = 0x0400;
/// `BEAMCON0` bit enabling programmable horizontal sync (`HSSTRT/HSSTOP`).
pub const BEAMCON0_VARHSYEN: u16 = 0x0200;
/// `BEAMCON0` bit redirecting composite blank to the external blank output.
pub const BEAMCON0_BLANKEN: u16 = 0x0010;
/// `BEAMCON0` bit selecting "true" polarity for composite sync output.
pub const BEAMCON0_CSYTRUE: u16 = 0x0008;
/// `BEAMCON0` bit selecting "true" polarity for vertical sync output.
pub const BEAMCON0_VSYTRUE: u16 = 0x0004;
/// `BEAMCON0` bit selecting "true" polarity for horizontal sync output.
pub const BEAMCON0_HSYTRUE: u16 = 0x0002;

/// Thin ECS wrapper that currently reuses the OCS Agnus implementation.
pub struct AgnusEcs {
    inner: InnerAgnusOcs,
    beamcon0: u16,
    htotal: u16,
    hsstop: u16,
    vtotal: u16,
    vsstop: u16,
    hbstrt: u16,
    hbstop: u16,
    vbstrt: u16,
    vbstop: u16,
    hsstrt: u16,
    vsstrt: u16,
    diwhigh: u16,
}

impl AgnusEcs {
    /// Create a new ECS Agnus wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerAgnusOcs::new(),
            beamcon0: 0,
            htotal: 0,
            hsstop: 0,
            vtotal: 0,
            vsstop: 0,
            hbstrt: 0,
            hbstop: 0,
            vbstrt: 0,
            vbstop: 0,
            hsstrt: 0,
            vsstrt: 0,
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
            htotal: 0,
            hsstop: 0,
            vtotal: 0,
            vsstop: 0,
            hbstrt: 0,
            hbstop: 0,
            vbstrt: 0,
            vbstop: 0,
            hsstrt: 0,
            vsstrt: 0,
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

    /// Tick one CCK, applying ECS programmable beam wrap limits when
    /// `BEAMCON0.VARBEAMEN` is enabled.
    ///
    /// This is currently a coarse compatibility model in the emulator's
    /// existing beam units (CCKs and raster lines), not a full ECS sync/blank
    /// generator implementation.
    pub fn tick_cck(&mut self) {
        if !self.varbeamen_enabled() {
            self.inner.tick_cck();
            return;
        }

        self.inner.hpos = self.inner.hpos.wrapping_add(1);
        if self.inner.hpos > self.htotal_highest_count() {
            self.inner.hpos = 0;
            self.inner.vpos = self.inner.vpos.wrapping_add(1);
            if self.inner.vpos > self.vtotal_highest_line() {
                self.inner.vpos = 0;
            }
        }
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

    #[must_use]
    pub const fn htotal(&self) -> u16 {
        self.htotal
    }

    pub fn write_htotal(&mut self, val: u16) {
        self.htotal = val;
    }

    #[must_use]
    pub const fn hsstop(&self) -> u16 {
        self.hsstop
    }

    pub fn write_hsstop(&mut self, val: u16) {
        self.hsstop = val;
    }

    #[must_use]
    pub const fn vtotal(&self) -> u16 {
        self.vtotal
    }

    pub fn write_vtotal(&mut self, val: u16) {
        self.vtotal = val;
    }

    #[must_use]
    pub const fn vsstop(&self) -> u16 {
        self.vsstop
    }

    pub fn write_vsstop(&mut self, val: u16) {
        self.vsstop = val;
    }

    #[must_use]
    pub const fn hbstrt(&self) -> u16 {
        self.hbstrt
    }

    pub fn write_hbstrt(&mut self, val: u16) {
        self.hbstrt = val;
    }

    #[must_use]
    pub const fn hbstop(&self) -> u16 {
        self.hbstop
    }

    pub fn write_hbstop(&mut self, val: u16) {
        self.hbstop = val;
    }

    #[must_use]
    pub const fn vbstrt(&self) -> u16 {
        self.vbstrt
    }

    pub fn write_vbstrt(&mut self, val: u16) {
        self.vbstrt = val;
    }

    #[must_use]
    pub const fn vbstop(&self) -> u16 {
        self.vbstop
    }

    pub fn write_vbstop(&mut self, val: u16) {
        self.vbstop = val;
    }

    #[must_use]
    pub const fn hsstrt(&self) -> u16 {
        self.hsstrt
    }

    pub fn write_hsstrt(&mut self, val: u16) {
        self.hsstrt = val;
    }

    #[must_use]
    pub const fn vsstrt(&self) -> u16 {
        self.vsstrt
    }

    pub fn write_vsstrt(&mut self, val: u16) {
        self.vsstrt = val;
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

    #[must_use]
    pub const fn varbeamen_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_VARBEAMEN) != 0
    }

    #[must_use]
    pub const fn varvben_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_VARVBEN) != 0
    }

    #[must_use]
    pub const fn varvsyen_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_VARVSYEN) != 0
    }

    #[must_use]
    pub const fn varhsyen_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_VARHSYEN) != 0
    }

    #[must_use]
    pub const fn harddis_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_HARDDIS) != 0
    }

    #[must_use]
    pub const fn blanken_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_BLANKEN) != 0
    }

    #[must_use]
    pub const fn csytrue_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_CSYTRUE) != 0
    }

    #[must_use]
    pub const fn vsytrue_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_VSYTRUE) != 0
    }

    #[must_use]
    pub const fn hsytrue_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_HSYTRUE) != 0
    }

    /// Coarse ECS vertical blanking window check used by `machine-amiga` display
    /// output gating while fuller sync/blank generator behavior is pending.
    #[must_use]
    pub fn vblank_window_active(&self, vpos: u16) -> bool {
        if !self.varvben_enabled() {
            return false;
        }

        let start = self.vbstrt & 0x07FF;
        let stop = self.vbstop & 0x07FF;
        if start == stop {
            return false;
        }
        if start < stop {
            vpos >= start && vpos < stop
        } else {
            vpos >= start || vpos < stop
        }
    }

    /// Coarse ECS horizontal blanking window check used by `machine-amiga`
    /// display output gating while fuller sync/blank generator behavior is
    /// pending.
    ///
    /// HRM exposes `HBSTRT/HBSTOP` without a dedicated "VARHBEN" bit; this
    /// helper uses `BEAMCON0.HARDDIS` as the coarse gate for programmable
    /// blank-window behavior in the current emulator beam model.
    #[must_use]
    pub fn hblank_window_active(&self, hpos: u16) -> bool {
        if !self.harddis_enabled() {
            return false;
        }

        let start = self.hbstrt & 0x01FF;
        let stop = self.hbstop & 0x01FF;
        if start == stop {
            return false;
        }
        if start < stop {
            hpos >= start && hpos < stop
        } else {
            hpos >= start || hpos < stop
        }
    }

    /// Coarse ECS horizontal sync window check used by `machine-amiga`
    /// debug/test-visible sync-state reporting while fuller sync generation is
    /// pending.
    #[must_use]
    pub fn hsync_window_active(&self, hpos: u16) -> bool {
        if !self.varhsyen_enabled() {
            return false;
        }

        let start = self.hsstrt & 0x01FF;
        let stop = self.hsstop & 0x01FF;
        if start == stop {
            return false;
        }
        if start < stop {
            hpos >= start && hpos < stop
        } else {
            hpos >= start || hpos < stop
        }
    }

    /// Coarse ECS vertical sync window check used by `machine-amiga`
    /// debug/test-visible sync-state reporting while fuller sync generation is
    /// pending.
    #[must_use]
    pub fn vsync_window_active(&self, vpos: u16) -> bool {
        if !self.varvsyen_enabled() {
            return false;
        }

        let start = self.vsstrt & 0x07FF;
        let stop = self.vsstop & 0x07FF;
        if start == stop {
            return false;
        }
        if start < stop {
            vpos >= start && vpos < stop
        } else {
            vpos >= start || vpos < stop
        }
    }

    fn htotal_highest_count(&self) -> u16 {
        if self.htotal == 0 {
            PAL_CCKS_PER_LINE - 1
        } else {
            // Coarse ECS model: treat the low 9 bits as the highest hpos count
            // in the emulator's current CCK-based beam units.
            self.htotal & 0x01FF
        }
    }

    fn vtotal_highest_line(&self) -> u16 {
        if self.vtotal == 0 {
            PAL_LINES_PER_FRAME - 1
        } else {
            self.vtotal & 0x07FF
        }
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
    use super::{
        AgnusEcs, BEAMCON0_BLANKEN, BEAMCON0_CSYTRUE, BEAMCON0_HARDDIS, BEAMCON0_HSYTRUE,
        BEAMCON0_VARBEAMEN, BEAMCON0_VARHSYEN, BEAMCON0_VARVBEN, BEAMCON0_VARVSYEN,
        BEAMCON0_VSYTRUE,
    };

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
        assert_eq!(agnus.htotal(), 0);
        assert_eq!(agnus.hsstop(), 0);
        assert_eq!(agnus.vtotal(), 0);
        assert_eq!(agnus.vsstop(), 0);
        assert_eq!(agnus.hbstrt(), 0);
        assert_eq!(agnus.hbstop(), 0);
        assert_eq!(agnus.vbstrt(), 0);
        assert_eq!(agnus.vbstop(), 0);
        assert_eq!(agnus.hsstrt(), 0);
        assert_eq!(agnus.vsstrt(), 0);
        assert_eq!(agnus.diwhigh(), 0);

        agnus.write_beamcon0(0x0020);
        agnus.write_htotal(0x0033);
        agnus.write_hsstop(0x0044);
        agnus.write_vtotal(0x0123);
        agnus.write_vsstop(0x0234);
        agnus.write_hbstrt(0x0010);
        agnus.write_hbstop(0x0020);
        agnus.write_vbstrt(0x0040);
        agnus.write_vbstop(0x0060);
        agnus.write_hsstrt(0x0070);
        agnus.write_vsstrt(0x0178);
        agnus.write_diwhigh(0xA5A5);

        assert_eq!(agnus.beamcon0(), 0x0020);
        assert_eq!(agnus.htotal(), 0x0033);
        assert_eq!(agnus.hsstop(), 0x0044);
        assert_eq!(agnus.vtotal(), 0x0123);
        assert_eq!(agnus.vsstop(), 0x0234);
        assert_eq!(agnus.hbstrt(), 0x0010);
        assert_eq!(agnus.hbstop(), 0x0020);
        assert_eq!(agnus.vbstrt(), 0x0040);
        assert_eq!(agnus.vbstop(), 0x0060);
        assert_eq!(agnus.hsstrt(), 0x0070);
        assert_eq!(agnus.vsstrt(), 0x0178);
        assert_eq!(agnus.diwhigh(), 0xA5A5);
        assert_eq!(agnus.diwstrt, 0);
        assert_eq!(agnus.diwstop, 0);
    }

    #[test]
    fn beamcon0_blanken_and_polarity_helpers_reflect_latched_bits() {
        let mut agnus = AgnusEcs::new();
        assert!(!agnus.blanken_enabled());
        assert!(!agnus.csytrue_enabled());
        assert!(!agnus.vsytrue_enabled());
        assert!(!agnus.hsytrue_enabled());

        agnus.write_beamcon0(
            BEAMCON0_BLANKEN | BEAMCON0_CSYTRUE | BEAMCON0_VSYTRUE | BEAMCON0_HSYTRUE,
        );

        assert!(agnus.blanken_enabled());
        assert!(agnus.csytrue_enabled());
        assert!(agnus.vsytrue_enabled());
        assert!(agnus.hsytrue_enabled());
    }

    #[test]
    fn varhsyen_and_varvsyen_bits_are_reported_from_beamcon0() {
        let mut agnus = AgnusEcs::new();
        assert!(!agnus.varhsyen_enabled());
        assert!(!agnus.varvsyen_enabled());

        agnus.write_beamcon0(BEAMCON0_VARHSYEN | BEAMCON0_VARVSYEN);

        assert!(agnus.varhsyen_enabled());
        assert!(agnus.varvsyen_enabled());
    }

    #[test]
    fn varbeamen_uses_programmed_htotal_and_vtotal_for_wrap() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(3);
        agnus.write_vtotal(1);
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN);

        // hpos counts 0..3 then wraps and advances vpos.
        for expected_h in [1u16, 2, 3] {
            agnus.tick_cck();
            assert_eq!(agnus.hpos, expected_h);
            assert_eq!(agnus.vpos, 0);
        }
        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 1);

        // One more 4-CCK line wraps vpos back to 0 because VTOTAL=1.
        for _ in 0..4 {
            agnus.tick_cck();
        }
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 0);
    }

    #[test]
    fn varvben_uses_programmed_vertical_blank_window() {
        let mut agnus = AgnusEcs::new();
        agnus.write_vbstrt(10);
        agnus.write_vbstop(20);
        agnus.write_beamcon0(BEAMCON0_VARVBEN);
        assert!(!agnus.vblank_window_active(9));
        assert!(agnus.vblank_window_active(10));
        assert!(agnus.vblank_window_active(19));
        assert!(!agnus.vblank_window_active(20));
    }

    #[test]
    fn varvben_blank_window_wraps_across_frame_zero() {
        let mut agnus = AgnusEcs::new();
        agnus.write_vbstrt(300);
        agnus.write_vbstop(20);
        agnus.write_beamcon0(BEAMCON0_VARVBEN);
        assert!(agnus.vblank_window_active(301));
        assert!(agnus.vblank_window_active(10));
        assert!(!agnus.vblank_window_active(200));
    }

    #[test]
    fn harddis_uses_programmed_horizontal_blank_window() {
        let mut agnus = AgnusEcs::new();
        agnus.write_hbstrt(10);
        agnus.write_hbstop(20);
        agnus.write_beamcon0(BEAMCON0_HARDDIS);
        assert!(!agnus.hblank_window_active(9));
        assert!(agnus.hblank_window_active(10));
        assert!(agnus.hblank_window_active(19));
        assert!(!agnus.hblank_window_active(20));
    }

    #[test]
    fn harddis_hblank_window_wraps_across_line_zero() {
        let mut agnus = AgnusEcs::new();
        agnus.write_hbstrt(220);
        agnus.write_hbstop(10);
        agnus.write_beamcon0(BEAMCON0_HARDDIS);
        assert!(agnus.hblank_window_active(221));
        assert!(agnus.hblank_window_active(5));
        assert!(!agnus.hblank_window_active(100));
    }

    #[test]
    fn varhsyen_uses_programmed_horizontal_sync_window() {
        let mut agnus = AgnusEcs::new();
        agnus.write_hsstrt(30);
        agnus.write_hsstop(40);
        agnus.write_beamcon0(BEAMCON0_VARHSYEN);
        assert!(!agnus.hsync_window_active(29));
        assert!(agnus.hsync_window_active(30));
        assert!(agnus.hsync_window_active(39));
        assert!(!agnus.hsync_window_active(40));
    }

    #[test]
    fn varhsyen_sync_window_wraps_across_line_zero() {
        let mut agnus = AgnusEcs::new();
        agnus.write_hsstrt(220);
        agnus.write_hsstop(12);
        agnus.write_beamcon0(BEAMCON0_VARHSYEN);
        assert!(agnus.hsync_window_active(223));
        assert!(agnus.hsync_window_active(5));
        assert!(!agnus.hsync_window_active(100));
    }

    #[test]
    fn varvsyen_uses_programmed_vertical_sync_window() {
        let mut agnus = AgnusEcs::new();
        agnus.write_vsstrt(100);
        agnus.write_vsstop(110);
        agnus.write_beamcon0(BEAMCON0_VARVSYEN);
        assert!(!agnus.vsync_window_active(99));
        assert!(agnus.vsync_window_active(100));
        assert!(agnus.vsync_window_active(109));
        assert!(!agnus.vsync_window_active(110));
    }

    #[test]
    fn varvsyen_sync_window_wraps_across_frame_zero() {
        let mut agnus = AgnusEcs::new();
        agnus.write_vsstrt(300);
        agnus.write_vsstop(12);
        agnus.write_beamcon0(BEAMCON0_VARVSYEN);
        assert!(agnus.vsync_window_active(301));
        assert!(agnus.vsync_window_active(5));
        assert!(!agnus.vsync_window_active(200));
    }
}
