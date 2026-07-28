//! Thin Commodore Super Agnus (ECS) wrapper crate.
//!
//! This crate starts the ECS path as a composition layer over the existing OCS
//! Agnus implementation. It preserves current behavior while giving us a place
//! to add ECS-specific DMA/register/timing deltas incrementally.

use std::ops::{Deref, DerefMut};

use commodore_agnus_ocs::{
    NTSC_VBL_END_LINE, PAL_VBL_END_LINE, SpriteDmaVerticalTiming,
    bits::{BPLCON0_LACE, BPLCON0_SHRES, BPLCON0_UHRES},
};

pub use commodore_agnus_ocs::Agnus as InnerAgnusOcs;
pub use commodore_agnus_ocs::{
    BlitterCckOutcome, BlitterDmaOp, CckBusPlan, Copper, CopperState, HIRES_DDF_TO_PLANE,
    LOWRES_DDF_TO_PLANE, LOWRES_DDF_TO_PLANE_AGA, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME,
    PaulaReturnProgressPolicy, SlotOwner,
};

// `BEAMCON0` bit layout (bits 14..0), matching the HRM and WinUAE definitions.
//
// Bit 5 (`PAL`) is a read/write flag that indicates PAL vs NTSC mode. On real
// hardware this defaults to set for PAL systems and clear for NTSC. Software
// (including `graphics.library`) reads this bit to detect the video standard.

/// Bit 0: select "true" (active-high) polarity for horizontal sync output.
pub const BEAMCON0_HSYTRUE: u16 = 0x0001;
/// Bit 1: select "true" (active-high) polarity for vertical sync output.
pub const BEAMCON0_VSYTRUE: u16 = 0x0002;
/// Bit 2: select "true" (active-high) polarity for composite sync output.
pub const BEAMCON0_CSYTRUE: u16 = 0x0004;
/// Bit 3: redirect composite blank to the external blank output.
pub const BEAMCON0_BLANKEN: u16 = 0x0008;
/// Bit 4: enable programmable composite sync (coarse modeled).
pub const BEAMCON0_VARCSYEN: u16 = 0x0010;
/// Bit 5: PAL mode flag. Set on PAL systems, clear on NTSC.
pub const BEAMCON0_PAL: u16 = 0x0020;
/// Bit 6: dual-playfield genlock mode (not emulated).
pub const BEAMCON0_DUAL: u16 = 0x0040;
/// Bit 7: enable programmable beam counter comparator limits.
pub const BEAMCON0_VARBEAMEN: u16 = 0x0080;
/// Bit 8: enable programmable horizontal sync (`HSSTRT/HSSTOP`).
pub const BEAMCON0_VARHSYEN: u16 = 0x0100;
/// Bit 9: enable programmable vertical sync (`VSSTRT/VSSTOP`).
pub const BEAMCON0_VARVSYEN: u16 = 0x0200;
/// Bit 10: redirect composite sync output path.
pub const BEAMCON0_CSCBEN: u16 = 0x0400;
/// Bit 11: disable long-line / short-line toggle (not emulated).
pub const BEAMCON0_LOLDIS: u16 = 0x0800;
/// Bit 12: enable programmable vertical blanking window (`VBSTRT/VBSTOP`).
pub const BEAMCON0_VARVBEN: u16 = 0x1000;
/// Bit 13: disable light-pen input latch (not emulated).
pub const BEAMCON0_LPENDIS: u16 = 0x2000;
/// Bit 14: disable hardwired horizontal/vertical blanking.
pub const BEAMCON0_HARDDIS: u16 = 0x4000;

// WinUAE seeds the write-only programmable blank comparators to $FFFF on a
// hard reset. Keep that value out of the 11-bit comparison domain so writing
// another vertical-timing register cannot accidentally arm a blank edge.
const UNWRITTEN_VERTICAL_BLANK_EDGE: u16 = u16::MAX;

/// Reported sync and blank output pin levels from the ECS sync generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncPinLevels {
    /// Horizontal sync output (polarity applied via HSYTRUE).
    pub hsync: bool,
    /// Vertical sync output (polarity applied via VSYTRUE).
    pub vsync: bool,
    /// Composite sync output (gated by CSCBEN, polarity via CSYTRUE).
    pub csync: bool,
    /// Composite blank output (gated by BLANKEN).
    pub blank: bool,
}

/// Thin ECS wrapper that currently reuses the OCS Agnus implementation.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AgnusEcs {
    inner: InnerAgnusOcs,
    beamcon0: u16,
    /// Effective reset default: standard-region highest horizontal count.
    htotal: u16,
    hsstop: u16,
    /// Effective reset default: standard-region short-field reset line.
    vtotal: u16,
    vsstop: u16,
    hbstrt: u16,
    hbstop: u16,
    vbstrt: u16,
    vbstop: u16,
    /// Whether any programmable vertical timing register has been written.
    /// Until then the emulator does not claim a physical reset value for the
    /// write-only comparator latches.
    programmed_vertical_accessed: bool,
    /// Hidden programmed vertical-blank latch. It follows VBSTRT/VBSTOP
    /// line-entry events independently of `BEAMCON0.VARVBEN`.
    programmed_vblank_active: bool,
    /// One-line programmed vertical-blank edge pulses.
    programmed_vblank_start_event: bool,
    programmed_vblank_stop_event: bool,
    hsstrt: u16,
    vsstrt: u16,
    diwhigh: u16,
    diwhigh_written: bool,
    /// Hidden vertical display-window latch. VSTART/VSTOP and the hard
    /// vertical-blank start event change this state on comparator events;
    /// register writes do not reconstruct it from the current beam position.
    vertical_diw_active: bool,
    /// ECS BLTSIZV ($05C) shadow — extended vertical blit size in
    /// lines (15 bits). Latched on write; consumed by the next
    /// BLTSIZH-triggered blit. KS 2.x / 3.x uses this for any blit
    /// that wouldn't fit the legacy BLTSIZE encoding (V > 1023 lines
    /// or H > 63 words), and the routine path for many text / icon
    /// blits even when small.
    pub bltsizv: u16,
    /// ECS BLTSIZH ($05E) shadow — extended horizontal blit size in
    /// words (11 bits). Writing this triggers the blit using V from
    /// the most recent BLTSIZV write.
    pub bltsizh: u16,
}

impl AgnusEcs {
    /// Create a new ECS Agnus wrapper. Defaults to PAL, the most
    /// common ECS chipset region. `agnus_id` is set to the ECS 8375
    /// PAL value (`$2000`) so VPOSR reads identify the chip as ECS,
    /// not the inner OCS core's default. Use [`Self::from_ocs`] to
    /// preserve a region-configured OCS core.
    #[must_use]
    pub fn new() -> Self {
        let mut inner = InnerAgnusOcs::new();
        // Override the OCS-inherited agnus_id with the ECS 8375 PAL
        // identifier. Stored pre-shifted into VPOSR bits 14-8.
        inner.agnus_id = 0x2000;
        // Seed the programmable-total shadows with the effective standard
        // timing. Commodore documents the counter limits but not the
        // write-only latches' silicon reset contents; this preserves normal
        // timing if VARBEAMEN is enabled before either total is written,
        // while leaving an explicit write of zero meaningful.
        let default_htotal = inner.current_line_ccks() - 1;
        let default_vtotal = inner.lines_per_frame - 1;
        Self {
            inner,
            beamcon0: BEAMCON0_PAL,
            htotal: default_htotal,
            hsstop: 0,
            vtotal: default_vtotal,
            vsstop: 0,
            hbstrt: 0,
            hbstop: 0,
            vbstrt: UNWRITTEN_VERTICAL_BLANK_EDGE,
            vbstop: UNWRITTEN_VERTICAL_BLANK_EDGE,
            programmed_vertical_accessed: false,
            programmed_vblank_active: false,
            programmed_vblank_start_event: false,
            programmed_vblank_stop_event: false,
            hsstrt: 0,
            vsstrt: 0,
            diwhigh: 0,
            diwhigh_written: false,
            vertical_diw_active: false,
            bltsizv: 0,
            bltsizh: 0,
        }
    }

    /// Wrap an existing OCS Agnus core while starting ECS extension registers
    /// from reset state. Overrides the inner core's `agnus_id` so the
    /// chip identifies as ECS 8375 (`$2000` PAL / `$3000` NTSC) rather
    /// than inheriting the inner OCS Agnus's identifier — KS 2.x / 3.x
    /// gate on this value when picking chipset-feature code paths.
    #[must_use]
    pub fn from_ocs(mut inner: InnerAgnusOcs) -> Self {
        inner.agnus_id = match inner.region {
            commodore_agnus_ocs::AgnusRegion::Pal => 0x2000,
            commodore_agnus_ocs::AgnusRegion::Ntsc => 0x3000,
        };
        // See `new`: these are effective emulator defaults rather than
        // claimed physical reset values for the write-only latches.
        let default_htotal = inner.current_line_ccks() - 1;
        let default_vtotal = inner.lines_per_frame - 1;
        let beamcon0 = match inner.region {
            commodore_agnus_ocs::AgnusRegion::Pal => BEAMCON0_PAL,
            commodore_agnus_ocs::AgnusRegion::Ntsc => 0,
        };
        Self {
            inner,
            beamcon0,
            htotal: default_htotal,
            hsstop: 0,
            vtotal: default_vtotal,
            vsstop: 0,
            hbstrt: 0,
            hbstop: 0,
            vbstrt: UNWRITTEN_VERTICAL_BLANK_EDGE,
            vbstop: UNWRITTEN_VERTICAL_BLANK_EDGE,
            programmed_vertical_accessed: false,
            programmed_vblank_active: false,
            programmed_vblank_start_event: false,
            programmed_vblank_stop_event: false,
            hsstrt: 0,
            vsstrt: 0,
            diwhigh: 0,
            diwhigh_written: false,
            vertical_diw_active: false,
            bltsizv: 0,
            bltsizh: 0,
        }
    }

    /// BLTCON0L ($05A) — write the low byte of BLTCON0 (the LF
    /// logic-function bits + USEx channel-enable bits) without
    /// disturbing the high byte (shift amount, A/B/C/D enables).
    ///
    /// ECS only. The high byte of `val` is discarded — this is a
    /// byte-write port even though the bus delivers a word. Doesn't
    /// trigger a blit.
    pub fn write_bltcon0l(&mut self, val: u16) {
        let lo = val & 0x00FF;
        self.inner.bltcon0 = (self.inner.bltcon0 & 0xFF00) | lo;
    }

    /// BLTSIZV ($05C) — set the ECS extended vertical blit size
    /// (lines, 15 bits). Latched for the next BLTSIZH-triggered
    /// blit; sticky across multiple blits (per WinUAE) so a series
    /// of same-height blits can write BLTSIZH only.
    ///
    /// Doesn't trigger the blit.
    pub fn write_bltsizv(&mut self, val: u16) {
        self.bltsizv = val & 0x7FFF;
    }

    /// BLTSIZH ($05E) — set the ECS extended horizontal blit size
    /// (words, 11 bits) AND trigger the blit. The blit runs with V from
    /// the most recent BLTSIZV write.
    ///
    /// Drives the blitter from the FULL ECS size (15-bit height, 11-bit
    /// width) via `start_blit_with_size`, so blits wider than the legacy
    /// 10+6-bit BLTSIZE field no longer wrap (#36). A value of 0 in
    /// either field means the field maximum, matching WinUAE
    /// (BLTSIZV → 0x8000 lines, BLTSIZH → 0x800 words).
    pub fn write_bltsizh(&mut self, val: u16) {
        let h = val & 0x07FF;
        self.bltsizh = h;
        let height = if self.bltsizv == 0 {
            0x8000
        } else {
            u32::from(self.bltsizv)
        };
        let width_words = if h == 0 { 0x0800 } else { u32::from(h) };
        // Keep a clamped legacy-encoded BLTSIZE so query/debug views of
        // `bltsize` stay populated; the engine itself runs from the full
        // size above, not this packed (and for large blits lossy) value.
        self.inner.bltsize = ((self.bltsizv & 0x03FF) << 6) | (h & 0x003F);
        self.inner.start_blit_with_size(height, width_words);
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

    fn sprite_dma_vertical_timing(&self) -> SpriteDmaVerticalTiming {
        let timing = if self.varvben_enabled() {
            SpriteDmaVerticalTiming::programmed(
                self.programmed_vblank_active,
                self.programmed_vblank_stop_event,
            )
        } else {
            let blank_stop = if (self.beamcon0 & BEAMCON0_PAL) != 0 {
                PAL_VBL_END_LINE
            } else {
                NTSC_VBL_END_LINE
            };
            SpriteDmaVerticalTiming::fixed(blank_stop)
        };
        timing.with_ten_bit_sprite_comparators()
    }

    fn enter_programmed_vertical_line(&mut self, vpos: u16) {
        self.programmed_vblank_start_event = false;
        self.programmed_vblank_stop_event = false;
        if !self.programmed_vertical_accessed {
            return;
        }

        // Start precedes stop. Equal comparators therefore describe an
        // empty blank level while still producing both one-line events.
        if self.vbstrt != UNWRITTEN_VERTICAL_BLANK_EDGE && vpos == (self.vbstrt & 0x07FF) {
            self.programmed_vblank_active = true;
            self.programmed_vblank_start_event = true;
        }
        if self.vbstop != UNWRITTEN_VERTICAL_BLANK_EDGE && vpos == (self.vbstop & 0x07FF) {
            self.programmed_vblank_active = false;
            self.programmed_vblank_stop_event = true;
        }
    }

    fn bitplane_vertical_bounds(&self) -> (u16, u16) {
        if self.diwhigh_written {
            // An explicit DIWHIGH write selects direct high-bit decoding even
            // when the value is zero. Alice exposes V10..V8; ECS Agnus also
            // exposes the undocumented V11 modelled by WinUAE.
            let high_mask = if self.inner.max_bitplanes > 6 {
                0x0007
            } else {
                0x000F
            };
            let vstart = ((self.diwhigh & high_mask) << 8) | ((self.inner.diwstrt >> 8) & 0x00FF);
            let vstop =
                (((self.diwhigh >> 8) & high_mask) << 8) | ((self.inner.diwstop >> 8) & 0x00FF);
            (vstart, vstop)
        } else {
            // Legacy OCS-style implicit V8 behavior until DIWHIGH is written.
            let vstart = (self.inner.diwstrt >> 8) & 0x00FF; // V8 = 0
            let stop_low = (self.inner.diwstop >> 8) & 0x00FF;
            let stop_v8 = ((!((stop_low >> 7) & 0x1)) & 0x1) << 8; // V8 != V7
            let vstop = stop_v8 | stop_low;
            (vstart, vstop)
        }
    }

    fn hard_vertical_diw_boundary_disabled(&self) -> bool {
        self.varbeamen_enabled() || self.varvben_enabled() || self.harddis_enabled()
    }

    fn horizontal_hard_ddf_limit_disabled(&self) -> bool {
        self.harddis_enabled()
            || self.varbeamen_enabled()
            || (self.inner.bplcon0 & (BPLCON0_SHRES | BPLCON0_UHRES)) != 0
    }

    fn fixed_vertical_blank_start_event(&self, vpos: u16) -> bool {
        vpos == self.current_field_lines() - 1
    }

    fn evaluate_vertical_diw_comparators(&mut self, vpos: u16) {
        let force_off = !self.hard_vertical_diw_boundary_disabled()
            && self.fixed_vertical_blank_start_event(vpos);
        let (vstart, vstop) = self.bitplane_vertical_bounds();
        let start = vpos == vstart && !force_off;
        let stop = vpos == vstop || force_off;

        // Stop has precedence. Equal comparators therefore keep the window
        // closed, and a VSTART match on the hard vertical-blank start event
        // cannot open it.
        if start && !stop {
            self.vertical_diw_active = true;
        } else if stop {
            self.vertical_diw_active = false;
        }
    }

    /// Current hidden vertical display-window latch used by bitplane DMA.
    #[must_use]
    pub const fn vertical_diw_active(&self) -> bool {
        self.vertical_diw_active
    }

    /// Current hidden programmed vertical-blank latch.
    #[must_use]
    pub const fn programmed_vblank_active(&self) -> bool {
        self.programmed_vblank_active
    }

    /// Whether the current line carries the programmed `VBSTOP` event.
    #[must_use]
    pub const fn programmed_vblank_stop_event(&self) -> bool {
        self.programmed_vblank_stop_event
    }

    /// Tick one CCK, applying ECS programmable beam wrap limits when
    /// `BEAMCON0.VARBEAMEN` is enabled.
    ///
    /// This is currently a coarse compatibility model in the emulator's
    /// existing beam units (CCKs and raster lines), not a full ECS sync/blank
    /// generator implementation.
    pub fn tick_cck(&mut self) {
        let line_ccks = self.current_line_ccks();
        let short_field_lines = self.short_field_lines();
        if let Some(vpos) = self.inner.next_cck_line_entry(line_ccks, short_field_lines) {
            self.enter_programmed_vertical_line(vpos);
            self.evaluate_vertical_diw_comparators(vpos);
        }
        let sprite_timing = self.sprite_dma_vertical_timing();
        let fixed_ddf_right_stop_enabled = !self.horizontal_hard_ddf_limit_disabled();
        self.inner.tick_cck_with_variant_timing(
            line_ccks,
            short_field_lines,
            sprite_timing,
            fixed_ddf_right_stop_enabled,
        );
    }

    /// Length of the active scanline in CCKs.
    ///
    /// Fixed timing follows the wrapped OCS core, including NTSC long-line
    /// state. `BEAMCON0.VARBEAMEN` selects the programmed `HTOTAL` limit and
    /// retains the same long-line extension used by beam advancement.
    #[must_use]
    pub fn current_line_ccks(&self) -> u16 {
        if self.varbeamen_enabled() {
            self.htotal_highest_count() + 1 + u16::from(self.inner.lol)
        } else {
            self.inner.current_line_ccks()
        }
    }

    /// Horizontal position currently visible to the Copper `WAIT`/`SKIP`
    /// comparator.
    ///
    /// Fixed timing uses the inherited OCS counter projection. With
    /// `BEAMCON0.VARBEAMEN`, the programmed `HTOTAL` and current long-line
    /// state select the physical line length. The comparator's period is the
    /// largest even number of CCKs in that line.
    #[must_use]
    pub fn copper_comparator_hpos(&self) -> u16 {
        if !self.varbeamen_enabled() {
            return self.inner.copper_comparator_hpos();
        }

        let period = self.current_line_ccks() & !1;
        if period == 0 {
            0
        } else {
            (self.inner.hpos + 2) % period
        }
    }

    fn short_field_lines(&self) -> u16 {
        if self.varbeamen_enabled() {
            self.vtotal_highest_line() + 1
        } else {
            self.inner.lines_per_frame
        }
    }

    fn current_field_lines(&self) -> u16 {
        self.short_field_lines()
            + u16::from((self.inner.bplcon0 & BPLCON0_LACE) != 0 && self.inner.lof)
    }

    fn bitplane_dma_vertical_active(&self) -> bool {
        self.vertical_diw_active
    }

    /// ECS-aware bus plan that applies vertical bitplane DMA gating from the
    /// display window timing (DIWSTRT/DIWSTOP[/DIWHIGH]) before exposing the
    /// Agnus slot grant decisions to the machine.
    #[must_use]
    pub fn cck_bus_plan(&self) -> CckBusPlan {
        self.inner.cck_bus_plan_with_vertical_timing(
            self.bitplane_dma_vertical_active(),
            self.sprite_dma_vertical_timing(),
        )
    }

    /// ECS-aware owner for callers that need only the raw slot identity.
    #[must_use]
    pub fn current_slot(&self) -> SlotOwner {
        self.cck_bus_plan().slot_owner
    }

    /// Service one sprite-DMA cycle using the selected fixed or programmed
    /// vertical blank boundary.
    pub fn service_sprite_dma_cyc(
        &mut self,
        channel: usize,
        second_word: bool,
        width: u8,
        read: impl FnMut(u32) -> u16,
    ) -> Option<(bool, u64)> {
        let sprite_timing = self.sprite_dma_vertical_timing();
        self.inner.service_sprite_dma_cyc_with_vertical_timing(
            channel,
            second_word,
            width,
            sprite_timing,
            read,
        )
    }

    /// Apply a direct `SPRxPOS` write through the selected vertical timing.
    pub fn poke_sprite_pos(&mut self, channel: usize, val: u16) {
        let sprite_timing = self.sprite_dma_vertical_timing();
        self.inner
            .poke_sprite_pos_with_vertical_timing(channel, val, sprite_timing);
    }

    /// Apply a direct `SPRxCTL` write through the selected vertical timing.
    pub fn poke_sprite_ctl(&mut self, channel: usize, val: u16) {
        let sprite_timing = self.sprite_dma_vertical_timing();
        self.inner
            .poke_sprite_ctl_with_vertical_timing(channel, val, sprite_timing);
    }

    /// ECS `BEAMCON0` latch (register semantics are not fully modeled yet).
    #[must_use]
    pub const fn beamcon0(&self) -> u16 {
        self.beamcon0
    }

    /// Set the effective BEAMCON0 PAL mode while preserving all other
    /// control bits. Construction uses the wrapped chip's region; guest
    /// writes reach [`Self::write_beamcon0`] instead.
    pub fn set_pal_mode(&mut self, pal: bool) {
        if pal {
            self.beamcon0 |= BEAMCON0_PAL;
        } else {
            self.beamcon0 &= !BEAMCON0_PAL;
        }
        self.sync_lol_toggle();
        self.evaluate_vertical_diw_comparators(self.inner.vpos);
    }

    /// Store ECS `BEAMCON0` and apply its PAL/LOLDIS line-toggle controls.
    pub fn write_beamcon0(&mut self, val: u16) {
        self.beamcon0 = val;
        self.sync_lol_toggle();
        self.evaluate_vertical_diw_comparators(self.inner.vpos);
    }

    fn sync_lol_toggle(&mut self) {
        self.inner.lol_toggle = (self.beamcon0 & (BEAMCON0_PAL | BEAMCON0_LOLDIS)) == 0;
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
        self.programmed_vertical_accessed = true;
    }

    #[must_use]
    pub const fn vsstop(&self) -> u16 {
        self.vsstop
    }

    pub fn write_vsstop(&mut self, val: u16) {
        self.vsstop = val;
        self.programmed_vertical_accessed = true;
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
        self.programmed_vertical_accessed = true;
    }

    #[must_use]
    pub const fn vbstop(&self) -> u16 {
        self.vbstop
    }

    pub fn write_vbstop(&mut self, val: u16) {
        self.vbstop = val;
        self.programmed_vertical_accessed = true;
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
        self.programmed_vertical_accessed = true;
    }

    /// ECS `DIWHIGH` latch (used by ECS display window extensions).
    #[must_use]
    pub const fn diwhigh(&self) -> u16 {
        self.diwhigh
    }

    /// Whether vertical display-window decoding currently uses explicit
    /// `DIWHIGH` extension bits rather than the legacy implicit-high-bit rule.
    #[must_use]
    pub const fn diwhigh_written(&self) -> bool {
        self.diwhigh_written
    }

    /// Write base display-window start and return vertical decoding to the
    /// legacy implicit-high-bit rule until `DIWHIGH` is written again.
    pub fn write_diwstrt(&mut self, val: u16) {
        self.inner.write_diwstrt(val);
        self.diwhigh_written = false;
        self.evaluate_vertical_diw_comparators(self.inner.vpos);
    }

    /// Write base display-window stop and return vertical decoding to the
    /// legacy implicit-high-bit rule until `DIWHIGH` is written again.
    pub fn write_diwstop(&mut self, val: u16) {
        self.inner.write_diwstop(val);
        self.diwhigh_written = false;
        self.evaluate_vertical_diw_comparators(self.inner.vpos);
    }

    /// Store ECS `DIWHIGH` and evaluate only comparator events at the current
    /// line. The existing vertical-DIW latch is otherwise preserved.
    pub fn write_diwhigh(&mut self, val: u16) {
        self.diwhigh = val;
        self.diwhigh_written = true;
        self.evaluate_vertical_diw_comparators(self.inner.vpos);
    }

    /// Route one ECS programmable timing-register write.
    ///
    /// Returns `true` when `offset` names a register owned by this
    /// wrapper. Unsupported ECS/AGA extension registers remain
    /// unhandled so the machine can route them to another chip.
    pub fn write_timing_register(&mut self, offset: u16, val: u16) -> bool {
        match offset {
            0x1C0 => self.write_htotal(val),
            0x1C2 => self.write_hsstop(val),
            0x1C4 => self.write_hbstrt(val),
            0x1C6 => self.write_hbstop(val),
            0x1C8 => self.write_vtotal(val),
            0x1CA => self.write_vsstop(val),
            0x1CC => self.write_vbstrt(val),
            0x1CE => self.write_vbstop(val),
            0x1DC => self.write_beamcon0(val),
            0x1DE => self.write_hsstrt(val),
            0x1E0 => self.write_vsstrt(val),
            0x1E4 => self.write_diwhigh(val),
            _ => return false,
        }
        true
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
    pub const fn cscben_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_CSCBEN) != 0
    }

    #[must_use]
    pub const fn varcsyen_enabled(&self) -> bool {
        (self.beamcon0 & BEAMCON0_VARCSYEN) != 0
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

    /// Geometric ECS vertical-blank window check for an arbitrary line.
    ///
    /// This remains a coarse sync-pin helper. Live sprite DMA uses the
    /// edge-driven programmed latch because mid-field register changes and
    /// unreachable comparator lines cannot be reconstructed from a range.
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

    /// Reported sync and blank pin levels for the current beam position.
    ///
    /// Applies `BEAMCON0` polarity bits (HSYTRUE, VSYTRUE, CSYTRUE),
    /// BLANKEN blank routing, and CSCBEN composite sync routing.
    #[must_use]
    pub fn sync_pin_levels(&self, hpos: u16, vpos: u16) -> SyncPinLevels {
        // Raw window-active states (active = true).
        let hsync_raw = self.hsync_window_active(hpos);
        let vsync_raw = self.vsync_window_active(vpos);
        let hblank_raw = self.hblank_window_active(hpos);
        let vblank_raw = self.vblank_window_active(vpos);

        // Apply polarity: "TRUE" polarity means active-high output.
        // When the bit is clear, the output is inverted (active-low).
        let hsync = if self.hsytrue_enabled() {
            hsync_raw
        } else {
            !hsync_raw
        };
        let vsync = if self.vsytrue_enabled() {
            vsync_raw
        } else {
            !vsync_raw
        };

        // Composite sync: XOR of HSYNC and VSYNC raw states, then polarity.
        let csync_raw = hsync_raw ^ vsync_raw;
        let csync = if self.csytrue_enabled() {
            csync_raw
        } else {
            !csync_raw
        };

        // Composite blank: OR of HBLANK and VBLANK.
        let cblank_raw = hblank_raw || vblank_raw;

        // BLANKEN gates composite blank to the external blank output pin.
        let blank_out = self.blanken_enabled() && cblank_raw;

        // CSCBEN routes composite sync to the external composite sync pin.
        let csync_out = self.cscben_enabled() && csync;

        SyncPinLevels {
            hsync,
            vsync,
            csync: csync_out,
            blank: blank_out,
        }
    }

    fn htotal_highest_count(&self) -> u16 {
        // Coarse ECS model: treat the low 9 bits as the highest hpos count
        // in the emulator's current CCK-based beam units.
        self.htotal & 0x01FF
    }

    fn vtotal_highest_line(&self) -> u16 {
        self.vtotal & 0x07FF
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
        AgnusEcs, BEAMCON0_BLANKEN, BEAMCON0_CSCBEN, BEAMCON0_CSYTRUE, BEAMCON0_HARDDIS,
        BEAMCON0_HSYTRUE, BEAMCON0_LOLDIS, BEAMCON0_PAL, BEAMCON0_VARBEAMEN, BEAMCON0_VARCSYEN,
        BEAMCON0_VARHSYEN, BEAMCON0_VARVBEN, BEAMCON0_VARVSYEN, BEAMCON0_VSYTRUE, BPLCON0_LACE,
        PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME, PaulaReturnProgressPolicy, SlotOwner,
        UNWRITTEN_VERTICAL_BLANK_EDGE,
    };

    fn tick_programmed_line(agnus: &mut AgnusEcs) {
        for _ in 0..=agnus.htotal_highest_count() {
            agnus.tick_cck();
        }
    }

    fn enter_vertical_line(agnus: &mut AgnusEcs, vpos: u16) {
        agnus.vpos = vpos;
        agnus.evaluate_vertical_diw_comparators(vpos);
    }

    fn observe_ddf_start(agnus: &mut AgnusEcs) {
        let mask = if agnus.agnus_id >= 0x2000 {
            0x00FE
        } else {
            0x00FC
        };
        let start = agnus.ddfstrt & mask;
        assert!(start > 0, "test helper requires a non-zero DDFSTRT");
        agnus.hpos = start - 1;
        agnus.tick_cck();
        assert_eq!(agnus.ddf_start_match(), Some(start));
    }

    fn tick_to_hpos(agnus: &mut AgnusEcs, target: u16) {
        while agnus.hpos < target {
            agnus.tick_cck();
        }
        assert_eq!(agnus.hpos, target);
    }

    #[test]
    fn enhanced_right_ddf_limit_obeys_horizontal_hard_limit_policy() {
        for (case, beamcon0, bplcon0_mode, expected_end) in [
            ("default", BEAMCON0_PAL, 0x0000, Some(0x00DF)),
            (
                "VARVBEN is vertical only",
                BEAMCON0_PAL | BEAMCON0_VARVBEN,
                0x0000,
                Some(0x00DF),
            ),
            ("HARDDIS", BEAMCON0_PAL | BEAMCON0_HARDDIS, 0x0000, None),
            ("VARBEAMEN", BEAMCON0_PAL | BEAMCON0_VARBEAMEN, 0x0000, None),
            ("SHRES", BEAMCON0_PAL, 0x0040, None),
            ("UHRES", BEAMCON0_PAL, 0x0080, None),
        ] {
            let mut agnus = AgnusEcs::new();
            agnus.dmacon = 0x0300; // DMAEN | BPLEN
            agnus.bplcon0 = 0xC000 | bplcon0_mode; // hires, four planes
            agnus.ddfstrt = 0x0018;
            agnus.ddfstop = 0x00E0;
            agnus.write_beamcon0(beamcon0);

            observe_ddf_start(&mut agnus);
            tick_to_hpos(&mut agnus, 0x00D8);

            assert_eq!(
                agnus.ddf_fetch_end(),
                expected_end,
                "{case} horizontal hard-limit policy",
            );
            assert_eq!(
                agnus.ddf_start_match(),
                Some(0x0018),
                "{case} must retain the active fetch origin",
            );
            if expected_end.is_none() {
                tick_to_hpos(&mut agnus, 0x00E0);
                assert_eq!(
                    agnus.ddf_stop_match(),
                    Some(0x00E0),
                    "{case} bypass must leave the ordinary comparator armed",
                );
                assert_eq!(
                    agnus.ddf_fetch_end(),
                    Some(0x00E7),
                    "{case} bypass must terminate at programmed DDFSTOP",
                );
            }
        }
    }

    #[test]
    fn enhanced_right_ddf_limit_retains_the_matched_fetch_phase() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0300; // DMAEN | BPLEN
        agnus.bplcon0 = 0xC000; // hires, four planes
        agnus.ddfstrt = 0x001C;
        agnus.ddfstop = 0x00E8;

        observe_ddf_start(&mut agnus);
        tick_to_hpos(&mut agnus, 0x00D8);

        assert_eq!(agnus.ddf_fetch_end(), Some(0x00E3));
    }

    #[test]
    fn equal_ddf_boundaries_start_an_idle_enhanced_run() {
        for (case, beamcon0, expected_end, expected_post_df_plane) in [
            ("default hard limit", BEAMCON0_PAL, Some(0x00DF), None),
            ("HARDDIS", BEAMCON0_PAL | BEAMCON0_HARDDIS, None, Some(3)),
        ] {
            let mut agnus = AgnusEcs::new();
            agnus.dmacon = 0x0300; // DMAEN | BPLEN
            agnus.bplcon0 = 0xC000; // hires, four planes
            agnus.ddfstrt = 0x0038;
            agnus.ddfstop = 0x0038;
            agnus.vertical_diw_active = true;
            agnus.write_beamcon0(beamcon0);

            observe_ddf_start(&mut agnus);
            assert_eq!(agnus.ddf_stop_match(), None);
            tick_to_hpos(&mut agnus, 0x00D8);
            assert_eq!(agnus.ddf_fetch_end(), expected_end, "{case} endpoint");
            tick_to_hpos(&mut agnus, 0x00E0);
            assert_eq!(
                agnus.cck_bus_plan().bitplane_dma_fetch_plane,
                expected_post_df_plane,
                "{case} post-$DF arbitration",
            );
        }
    }

    /// BLTCON0L is a byte-write port — only the low byte updates,
    /// the high byte of BLTCON0 (shift amount + channel enables)
    /// must remain untouched. KS 2.x relies on this to change LF
    /// without re-issuing channel-enable bits.
    #[test]
    fn bltcon0l_updates_only_low_byte() {
        let mut agnus = AgnusEcs::new();
        agnus.inner.bltcon0 = 0x1234;
        agnus.write_bltcon0l(0x56AB);
        assert_eq!(agnus.inner.bltcon0, 0x12AB);
    }

    /// BLTSIZV just latches; doesn't trigger.
    #[test]
    fn bltsizv_latches_without_trigger() {
        let mut agnus = AgnusEcs::new();
        agnus.write_bltsizv(0x000B);
        assert_eq!(agnus.bltsizv, 0x000B);
        assert!(
            !agnus.inner.blitter_busy,
            "BLTSIZV alone must not start the blit"
        );
    }

    /// BLTSIZH triggers the blit using the previously-latched
    /// BLTSIZV. Packs into the legacy BLTSIZE encoding so the
    /// existing OCS blitter engine drives it.
    #[test]
    fn bltsizh_triggers_blit_using_latched_bltsizv() {
        let mut agnus = AgnusEcs::new();
        agnus.write_bltsizv(11); // V = 11 lines
        agnus.write_bltsizh(40); // H = 40 words; triggers
        // Legacy encoding: bits 15..6 = V (10), bits 5..0 = H (6).
        assert_eq!(agnus.inner.bltsize, (11 << 6) | 40);
        assert!(agnus.inner.blitter_busy, "BLTSIZH must start the blit");
    }

    /// BLTSIZV is sticky across consecutive BLTSIZH-triggered blits
    /// — a common pattern for text rendering (many same-height
    /// blits, one per glyph).
    #[test]
    fn bltsizv_is_sticky_across_bltsizh_blits() {
        let mut agnus = AgnusEcs::new();
        agnus.write_bltsizv(8);
        agnus.write_bltsizh(2); // first blit: 8 × 2
        agnus.inner.blitter_busy = false; // simulate completion
        agnus.write_bltsizh(2); // second blit: 8 × 2 again, no BLTSIZV between
        assert_eq!(agnus.inner.bltsize, (8 << 6) | 2);
    }

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
    fn effective_beamcon0_pal_default_matches_wrapped_region() {
        let pal = AgnusEcs::new();
        assert_eq!(pal.beamcon0() & BEAMCON0_PAL, BEAMCON0_PAL);
        assert!(!pal.lol_toggle);

        let ntsc = AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new_with_region(
            commodore_agnus_ocs::AgnusRegion::Ntsc,
        ));
        assert_eq!(ntsc.beamcon0() & BEAMCON0_PAL, 0);
        assert!(ntsc.lol_toggle);
    }

    #[test]
    fn set_pal_mode_preserves_other_beamcon0_bits() {
        let mut agnus = AgnusEcs::new();
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_LOLDIS);

        agnus.set_pal_mode(true);
        assert_eq!(
            agnus.beamcon0(),
            BEAMCON0_VARBEAMEN | BEAMCON0_LOLDIS | BEAMCON0_PAL
        );

        agnus.set_pal_mode(false);
        assert_eq!(agnus.beamcon0(), BEAMCON0_VARBEAMEN | BEAMCON0_LOLDIS);
    }

    #[test]
    fn ecs_register_latches_are_independent_of_ocs_core_state() {
        let mut agnus = AgnusEcs::new();
        assert_eq!(agnus.beamcon0(), BEAMCON0_PAL);
        assert_eq!(agnus.htotal(), PAL_CCKS_PER_LINE - 1);
        assert_eq!(agnus.hsstop(), 0);
        assert_eq!(agnus.vtotal(), PAL_LINES_PER_FRAME - 1);
        assert_eq!(agnus.vsstop(), 0);
        assert_eq!(agnus.hbstrt(), 0);
        assert_eq!(agnus.hbstop(), 0);
        assert_eq!(agnus.vbstrt(), UNWRITTEN_VERTICAL_BLANK_EDGE);
        assert_eq!(agnus.vbstop(), UNWRITTEN_VERTICAL_BLANK_EDGE);
        assert_eq!(agnus.hsstrt(), 0);
        assert_eq!(agnus.vsstrt(), 0);
        assert_eq!(agnus.diwhigh(), 0);
        assert!(!agnus.diwhigh_written());

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
        assert!(agnus.diwhigh_written());
        assert_eq!(agnus.diwstrt, 0);
        assert_eq!(agnus.diwstop, 0);
    }

    #[test]
    fn timing_register_dispatch_routes_supported_offsets() {
        let mut agnus = AgnusEcs::new();

        assert!(agnus.write_timing_register(0x1C0, 0x0033));
        assert!(agnus.write_timing_register(0x1C8, 0x0123));
        assert!(agnus.write_timing_register(0x1DC, BEAMCON0_VARBEAMEN));
        assert!(agnus.write_timing_register(0x1E4, 0x0101));
        assert!(!agnus.write_timing_register(0x1E2, 0x0044));

        assert_eq!(agnus.htotal(), 0x0033);
        assert_eq!(agnus.vtotal(), 0x0123);
        assert!(agnus.varbeamen_enabled());
        assert_eq!(agnus.diwhigh(), 0x0101);
        assert!(agnus.diwhigh_written());
    }

    #[test]
    fn beamcon0_blanken_and_polarity_helpers_reflect_latched_bits() {
        let mut agnus = AgnusEcs::new();
        assert!(!agnus.blanken_enabled());
        assert!(!agnus.cscben_enabled());
        assert!(!agnus.csytrue_enabled());
        assert!(!agnus.varcsyen_enabled());
        assert!(!agnus.vsytrue_enabled());
        assert!(!agnus.hsytrue_enabled());

        agnus.write_beamcon0(
            BEAMCON0_CSCBEN
                | BEAMCON0_VARCSYEN
                | BEAMCON0_BLANKEN
                | BEAMCON0_CSYTRUE
                | BEAMCON0_VSYTRUE
                | BEAMCON0_HSYTRUE,
        );

        assert!(agnus.blanken_enabled());
        assert!(agnus.cscben_enabled());
        assert!(agnus.csytrue_enabled());
        assert!(agnus.varcsyen_enabled());
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
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_PAL);

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
        assert_eq!(agnus.vbl_count, 1);
    }

    #[test]
    fn copper_horizontal_comparator_uses_programmed_htotal_wrap_origin() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(0x00FA);
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN);

        // Short line: L=$FB, even comparator period P=$FA, origin=$F8.
        for (physical, comparator) in [(0x00F7, 0x00F9), (0x00F8, 0x0000), (0x00FA, 0x0002)] {
            agnus.hpos = physical;
            assert_eq!(agnus.copper_comparator_hpos(), comparator);
        }

        // Long line: L=$FC, period P=$FC, origin moves to $FA.
        agnus.lol = true;
        assert_eq!(agnus.current_line_ccks(), 0x00FC);
        for (physical, comparator) in [(0x00F8, 0x00FA), (0x00FA, 0x0000), (0x00FB, 0x0001)] {
            agnus.hpos = physical;
            assert_eq!(agnus.copper_comparator_hpos(), comparator);
        }

        // An odd programmed highest count produces an even short line, so
        // LOL does not move the comparator period on the following odd line.
        agnus.write_htotal(0x00F9);
        for lol in [false, true] {
            agnus.lol = lol;
            agnus.hpos = 0x00F8;
            assert_eq!(agnus.copper_comparator_hpos(), 0);
        }
    }

    #[test]
    fn copper_horizontal_comparator_handles_degenerate_programmed_totals() {
        let mut agnus = AgnusEcs::new();
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN);

        agnus.write_htotal(0);
        agnus.lol = false;
        agnus.hpos = 0;
        assert_eq!(agnus.current_line_ccks(), 1);
        assert_eq!(agnus.copper_comparator_hpos(), 0);

        agnus.lol = true;
        agnus.hpos = 1;
        assert_eq!(agnus.current_line_ccks(), 2);
        assert_eq!(agnus.copper_comparator_hpos(), 1);

        agnus.write_htotal(1);
        agnus.lol = false;
        assert_eq!(agnus.current_line_ccks(), 2);
        agnus.hpos = 1;
        assert_eq!(agnus.copper_comparator_hpos(), 1);

        agnus.lol = true;
        assert_eq!(agnus.current_line_ccks(), 3);
        agnus.hpos = 2;
        assert_eq!(agnus.copper_comparator_hpos(), 0);
    }

    #[test]
    fn varbeamen_distinguishes_explicit_zero_totals_from_unwritten_defaults() {
        let mut agnus = AgnusEcs::new();
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_PAL);

        // Unwritten timing registers retain the region defaults.
        agnus.tick_cck();
        assert_eq!((agnus.vpos, agnus.hpos), (0, 1));

        // Zero is nevertheless a valid programmed highest count: one
        // horizontal position in a one-line field.
        agnus.hpos = 0;
        agnus.write_htotal(0);
        agnus.write_vtotal(0);
        agnus.tick_cck();
        assert_eq!((agnus.vpos, agnus.hpos), (0, 0));
        assert_eq!(agnus.vbl_count, 1);
    }

    #[test]
    fn varbeamen_preserves_interlace_field_lengths_and_lof() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1); // Two CCKs per line.
        agnus.write_vtotal(1); // Short field is VTOTAL + 1 = 2 lines.
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_PAL);
        agnus.bplcon0 = 0x0004; // LACE

        // LOF starts set, so the first field is the long field:
        // VTOTAL + 2 = 3 lines.
        for _ in 0..2 {
            tick_programmed_line(&mut agnus);
        }
        assert_eq!(agnus.vpos, 2);
        assert_eq!(agnus.vbl_count, 0);
        assert!(agnus.lof);

        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 0);
        assert_eq!(agnus.vbl_count, 1);
        assert!(!agnus.lof);

        // The following short field contains exactly VTOTAL + 1 lines.
        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 1);
        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 0);
        assert_eq!(agnus.vbl_count, 2);
        assert!(agnus.lof);
    }

    #[test]
    fn varbeamen_runs_sprite_lifecycle_against_programmed_field_length() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1);
        agnus.write_vtotal(27); // 28-line field; final line is 27.
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_PAL);
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.poke_sprite_pos(0, 26 << 8);
        agnus.poke_sprite_ctl(0, 40 << 8);

        for _ in 0..26 {
            tick_programmed_line(&mut agnus);
        }
        assert_eq!(agnus.vpos, 26);
        assert!(agnus.sprite_dma_on(0), "sprite activates at VSTART");

        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 27);
        assert!(
            !agnus.sprite_dma_on(0),
            "sprite shuts down on the programmed final line"
        );
    }

    #[test]
    fn varbeamen_sprite_shutdown_tracks_long_and_short_interlace_fields() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1);
        agnus.write_vtotal(27); // 28-line short field, 29-line long field.
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_PAL);
        agnus.bplcon0 = 0x0004; // LACE; LOF starts on the long field.
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.poke_sprite_pos(0, 26 << 8);
        agnus.poke_sprite_ctl(0, 40 << 8);

        for _ in 0..26 {
            tick_programmed_line(&mut agnus);
        }
        assert!(agnus.sprite_dma_on(0));
        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 27);
        assert!(
            agnus.sprite_dma_on(0),
            "long field keeps the sprite active on its penultimate line"
        );
        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 28);
        assert!(
            !agnus.sprite_dma_on(0),
            "long field shuts the sprite down on its extra final line"
        );
        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 0);
        assert!(!agnus.lof);

        for _ in 0..26 {
            tick_programmed_line(&mut agnus);
        }
        assert!(agnus.sprite_dma_on(0));
        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 27);
        assert!(
            !agnus.sprite_dma_on(0),
            "short field shuts the sprite down one line earlier"
        );
    }

    #[test]
    fn varbeamen_preserves_ntsc_long_line_state_transition() {
        let mut agnus = AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new_with_region(
            commodore_agnus_ocs::AgnusRegion::Ntsc,
        ));
        agnus.write_htotal(1);
        agnus.write_vtotal(3);
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN);

        for expected_lol in [true, false] {
            let line_ccks = agnus.htotal_highest_count() + 1 + u16::from(agnus.lol);
            for _ in 0..line_ccks {
                agnus.tick_cck();
            }
            assert_eq!(agnus.hpos, 0);
            assert_eq!(agnus.lol, expected_lol);
        }
    }

    #[test]
    fn varbeamen_ntsc_alternates_short_and_long_programmed_lines() {
        let mut agnus = AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new_with_region(
            commodore_agnus_ocs::AgnusRegion::Ntsc,
        ));
        agnus.write_htotal(1); // Two CCK short line, three CCK long line.
        agnus.write_vtotal(7);
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN);

        for _ in 0..2 {
            agnus.tick_cck();
        }
        assert_eq!((agnus.vpos, agnus.hpos), (1, 0));
        assert!(agnus.lol);

        for _ in 0..2 {
            agnus.tick_cck();
        }
        assert_eq!(
            (agnus.vpos, agnus.hpos),
            (1, 2),
            "long line has one extra CCK"
        );
        agnus.tick_cck();
        assert_eq!((agnus.vpos, agnus.hpos), (2, 0));
        assert!(!agnus.lol);
    }

    #[test]
    fn pal_and_loldis_each_disable_long_line_toggle() {
        for disable_bit in [BEAMCON0_PAL, BEAMCON0_LOLDIS] {
            let mut agnus = AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new_with_region(
                commodore_agnus_ocs::AgnusRegion::Ntsc,
            ));
            agnus.write_beamcon0(disable_bit);

            for _ in 0..PAL_CCKS_PER_LINE {
                agnus.tick_cck();
            }
            assert_eq!((agnus.vpos, agnus.hpos), (1, 0));
            assert!(
                !agnus.lol,
                "BEAMCON0 bit {disable_bit:#06x} forces short lines"
            );
        }
    }

    #[test]
    fn loldis_finishes_current_long_line_then_forces_short_lines() {
        let mut agnus = AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new_with_region(
            commodore_agnus_ocs::AgnusRegion::Ntsc,
        ));
        agnus.write_htotal(1);
        agnus.write_vtotal(7);
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN);
        tick_programmed_line(&mut agnus);
        assert!(agnus.lol);

        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_LOLDIS);
        for _ in 0..2 {
            agnus.tick_cck();
        }
        assert_eq!((agnus.vpos, agnus.hpos), (1, 2));
        assert!(agnus.lol);

        agnus.tick_cck();
        assert_eq!((agnus.vpos, agnus.hpos), (2, 0));
        assert!(!agnus.lol);
        tick_programmed_line(&mut agnus);
        assert_eq!((agnus.vpos, agnus.hpos), (3, 0));
        assert!(!agnus.lol);
    }

    #[test]
    fn diwhigh_switches_vertical_dma_decode_from_legacy_to_explicit_high_bits() {
        let mut agnus = AgnusEcs::new();
        agnus.write_diwstrt(0x2010);
        agnus.write_diwstop(0xA020);

        enter_vertical_line(&mut agnus, 0x0020);
        assert!(agnus.bitplane_dma_vertical_active());
        enter_vertical_line(&mut agnus, 0x00A0);
        assert!(!agnus.bitplane_dma_vertical_active());

        agnus.write_diwhigh(0x0101);

        enter_vertical_line(&mut agnus, 0x0120);
        assert!(agnus.bitplane_dma_vertical_active());
        enter_vertical_line(&mut agnus, 0x01A0);
        assert!(!agnus.bitplane_dma_vertical_active());
    }

    #[test]
    fn explicit_zero_diwhigh_uses_direct_vertical_stop_bits() {
        let mut unwritten = AgnusEcs::new();
        unwritten.write_diwstrt(0x0110);
        unwritten.write_diwstop(0x0220);

        enter_vertical_line(&mut unwritten, 0x0001);
        enter_vertical_line(&mut unwritten, 0x0002);
        assert!(
            unwritten.bitplane_dma_vertical_active(),
            "without DIWHIGH, legacy decoding makes VSTOP=$102",
        );
        enter_vertical_line(&mut unwritten, 0x0102);
        assert!(!unwritten.bitplane_dma_vertical_active());

        let mut agnus = AgnusEcs::new();
        agnus.write_diwstrt(0x0110);
        agnus.write_diwstop(0x0220);
        agnus.write_diwhigh(0x0000);

        enter_vertical_line(&mut agnus, 0x0001);
        assert!(agnus.bitplane_dma_vertical_active());
        enter_vertical_line(&mut agnus, 0x0002);
        assert!(
            !agnus.bitplane_dma_vertical_active(),
            "an explicit zero DIWHIGH makes VSTOP=$002, not legacy $102",
        );
    }

    #[test]
    fn hard_vertical_blank_closes_vertical_diw_before_frame_zero() {
        let mut agnus = AgnusEcs::new();
        agnus.write_diwstrt(0x2C10);
        agnus.write_diwstop(0x1020);
        agnus.write_diwhigh(0x0001);

        enter_vertical_line(&mut agnus, 0x012C);
        assert!(agnus.bitplane_dma_vertical_active());

        enter_vertical_line(&mut agnus, PAL_LINES_PER_FRAME - 1);
        assert!(
            !agnus.bitplane_dma_vertical_active(),
            "hard vertical blank starts on the final fixed-timing line",
        );
        enter_vertical_line(&mut agnus, 0x0000);
        assert!(
            !agnus.bitplane_dma_vertical_active(),
            "vertical DIW remains closed after the field wraps",
        );
        enter_vertical_line(&mut agnus, 0x0005);
        assert!(!agnus.bitplane_dma_vertical_active());
    }

    #[test]
    fn interlace_long_field_moves_hard_blank_to_the_extra_line() {
        let mut agnus = AgnusEcs::new();
        agnus.bplcon0 = BPLCON0_LACE;
        agnus.lof = true;
        agnus.write_diwstrt(0x2C10);
        agnus.write_diwstop(0x1020);
        agnus.write_diwhigh(0x0001);

        enter_vertical_line(&mut agnus, 0x012C);
        enter_vertical_line(&mut agnus, PAL_LINES_PER_FRAME - 1);
        assert!(
            agnus.vertical_diw_active(),
            "the penultimate line remains outside hard blank in a long field",
        );
        enter_vertical_line(&mut agnus, PAL_LINES_PER_FRAME);
        assert!(
            !agnus.vertical_diw_active(),
            "the interlace extension is the long field's hard-blank line",
        );

        agnus.lof = false;
        enter_vertical_line(&mut agnus, 0x012C);
        enter_vertical_line(&mut agnus, PAL_LINES_PER_FRAME - 1);
        assert!(
            !agnus.vertical_diw_active(),
            "the short field still blanks on its ordinary final line",
        );
    }

    #[test]
    fn programmed_vtotal_can_make_extended_diwhigh_start_reachable() {
        let mut agnus = AgnusEcs::new();
        agnus.write_diwstrt(0x2010);
        agnus.write_diwstop(0x1020);
        agnus.write_diwhigh(0x0003); // VSTART=$320, VSTOP=$010

        assert!(
            !agnus.bitplane_dma_vertical_active(),
            "VSTART=$320 is unreachable with the standard PAL total",
        );

        agnus.write_vtotal(0x03FF);
        agnus.write_beamcon0(BEAMCON0_VARBEAMEN | BEAMCON0_PAL);
        enter_vertical_line(&mut agnus, 0x0320);
        assert!(agnus.bitplane_dma_vertical_active());
        enter_vertical_line(&mut agnus, 0x0005);
        assert!(
            agnus.bitplane_dma_vertical_active(),
            "VARBEAMEN disables the fixed hard-blank boundary",
        );
        enter_vertical_line(&mut agnus, 0x0010);
        assert!(!agnus.bitplane_dma_vertical_active());
    }

    #[test]
    fn unreachable_diwhigh_start_keeps_bitplane_dma_closed() {
        let mut agnus = AgnusEcs::new();
        agnus.hpos = 0x39; // DDFSTRT + 1 => BPL2 in a hires fetch group
        agnus.vpos = 0x002B;
        agnus.dmacon = 0x0300; // DMAEN | BPLEN
        agnus.bplcon0 = 0xA302; // HIRES + 2 bitplanes + COLOR
        agnus.ddfstrt = 0x0038;
        agnus.ddfstop = 0x00D8;
        agnus.write_diwstrt(0x2C81);
        agnus.write_diwstop(0x2CC1);

        // graphics.library uses this value while replacing a Copper
        // display list. VSTART decodes to $F2C, beyond the PAL field,
        // so its comparator cannot open the DMA window. Treating the
        // ordered pair as a stateless wrapping range would incorrectly
        // grant a partial line before the replacement pointers settle.
        agnus.write_diwhigh(0x00FF);
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            None,
            "an unreachable VSTART comparator must leave the window closed",
        );

        // Restoring the normal ECS extension re-establishes the
        // $02C..$12C window: line $02B remains closed and line $02C opens.
        agnus.write_diwhigh(0x2100);
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);
        enter_vertical_line(&mut agnus, 0x002C);
        observe_ddf_start(&mut agnus);
        agnus.hpos = 0x0039;
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, Some(1),);
    }

    #[test]
    fn diwhigh_write_preserves_an_already_open_vertical_diw_latch() {
        let mut agnus = AgnusEcs::new();
        agnus.write_diwstrt(0x2C81);
        agnus.write_diwstop(0x2CC1);
        agnus.write_diwhigh(0x2100);
        enter_vertical_line(&mut agnus, 0x002C);
        assert!(agnus.vertical_diw_active());

        agnus.vpos = 0x0064;
        agnus.write_diwhigh(0x00FF);
        assert!(
            agnus.vertical_diw_active(),
            "changing comparators must not reconstruct an open latch from position",
        );

        enter_vertical_line(&mut agnus, PAL_LINES_PER_FRAME - 1);
        assert!(
            !agnus.vertical_diw_active(),
            "fixed vertical-blank start must still close the open latch",
        );
        enter_vertical_line(&mut agnus, 0x0000);
        assert!(!agnus.vertical_diw_active());
    }

    #[test]
    fn equal_vertical_diw_comparators_leave_latch_closed() {
        let mut agnus = AgnusEcs::new();
        agnus.write_diwstrt(0x2C81);
        agnus.write_diwstop(0x2CC1);
        agnus.write_diwhigh(0x0101);

        enter_vertical_line(&mut agnus, 0x012C);
        assert!(!agnus.vertical_diw_active());
    }

    #[test]
    fn base_diw_write_returns_vertical_decode_to_implicit_high_bits() {
        let mut agnus = AgnusEcs::new();
        agnus.write_diwhigh(0x2100);
        assert!(agnus.diwhigh_written());

        agnus.write_diwstrt(0x2C81);
        assert!(!agnus.diwhigh_written());

        agnus.write_diwhigh(0x2100);
        agnus.write_diwstop(0x2CC1);
        assert!(!agnus.diwhigh_written());
    }

    #[test]
    fn top_of_field_vstart_can_reopen_after_hard_blank_start_event() {
        let mut pal = AgnusEcs::new();
        pal.write_diwstrt(0x1481);
        pal.write_diwstop(0x64C1);
        enter_vertical_line(&mut pal, 20);
        assert!(
            pal.vertical_diw_active(),
            "hard blank is a close event, not a level gate over early PAL lines",
        );

        let mut ntsc = AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new_with_region(
            commodore_agnus_ocs::AgnusRegion::Ntsc,
        ));
        ntsc.write_diwstrt(0x1481);
        ntsc.write_diwstop(0x64C1);
        enter_vertical_line(&mut ntsc, 20);
        assert!(
            ntsc.vertical_diw_active(),
            "the same event-driven rule applies to NTSC top-of-field lines",
        );
    }

    #[test]
    fn programmable_vertical_modes_disable_fixed_hard_blank() {
        for mode in [BEAMCON0_VARBEAMEN, BEAMCON0_VARVBEN, BEAMCON0_HARDDIS] {
            let mut agnus = AgnusEcs::new();
            agnus.vpos = PAL_LINES_PER_FRAME - 1;
            agnus.write_diwstrt(0x3710);
            agnus.write_diwstop(0x1020);
            agnus.write_diwhigh(0x0001); // VSTART=$137, the PAL final line.
            assert!(!agnus.vertical_diw_active());

            agnus.write_beamcon0(BEAMCON0_PAL | mode);
            assert!(
                agnus.vertical_diw_active(),
                "BEAMCON0 mode {mode:#06x} should disable the fixed boundary",
            );
        }
    }

    #[test]
    fn cck_bus_plan_demotes_bitplane_slot_when_diwhigh_moves_dma_window() {
        let mut agnus = AgnusEcs::new();
        agnus.hpos = 0x23; // ddfstrt + 7 => BPL1 slot in lowres fetch group
        agnus.vpos = 0x0020;
        agnus.dmacon = 0x0300; // DMAEN | BPLEN
        agnus.bplcon0 = 1 << 12; // 1 bitplane enabled
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0x1C;
        agnus.write_diwstrt(0x2010);
        agnus.write_diwstop(0xA020);
        enter_vertical_line(&mut agnus, 0x0020);
        observe_ddf_start(&mut agnus);
        agnus.hpos = 0x0023;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Bitplane(0));
        assert_eq!(plan.bitplane_dma_fetch_plane, Some(0));
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Stall
        );

        enter_vertical_line(&mut agnus, PAL_LINES_PER_FRAME - 1);
        enter_vertical_line(&mut agnus, 0x0000);
        agnus.vpos = 0x0020;
        agnus.write_diwhigh(0x0101);

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
        assert!(plan.cpu_chip_bus_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Advance
        );
    }

    #[test]
    fn diwhigh_demoted_bitplane_slot_falls_through_to_requesting_sprite() {
        let mut agnus = AgnusEcs::new();
        agnus.hpos = 0x23; // BPL1 and sprite 3 overlap
        agnus.vpos = 0x0020;
        agnus.dmacon = 0x0320; // DMAEN | BPLEN | SPREN
        agnus.bplcon0 = 1 << 12;
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0x1C;
        agnus.write_diwstrt(0x1010);
        agnus.write_diwstop(0xA020);
        agnus.poke_sprite_ctl(3, 0x2000); // VSTOP=$20: control fetch requested

        agnus.write_diwhigh(0x0101);
        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Sprite(3));
        assert_eq!(plan.sprite_dma_service_channel, Some(3));
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
    }

    #[test]
    fn diwhigh_can_promote_bitplane_slot_outside_legacy_window() {
        let mut agnus = AgnusEcs::new();
        agnus.hpos = 0x23;
        agnus.vpos = 0x0120;
        agnus.dmacon = 0x0300;
        agnus.bplcon0 = 1 << 12;
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0x1C;
        agnus.write_diwstrt(0x1010);
        agnus.write_diwstop(0xA020);
        agnus.write_diwhigh(0x0101);
        enter_vertical_line(&mut agnus, 0x0110);
        observe_ddf_start(&mut agnus);
        agnus.hpos = 0x0023;
        agnus.vpos = 0x0120;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Bitplane(0));
        assert_eq!(plan.bitplane_dma_fetch_plane, Some(0));
    }

    #[test]
    fn enhanced_sprite_ctl_bits_extend_direct_vertical_coordinates() {
        let mut agnus = AgnusEcs::new();

        // CTL bit 6 supplies VSTART[9]. Writing POS afterwards must retain
        // both high VSTART bits while replacing the low byte.
        agnus.poke_sprite_ctl(0, 0x0246);
        agnus.poke_sprite_pos(0, 0x0100);
        assert_eq!(agnus.sprite_vstart(0), 0x301);
        assert_eq!(agnus.sprite_vstop(0), 0x102);

        // CTL bit 5 independently supplies VSTOP[9].
        agnus.poke_sprite_ctl(0, 0x0226);
        assert_eq!(agnus.sprite_vstart(0), 0x101);
        assert_eq!(agnus.sprite_vstop(0), 0x302);

        // A subsequent CTL write replaces both enhanced bits rather than
        // preserving stale V9 state.
        agnus.poke_sprite_ctl(0, 0x0206);
        assert_eq!(agnus.sprite_vstart(0), 0x101);
        assert_eq!(agnus.sprite_vstop(0), 0x102);
    }

    #[test]
    fn enhanced_sprite_ctl_bits_extend_dma_fetched_vertical_coordinates() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.spr_pt[0] = 0x1000;
        agnus.vpos = 30;
        agnus.poke_sprite_ctl(0, 30 << 8);

        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x0100),
            Some((true, 0x0100))
        );
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, true, 1, |_| 0x0266),
            Some((true, 0x0266))
        );
        assert_eq!(agnus.sprite_vstart(0), 0x301);
        assert_eq!(agnus.sprite_vstop(0), 0x302);
    }

    #[test]
    fn enhanced_sprite_comparators_follow_ten_bit_programmed_beam() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(0);
        agnus.write_vtotal(0x03FF);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN);
        agnus.poke_sprite_pos(0, 0x0100);
        agnus.poke_sprite_ctl(0, 0x0266); // VSTART=$301, VSTOP=$302
        agnus.vpos = 0x0300;

        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 0x0301);
        assert!(
            agnus.sprite_dma_on(0),
            "VSTART[9] participates in comparison"
        );

        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 0x0302);
        assert!(
            !agnus.sprite_dma_on(0),
            "VSTOP[9] participates in comparison"
        );
    }

    #[test]
    fn enhanced_sprite_comparators_do_not_alias_above_ten_bits() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(0);
        agnus.write_vtotal(0x07FF);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN);
        agnus.poke_sprite_pos(0, 0xFF00);
        agnus.poke_sprite_ctl(0, 0x0144); // VSTART=$3FF, VSTOP=$001
        agnus.vpos = 0x03FE;

        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 0x03FF);
        assert!(
            agnus.sprite_dma_on(0),
            "the sprite must first activate at its exact ten-bit VSTART"
        );

        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 0x0400);
        assert!(agnus.sprite_dma_on(0));

        tick_programmed_line(&mut agnus);

        assert_eq!(agnus.vpos, 0x0401);
        assert!(
            agnus.sprite_dma_on(0),
            "VSTOP=$001 must not alias onto beam $401"
        );
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
    fn varvben_replaces_fixed_sprite_control_refetch_boundary() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.hpos = 0x15;
        agnus.write_vbstrt(300);
        agnus.write_vbstop(40);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);

        agnus.vpos = 25;
        assert_eq!(
            agnus.cck_bus_plan().slot_owner,
            SlotOwner::Cpu,
            "the fixed PAL boundary must stop generating requests"
        );

        agnus.vpos = 39;
        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        agnus.hpos = 0x15;
        assert_eq!(
            agnus.cck_bus_plan().slot_owner,
            SlotOwner::Sprite(0),
            "VBSTOP must become the sprite control-refetch boundary"
        );
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x4000),
            Some((true, 0x4000))
        );
    }

    #[test]
    fn programmed_sprite_boundary_is_region_independent_and_fixed_boundary_restores() {
        for (region, fixed_line, beamcon0) in [
            (commodore_agnus_ocs::AgnusRegion::Pal, 25, BEAMCON0_PAL),
            (commodore_agnus_ocs::AgnusRegion::Ntsc, 20, 0),
        ] {
            let inner = commodore_agnus_ocs::Agnus::new_with_region(region);
            let mut agnus = AgnusEcs::from_ocs(inner);
            agnus.dmacon = 0x0220; // DMAEN | SPREN
            agnus.hpos = 0x15;
            agnus.write_vbstrt(300);
            agnus.write_vbstop(40);
            agnus.write_beamcon0(beamcon0 | BEAMCON0_VARVBEN);

            agnus.vpos = fixed_line;
            assert_eq!(agnus.cck_bus_plan().slot_owner, SlotOwner::Cpu);
            agnus.vpos = 39;
            agnus.hpos = agnus.current_line_ccks() - 1;
            agnus.tick_cck();
            agnus.hpos = 0x15;
            assert_eq!(agnus.cck_bus_plan().slot_owner, SlotOwner::Sprite(0));

            agnus.write_beamcon0(beamcon0);
            agnus.vpos = fixed_line;
            assert_eq!(
                agnus.cck_bus_plan().slot_owner,
                SlotOwner::Sprite(0),
                "clearing VARVBEN restores the regional fixed boundary"
            );
        }
    }

    #[test]
    fn guest_pal_bit_selects_the_fixed_sprite_boundary() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.hpos = 0x15;

        agnus.write_beamcon0(0); // guest selects NTSC fixed timing
        agnus.vpos = 20;
        assert_eq!(agnus.cck_bus_plan().slot_owner, SlotOwner::Sprite(0));

        agnus.set_pal_mode(true);
        agnus.vpos = 20;
        assert_eq!(agnus.cck_bus_plan().slot_owner, SlotOwner::Cpu);
        agnus.vpos = 25;
        assert_eq!(agnus.cck_bus_plan().slot_owner, SlotOwner::Sprite(0));
    }

    #[test]
    fn unrelated_vertical_write_does_not_arm_unwritten_blank_edges() {
        let mut agnus = AgnusEcs::new();
        agnus.write_vtotal(311);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.spr_pt[0] = 0x1000;
        agnus.poke_sprite_pos(0, 50 << 8);
        agnus.poke_sprite_ctl(0, 100 << 8);
        agnus.vpos = agnus.lines_per_frame - 1;
        agnus.hpos = agnus.current_line_ccks() - 1;

        agnus.tick_cck();

        assert_eq!(agnus.vpos, 0);
        assert!(
            !agnus.programmed_vblank_stop_event(),
            "an unrelated timing write must not turn an unwritten VBSTOP into a line-zero edge"
        );
        agnus.hpos = 0x15;
        assert_eq!(
            agnus.cck_bus_plan().slot_owner,
            SlotOwner::Cpu,
            "an unwritten blank comparator must not request sprite control"
        );

        let mut agnus = AgnusEcs::new();
        agnus.write_vtotal(0x07FF);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN | BEAMCON0_VARVBEN);
        agnus.dmacon = 0x0220;
        agnus.poke_sprite_pos(0, 50 << 8);
        agnus.poke_sprite_ctl(0, 100 << 8);
        agnus.vpos = 0x07FE;
        agnus.hpos = agnus.current_line_ccks() - 1;

        agnus.tick_cck();

        assert_eq!(agnus.vpos, 0x07FF);
        assert!(
            !agnus.programmed_vblank_stop_event(),
            "the reset sentinel must remain outside the 11-bit comparator domain"
        );
        agnus.hpos = 0x15;
        assert_eq!(agnus.cck_bus_plan().slot_owner, SlotOwner::Cpu);
    }

    #[test]
    fn programmed_blank_latch_survives_wrap_when_stop_is_unreachable() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1);
        agnus.write_vtotal(311);
        agnus.write_vbstrt(300);
        agnus.write_vbstop(400);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN | BEAMCON0_VARVBEN);
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        // VSTART=299, VSTOP=500: latent data state would span the wrap.
        agnus.poke_sprite_pos(0, 43 << 8);
        agnus.poke_sprite_ctl(0, (244 << 8) | 0x0006);
        agnus.vpos = 298;

        tick_programmed_line(&mut agnus);
        assert!(agnus.sprite_dma_on(0));
        for _ in 299..312 {
            tick_programmed_line(&mut agnus);
        }
        assert_eq!(agnus.vpos, 0);
        agnus.hpos = 0x15;
        assert_eq!(
            agnus.cck_bus_plan().slot_owner,
            SlotOwner::Cpu,
            "blank stays latched because the programmed stop edge was never reached"
        );
    }

    #[test]
    fn programming_blank_edges_mid_line_does_not_reconstruct_the_latch() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.hpos = 0x15;
        agnus.vpos = 350;
        agnus.poke_sprite_pos(0, 94 << 8);
        agnus.poke_sprite_ctl(0, (244 << 8) | 0x0006);
        assert!(agnus.sprite_dma_on(0));

        agnus.write_vbstrt(300);
        agnus.write_vbstop(400);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);

        assert_eq!(
            agnus.cck_bus_plan().slot_owner,
            SlotOwner::Sprite(0),
            "enabling VARVBEN selects the tracked latch, not a geometric range"
        );
    }

    #[test]
    fn writing_vbstop_to_current_line_does_not_manufacture_reset_event() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.hpos = 0x15;
        agnus.vpos = 40;
        agnus.spr_pt[0] = 0x1000;
        agnus.write_vbstrt(100);
        agnus.write_vbstop(200);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);
        agnus.poke_sprite_pos(0, 40 << 8);
        agnus.poke_sprite_ctl(0, 60 << 8);
        assert!(agnus.sprite_dma_on(0));

        agnus.write_vbstop(40);

        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0xA55A),
            Some((false, 0xA55A)),
            "register writes take effect at a future line-entry comparison"
        );
    }

    #[test]
    fn changing_vbstop_mid_line_does_not_cancel_latched_reset_event() {
        let mut agnus = AgnusEcs::new();
        agnus.spr_pt[0] = 0x1000;
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.write_vbstrt(35);
        agnus.write_vbstop(40);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);
        agnus.vpos = 39;
        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.vpos, 40);

        agnus.write_vbstop(60);

        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x4000),
            Some((true, 0x4000)),
            "the line-held stop event survives a later register write"
        );
    }

    #[test]
    fn varvben_resets_active_sprite_when_beam_enters_vbstop_line() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1);
        agnus.write_vtotal(311);
        agnus.write_vbstrt(35);
        agnus.write_vbstop(40);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN | BEAMCON0_VARVBEN);
        agnus.poke_sprite_pos(0, 30 << 8);
        agnus.poke_sprite_ctl(0, 60 << 8);
        agnus.vpos = 29;

        tick_programmed_line(&mut agnus);
        assert!(agnus.sprite_dma_on(0), "VSTART activates the sprite");

        for _ in 30..40 {
            tick_programmed_line(&mut agnus);
        }
        assert_eq!(agnus.vpos, 40);
        assert!(
            !agnus.sprite_dma_on(0),
            "the programmed reset boundary clears active sprite data"
        );
    }

    #[test]
    fn programmed_vbstop_event_wins_over_same_line_sprite_vstart() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1);
        agnus.write_vtotal(311);
        agnus.write_vbstrt(35);
        agnus.write_vbstop(40);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN | BEAMCON0_VARVBEN);
        agnus.poke_sprite_pos(0, 40 << 8);
        agnus.poke_sprite_ctl(0, 60 << 8);
        agnus.vpos = 39;

        tick_programmed_line(&mut agnus);

        assert_eq!(agnus.vpos, 40);
        assert!(
            !agnus.sprite_dma_on(0),
            "the reset event must take precedence over VSTART"
        );
    }

    #[test]
    fn varvben_direct_sprite_writes_respect_programmed_vertical_blank() {
        let mut agnus = AgnusEcs::new();
        agnus.write_vbstrt(20);
        agnus.write_vbstop(40);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);
        agnus.vpos = 19;
        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert!(agnus.programmed_vblank_active());
        agnus.poke_sprite_ctl(0, 60 << 8);
        agnus.vpos = 30;

        agnus.poke_sprite_pos(0, 30 << 8);

        assert!(
            !agnus.sprite_dma_on(0),
            "direct comparator writes inside programmed blank must not activate data"
        );
    }

    #[test]
    fn equal_programmed_blank_edges_still_request_sprite_control() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.hpos = 0x15;
        agnus.vpos = 50;
        agnus.write_vbstrt(50);
        agnus.write_vbstop(50);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);
        agnus.vpos = 49;
        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        agnus.hpos = 0x15;

        assert!(
            !agnus.vblank_window_active(50),
            "equal edges describe an empty blank interval"
        );
        assert_eq!(
            agnus.cck_bus_plan().slot_owner,
            SlotOwner::Sprite(0),
            "the VBSTOP edge still resets and refetches sprite control"
        );
    }

    #[test]
    fn zero_vbstop_refetches_control_on_line_zero_then_allows_line_one_data() {
        let mut agnus = AgnusEcs::new();
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.spr_pt[0] = 0x1000;
        agnus.write_vbstrt(300);
        agnus.write_vbstop(0);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARVBEN);
        agnus.poke_sprite_ctl(0, 99 << 8);
        agnus.vpos = agnus.lines_per_frame - 1;
        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.vpos, 0);
        assert!(agnus.programmed_vblank_stop_event());

        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x0100),
            Some((true, 0x0100))
        );
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, true, 1, |_| 0x0200),
            Some((true, 0x0200))
        );
        assert!(!agnus.sprite_dma_on(0));

        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.vpos, 1);
        assert!(agnus.sprite_dma_on(0), "VSTART=1 enables line-one data");
        agnus.hpos = 0x15;
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0xA55A),
            Some((false, 0xA55A))
        );
    }

    #[test]
    fn programmed_blank_suppresses_sprite_vstop_comparator_and_bus_request() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1);
        agnus.write_vtotal(311);
        agnus.write_vbstrt(35);
        agnus.write_vbstop(40);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN | BEAMCON0_VARVBEN);
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.poke_sprite_pos(0, 30 << 8);
        agnus.poke_sprite_ctl(0, 37 << 8);
        agnus.vpos = 29;

        tick_programmed_line(&mut agnus);
        assert!(agnus.sprite_dma_on(0));
        for _ in 30..37 {
            tick_programmed_line(&mut agnus);
        }
        assert_eq!(agnus.vpos, 37);
        assert!(
            agnus.sprite_dma_on(0),
            "an ordinary VSTOP match inside blank is suppressed"
        );
        agnus.hpos = 0x15;
        assert_eq!(
            agnus.cck_bus_plan().slot_owner,
            SlotOwner::Cpu,
            "latent sprite state cannot claim a bus slot during blank"
        );
    }

    #[test]
    fn programmable_sprite_state_can_cross_the_field_boundary() {
        let mut agnus = AgnusEcs::new();
        agnus.write_htotal(1);
        agnus.write_vtotal(311);
        agnus.write_vbstrt(20);
        agnus.write_vbstop(300);
        agnus.write_beamcon0(BEAMCON0_PAL | BEAMCON0_VARBEAMEN | BEAMCON0_VARVBEN);
        // VSTART=301, VSTOP=10: the active interval crosses counter wrap.
        agnus.poke_sprite_pos(0, 45 << 8);
        agnus.poke_sprite_ctl(0, (10 << 8) | 0x0004);
        agnus.vpos = 300;

        tick_programmed_line(&mut agnus);
        assert_eq!(agnus.vpos, 301);
        assert!(agnus.sprite_dma_on(0), "VSTART activates after VBSTOP");

        for _ in 301..312 {
            tick_programmed_line(&mut agnus);
        }
        assert_eq!(agnus.vpos, 0);
        assert!(
            agnus.sprite_dma_on(0),
            "counter wrap alone must not manufacture a programmable reset"
        );

        for _ in 0..10 {
            tick_programmed_line(&mut agnus);
        }
        assert_eq!(agnus.vpos, 10);
        assert!(!agnus.sprite_dma_on(0), "VSTOP still ends the sprite");
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

    #[test]
    fn sync_polarity_hsytrue_inverts_output() {
        let mut agnus = AgnusEcs::new();
        agnus.write_hsstrt(30);
        agnus.write_hsstop(40);
        // Enable VARHSYEN but NOT HSYTRUE → active-low output.
        agnus.write_beamcon0(BEAMCON0_VARHSYEN);

        // Inside sync window: raw=true, inverted→false.
        let pins = agnus.sync_pin_levels(35, 0);
        assert!(!pins.hsync, "HSYTRUE=0: active-low, inside window → false");

        // Outside sync window: raw=false, inverted→true.
        let pins = agnus.sync_pin_levels(50, 0);
        assert!(pins.hsync, "HSYTRUE=0: active-low, outside window → true");

        // Now enable HSYTRUE → active-high.
        agnus.write_beamcon0(BEAMCON0_VARHSYEN | BEAMCON0_HSYTRUE);
        let pins = agnus.sync_pin_levels(35, 0);
        assert!(pins.hsync, "HSYTRUE=1: active-high, inside window → true");
    }

    #[test]
    fn sync_polarity_vsytrue_inverts_output() {
        let mut agnus = AgnusEcs::new();
        agnus.write_vsstrt(100);
        agnus.write_vsstop(110);
        agnus.write_beamcon0(BEAMCON0_VARVSYEN | BEAMCON0_VSYTRUE);

        let pins = agnus.sync_pin_levels(0, 105);
        assert!(pins.vsync, "VSYTRUE=1, inside window → true");

        let pins = agnus.sync_pin_levels(0, 50);
        assert!(!pins.vsync, "VSYTRUE=1, outside window → false");
    }

    #[test]
    fn blanken_gates_composite_blank_output() {
        let mut agnus = AgnusEcs::new();
        agnus.write_hbstrt(10);
        agnus.write_hbstop(20);
        agnus.write_vbstrt(50);
        agnus.write_vbstop(60);

        // HARDDIS + VARVBEN enable the blank windows, but BLANKEN is clear.
        agnus.write_beamcon0(BEAMCON0_HARDDIS | BEAMCON0_VARVBEN);
        let pins = agnus.sync_pin_levels(15, 55);
        assert!(!pins.blank, "BLANKEN=0: blank output should be gated off");

        // Now enable BLANKEN.
        agnus.write_beamcon0(BEAMCON0_HARDDIS | BEAMCON0_VARVBEN | BEAMCON0_BLANKEN);
        let pins = agnus.sync_pin_levels(15, 55);
        assert!(
            pins.blank,
            "BLANKEN=1: blank output should follow composite blank"
        );

        // Outside blank windows.
        let pins = agnus.sync_pin_levels(5, 30);
        assert!(!pins.blank, "outside blank windows → blank=false");
    }

    #[test]
    fn cscben_gates_composite_sync_output() {
        let mut agnus = AgnusEcs::new();
        agnus.write_hsstrt(30);
        agnus.write_hsstop(40);
        agnus.write_vsstrt(100);
        agnus.write_vsstop(110);

        // Enable sync windows with CSYTRUE but NOT CSCBEN.
        agnus.write_beamcon0(BEAMCON0_VARHSYEN | BEAMCON0_VARVSYEN | BEAMCON0_CSYTRUE);
        let pins = agnus.sync_pin_levels(35, 50);
        assert!(
            !pins.csync,
            "CSCBEN=0: composite sync output should be gated off"
        );

        // Enable CSCBEN.
        agnus.write_beamcon0(
            BEAMCON0_VARHSYEN | BEAMCON0_VARVSYEN | BEAMCON0_CSYTRUE | BEAMCON0_CSCBEN,
        );
        // Inside hsync (35) but outside vsync (50): csync = hsync XOR vsync = true XOR false = true.
        let pins = agnus.sync_pin_levels(35, 50);
        assert!(pins.csync, "CSCBEN=1, CSYTRUE=1, hsync XOR vsync → true");
    }
}
