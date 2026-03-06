//! Thin Commodore Super Denise (ECS) wrapper crate.
//!
//! This crate starts the ECS video path by wrapping `commodore-denise-ocs`.
//! ECS-specific Denise behavior (e.g. ECS display-mode extensions) can be
//! layered in here while preserving the current OCS rendering baseline.

use std::ops::{Deref, DerefMut};

pub use commodore_denise_ocs::DeniseOcs as InnerDeniseOcs;

const BPLCON3_KILLEHB: u16 = 0x0200;
const BPLCON3_ENBPLCN3: u16 = 0x0001;

/// Thin ECS wrapper that currently reuses the OCS Denise implementation.
pub struct DeniseEcs {
    inner: InnerDeniseOcs,
    /// ECS/ECS+ bitplane control extension register.
    ///
    /// This stays at the ECS layer because OCS Denise cannot observe it, while
    /// AGA Lisa builds on the same register state for palette banking and
    /// LOCT handling.
    pub bplcon3: u16,
}

impl DeniseEcs {
    /// Create a new ECS Denise wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerDeniseOcs::new(),
            bplcon3: 0,
        }
    }

    /// Wrap an existing OCS Denise core for behavior-identical OCS/ECS
    /// constructor routing during the early ECS bring-up phase.
    #[must_use]
    pub fn from_ocs(inner: InnerDeniseOcs) -> Self {
        Self { inner, bplcon3: 0 }
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

    /// Whether the ECS enhanced BPLCON3 register is enabled.
    #[must_use]
    pub const fn bplcon3_extensions_enabled(&self) -> bool {
        (self.bplcon3 & BPLCON3_ENBPLCN3) != 0
    }

    /// Whether ECS requests that EHB decoding be suppressed.
    #[must_use]
    pub const fn killehb_enabled(&self) -> bool {
        self.bplcon3_extensions_enabled() && (self.bplcon3 & BPLCON3_KILLEHB) != 0
    }

    /// Resolve a playfield colour index to 12-bit RGB, applying ECS-only
    /// BPLCON3 extensions on top of the shared Denise colour pipeline.
    pub fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        let ham = (self.inner.bplcon0 & 0x0800) != 0;
        let dual_playfield = (self.inner.bplcon0 & 0x0400) != 0;
        let num_planes = self.inner.num_bitplanes();

        if self.killehb_enabled()
            && !ham
            && !dual_playfield
            && num_planes == 6
            && (color_idx & 0x20) != 0
        {
            self.inner.palette[(color_idx as usize) & 0x1F]
        } else {
            self.inner.resolve_color_rgb12(color_idx)
        }
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

    fn prime_lowres_baseline(denise: &mut InnerDeniseOcs) {
        denise.set_palette(0, 0x000);
        denise.set_palette(1, 0x00F);
        denise.bplcon0 = 0x1000; // 1 bitplane, lowres
        denise.begin_beam_line();
        denise.bpl_data[0] = 0xA000; // 1,0,1,0,...
        denise.trigger_shift_load();
    }

    fn encode_sprite_pos_ctl(x: u16, vstart: u16, vstop: u16) -> (u16, u16) {
        let pos = ((vstart & 0x00FF) << 8) | ((x >> 1) & 0x00FF);
        let ctl = ((vstop & 0x00FF) << 8)
            | (((vstart >> 8) & 1) << 2)
            | (((vstop >> 8) & 1) << 1)
            | (x & 1);
        (pos, ctl)
    }

    #[test]
    fn wrapper_uses_ocs_raster_framebuffer_and_palette_baseline() {
        let mut denise = DeniseEcs::new();
        assert_eq!(
            denise.framebuffer_raster.len(),
            (RASTER_FB_WIDTH * PAL_RASTER_FB_HEIGHT) as usize
        );
        assert_eq!(denise.palette[0], 0);
        assert_eq!(denise.bplcon3, 0);

        denise.set_palette(0, 0x0FFF);
        assert_eq!(denise.palette[0], 0x0FFF);
    }

    #[test]
    fn from_ocs_preserves_wrapped_core_state() {
        let mut inner = InnerDeniseOcs::new();
        inner.set_palette(1, 0x0123);

        let denise = DeniseEcs::from_ocs(inner);
        assert_eq!(denise.palette[1], 0x0123);
        assert_eq!(denise.bplcon3, 0);
    }

    #[test]
    fn as_inner_mut_routes_mutations_to_wrapped_core() {
        let mut denise = DeniseEcs::new();

        denise.as_inner_mut().set_palette(2, 0x0456);

        assert_eq!(denise.as_inner().palette[2], 0x0456);
    }

    #[test]
    fn into_inner_returns_mutated_wrapped_core() {
        let mut denise = DeniseEcs::default();
        denise.set_palette(3, 0x0789);
        denise.bplcon3 = 0xA200;

        let inner = denise.into_inner();

        assert_eq!(inner.palette[3], 0x0789);
        assert_eq!(
            inner.framebuffer_raster.len(),
            (RASTER_FB_WIDTH * PAL_RASTER_FB_HEIGHT) as usize
        );
    }

    #[test]
    fn ecs_bplcon3_state_does_not_perturb_current_ocs_rendering_baseline() {
        let mut ocs = InnerDeniseOcs::new();
        prime_lowres_baseline(&mut ocs);

        let mut ecs = DeniseEcs::new();
        prime_lowres_baseline(&mut ecs);
        ecs.bplcon3 = 0xFE00;

        let ocs_dbg = ocs.output_pixel_with_beam(u32::MAX, u32::MAX, 0, 0);
        let ecs_dbg = ecs.output_pixel_with_beam(u32::MAX, u32::MAX, 0, 0);

        assert_eq!(ecs_dbg, ocs_dbg);
        assert_eq!(ecs.shift_count, ocs.shift_count);
        assert_eq!(ecs.framebuffer_raster[0], ocs.framebuffer_raster[0]);
    }

    #[test]
    fn ecs_wrapper_matches_ocs_sprite_overlay_behavior() {
        let mut ocs = InnerDeniseOcs::new();
        let mut ecs = DeniseEcs::new();

        for denise in [&mut ocs, &mut *ecs] {
            denise.set_palette(0, 0x000);
            denise.set_palette(1, 0x00F);
            denise.set_palette(17, 0xF00);
            denise.bplcon2 = 0x0001; // PF1P=1 => sprite group 0 in front of PF1
            denise.bpl_shift[0] = 0x8000;
            denise.shift_count = 1;

            let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 11);
            denise.write_sprite_pos(0, pos);
            denise.write_sprite_ctl(0, ctl);
            denise.write_sprite_datb(0, 0x0000);
            denise.write_sprite_data(0, 0x8000);
        }

        assert_eq!(
            ecs.output_pixel_color(20, 10),
            ocs.output_pixel_color(20, 10)
        );
        assert_eq!(ecs.clxdat, ocs.clxdat);
    }

    #[test]
    fn killehb_requires_extension_enable_and_disables_halfbrite_when_active() {
        let mut denise = DeniseEcs::new();
        denise.set_palette(5, 0x0ACE);
        denise.bplcon0 = 0x6000; // 6 planes, EHB

        assert_eq!(denise.resolve_color_rgb12(0x25), 0x0567);

        denise.bplcon3 = 0x0200; // KILLEHB without ENBPLCN3
        assert_eq!(denise.resolve_color_rgb12(0x25), 0x0567);

        denise.bplcon3 = 0x0201; // KILLEHB + ENBPLCN3
        assert_eq!(denise.resolve_color_rgb12(0x25), 0x0ACE);
        assert_eq!(denise.resolve_color_rgb12(0x05), 0x0ACE);
    }
}
