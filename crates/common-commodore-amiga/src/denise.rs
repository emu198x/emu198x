//! Denise — board-level wrapper, generic over the Denise chip variant.
//!
//! Register + pixel-pipeline state lives in the concrete chip
//! (`commodore_denise_ocs::DeniseOcs` for OCS, `commodore_denise_ecs::DeniseEcs`
//! for ECS, future variants for AGA Lisa / CD32). This module is the
//! narrow wiring layer that the machine uses to:
//!
//!   - dispatch custom-register writes into the chip's `write_word`
//!     (BPLCON0/1/2, CLXCON, BPL*DAT, SPR*, COLOR00..COLOR31),
//!   - drive the per-CCK bitplane DMA fetch + shift-load cycle that
//!     the chip leaves to the caller,
//!   - copy the chip's per-pixel output into the board's own ARGB
//!     framebuffer for display.
//!
//! DDF / DIW windowing helpers (`ddf_window`, `diw_vertical_window`)
//! live here because Agnus (not Denise) owns those registers on real
//! silicon — the helpers only need the register values, not Denise
//! state. The per-CCK DMA slot schedule itself now lives in one place:
//! Agnus's `current_slot` / `cck_bus_plan` (#30).
//!
//! HIRES / HAM / EHB / DPF / sprites / collisions all flow through
//! the chip's `output_pixel_with_beam_and_playfield_gate` unchanged.

use crate::denise_chip::DeniseChip;
use crate::memory::Memory;

/// Display dimensions for PAL Standard (line-doubled, lores → 4:3).
pub const FB_WIDTH: u32 = 768;
pub const FB_HEIGHT: u32 = 576;

/// Decode a DIWSTRT/DIWSTOP register pair into the effective vertical
/// display window `[vstart, vstop)` in absolute line numbers.
///
/// Per HRM 3rd ed., Table 3-9 and the surrounding "Setting the
/// Display Window" prose:
///
/// - VSTART is the low 8 bits of DIWSTRT's high byte; bit 8 is
///   implicitly 0 (VSTART is in the upper half of the frame).
/// - VSTOP is the low 8 bits of DIWSTOP's high byte; bit 8 is
///   **the complement of bit 7** ("forcing the MSB of the stop
///   position to be the complement of the next MSB"). So a byte
///   value >=$80 means VSTOP is in the upper half (bit 8 = 0),
///   and <$80 means VSTOP is in the lower half (bit 8 = 1,
///   i.e. add $100).
#[must_use]
pub fn diw_vertical_window(diwstrt: u16, diwstop: u16) -> (u16, u16) {
    let vstart = (diwstrt >> 8) & 0xFF;
    let vstop_byte = (diwstop >> 8) & 0xFF;
    let vstop = if vstop_byte & 0x80 != 0 {
        vstop_byte
    } else {
        vstop_byte | 0x100
    };
    (vstart, vstop)
}

/// Decode a DDFSTRT/DDFSTOP register pair into a CCK-aligned fetch
/// window `[ddf_start, ddf_stop)`. The low 2 bits of each register
/// are forced to zero per HRM — fetch block boundaries must align to
/// 4 CCKs for lores.
#[must_use]
pub fn ddf_window(ddfstrt: u16, ddfstop: u16) -> (u16, u16) {
    (ddfstrt & 0x00FC, ddfstop & 0x00FC)
}

/// Board-level Denise wrapper, generic over the concrete chip
/// variant via [`DeniseChip`]. Each per-chipset machine crate
/// instantiates this with its specific Denise type
/// (`Denise<DeniseOcs>`, `Denise<DeniseEcs>`, future `Denise<DeniseAga>`).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(serialize = "C: serde::Serialize"))]
#[serde(bound(deserialize = "C: serde::de::DeserializeOwned"))]
pub struct Denise<C: DeniseChip> {
    /// The concrete Denise chip — owns BPLCON1/2, COLOR palette,
    /// sprite registers, shift registers, HAM prev-RGB, collision
    /// state.
    pub ocs: C,
    /// ARGB8888 framebuffer (FB_WIDTH × FB_HEIGHT pixels) for the
    /// frontend. We resolve the chip's `final_color_idx` through
    /// its palette for each pixel and fill a 2×2 block here (pixel-
    /// doubling + line-doubling for square-pixel 4:3 output).
    pub framebuffer: Vec<u32>,
    /// Bytes fetched on the current line (used to decide when to
    /// apply the end-of-line modulo).
    bytes_this_line: u32,
    /// vpos of the most recent `begin_beam_line()` call — guards
    /// against multiple resets per line.
    last_begin_line: Option<u16>,
}

impl<C: DeniseChip> Default for Denise<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: DeniseChip> Denise<C> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ocs: C::new(),
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            bytes_this_line: 0,
            last_begin_line: None,
        }
    }

    /// CPU / copper write to a Denise-owned custom register. Thin
    /// forwarder into the chip's `write_word`.
    pub fn write_word(&mut self, offset: u16, val: u16) {
        self.ocs.write_word(offset, val);
    }

    /// DENISEID register read ($DFF07C). Each chip variant returns
    /// its own marker — KS uses the value to discriminate OCS / ECS
    /// / AGA at boot.
    #[must_use]
    pub fn deniseid(&self) -> u16 {
        self.ocs.deniseid()
    }

    /// CLXDAT register read ($DFF00E) — latched sprite/playfield
    /// collision bits, cleared on read. Forwarded to the concrete
    /// chip's collision latch.
    pub fn read_clxdat(&mut self) -> u16 {
        self.ocs.read_clxdat()
    }

    /// Non-destructive CLXDAT read for the debug / inspection bus
    /// (`&self`); does not clear the latch.
    #[must_use]
    pub fn peek_clxdat(&self) -> u16 {
        self.ocs.peek_clxdat()
    }

    /// Framebuffer dimensions (width, height).
    #[must_use]
    pub fn framebuffer_size(&self) -> (u32, u32) {
        (FB_WIDTH, FB_HEIGHT)
    }

    /// Read-only framebuffer access.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Read one COLOR palette entry from the chip.
    #[must_use]
    pub fn color(&self, idx: usize) -> u16 {
        if idx < 32 { self.ocs.palette()[idx] } else { 0 }
    }

    /// Tick one master/4 period (= 1 lores pixel, = half a CCK).
    /// `phase` selects which half of the CCK this tick belongs to:
    ///   - `0`: first lores pixel of the CCK. CCK-boundary events
    ///     (fetch, end-of-line modulo, begin-line reset) fire here.
    ///   - `1`: second lores pixel of the CCK.
    ///
    /// Every tick advances the chip's shift register by the mode-
    /// appropriate number of source pixels and writes one lores pixel
    /// (pixel-doubled) to the framebuffer when the beam is inside the
    /// display window.
    pub fn tick(
        &mut self,
        phase: u8,
        vpos: u16,
        hpos: u16,
        dmacon: u16,
        agnus: &mut commodore_agnus_ocs::Agnus,
        memory: &Memory,
    ) {
        let (vstart, vstop) = diw_vertical_window(agnus.diwstrt, agnus.diwstop);
        let (ddf_start, _) = ddf_window(agnus.ddfstrt, agnus.ddfstop);

        let in_visible_line = (vstart..vstop).contains(&vpos);
        let bpl_dma_on = dmacon & 0x0300 == 0x0300;
        let bpu = agnus.num_bitplanes();
        // The fetch sequencer completes the 8-CCK block containing
        // DDFSTOP *and one more* (per WinUAE's DDF state machine).
        // Keep the chip's BPLCON0 copy in lockstep with Agnus's —
        // Agnus owns the primary storage (it consumes BPU for the DMA
        // scheduler); Denise reads HIRES/HOMOD/DBLPF/LACE from it.
        self.ocs.set_bplcon0(agnus.bplcon0);
        // Mirror interlace state: Agnus toggles `lof` each frame when
        // BPLCON0 LACE (bit 2) is set; Denise consumes both for
        // per-field row interleaving.
        let lace = (agnus.bplcon0 & 0x0004) != 0;
        self.ocs.set_interlace_active(lace);
        self.ocs.set_lof(agnus.lof);

        // ── CCK-boundary events (phase 0 only) ──────────────────
        if phase == 0 {
            // Bitplane fetch — follow Agnus's live DMA grant.
            if in_visible_line
                && bpl_dma_on
                && let Some(plane_u8) = agnus.cck_bus_plan().bitplane_dma_fetch_plane
            {
                let plane = plane_u8 as usize;
                let width = u32::from(agnus.bpl_fetch_width());
                let addr = agnus.bpl_pt[plane];
                // First word feeds the normal shift-register load path.
                let word = memory.read_chip_ram_word(addr);
                self.ocs.load_bitplane(plane, word);
                if plane == 0 {
                    self.ocs.queue_shift_load_from_bpl1dat();
                }
                // AGA wide fetch (FMODE > 0): a single DMA slot transfers
                // 2 (32-bit) or 4 (64-bit) words. The extra words queue in
                // Denise's per-plane FIFO and reload the shift register as
                // it drains. Width 1 (OCS / ECS) skips this loop entirely.
                for w in 1..width {
                    let extra = memory.read_chip_ram_word(addr.wrapping_add(2 * w));
                    self.ocs.push_bpl_fifo(plane, extra);
                }
                let bytes = 2 * width;
                agnus.bpl_pt[plane] = agnus.bpl_pt[plane].wrapping_add(bytes);
                self.bytes_this_line += bytes;
            }

            // End-of-line modulo — applied the moment hpos wraps to
            // zero (i.e. at the start of the next line).
            if hpos == 0 && self.bytes_this_line > 0 && bpl_dma_on {
                for p in 0..bpu as usize {
                    let modulo = if p & 1 == 0 {
                        i32::from(agnus.bpl1mod)
                    } else {
                        i32::from(agnus.bpl2mod)
                    };
                    agnus.bpl_pt[p] = agnus.bpl_pt[p].wrapping_add(modulo as u32);
                }
                self.bytes_this_line = 0;
            }

            // Line-start reset — clears the chip's BPLCON1 carry and
            // the HAM prev-RGB. Fire once per visible line.
            if in_visible_line && self.last_begin_line != Some(vpos) {
                self.ocs.begin_beam_line();
                self.last_begin_line = Some(vpos);
            } else if !in_visible_line {
                self.last_begin_line = None;
            }

            // Sprite DMA is serviced per colour-clock slot by the machine
            // (Agnus owns the control/data state machine + the SPRxPT
            // pointers; see `Agnus::service_sprite_dma_cyc`). The earlier
            // per-line implementation here advanced the pointer every line
            // — including idle and vblank lines — which desynced VSTART/
            // VSTOP from the data stream after the first control fetch, so
            // DMA-driven sprites never displayed (gap #162). The control/
            // data words land in this chip via the same `write_sprite_*`
            // helpers, so the render path is unchanged.
        }

        // ── Per-tick: output one lores pixel ────────────────────
        const VIEWPORT_H_START_CCK: u16 = 0x2C;
        const VIEWPORT_V_START_LINE: u16 = 0x19;
        const VIEWPORT_H_END_CCK: u16 = 0xEC;
        const VIEWPORT_V_END_LINE: u16 = 0x139;
        let in_viewport_h = (VIEWPORT_H_START_CCK..VIEWPORT_H_END_CCK).contains(&hpos);
        let in_viewport_v = (VIEWPORT_V_START_LINE..VIEWPORT_V_END_LINE).contains(&vpos);
        if in_viewport_h && in_viewport_v {
            let fb_y = u32::from(vpos - VIEWPORT_V_START_LINE) * 2;
            let fb_x_lores = u32::from(hpos - VIEWPORT_H_START_CCK) * 2 + u32::from(phase);

            let pipeline_x = if hpos >= ddf_start {
                u32::from(hpos - ddf_start) * 2 + u32::from(phase)
            } else {
                0
            };
            let pipeline_y = u32::from(vpos.saturating_sub(vstart)) * 2;

            let local_x = fb_x_lores;
            let local_y = fb_y;

            // Apply the horizontal DIW gate. DIWSTRT/DIWSTOP comparator
            // blanks both playfields and sprites outside the window;
            // only COLOR00 is output.
            let beam_x_lores = u32::from(hpos) * 2 + u32::from(phase);
            let hstart = u32::from(agnus.diwstrt & 0x00FF);
            let hstop = 0x0100u32 | u32::from(agnus.diwstop & 0x00FF);
            let in_visible_h = beam_x_lores >= hstart && beam_x_lores < hstop;
            let playfield_gate = in_visible_line && in_visible_h;

            // The bitplane pipeline runs in scroll-relative coordinates
            // (`pipeline_x`/`pipeline_y`), but the sprite comparator needs
            // the *absolute* beam position: SPRxPOS/CTL decode to an
            // absolute raster line (VSTART/VSTOP) and lores HSTART. Feed
            // the sprite path `beam_x_lores` and the raw `vpos` so DMA-
            // driven sprites land where the copper positioned them. gap #162.
            let dbg = self.ocs.output_pixel_with_beam_sprite_coords(
                pipeline_x,
                pipeline_y,
                pipeline_x,
                pipeline_y,
                beam_x_lores,
                u32::from(vpos),
                playfield_gate,
            );
            let cols = if dbg.called {
                match dbg.source_pixels_per_fb_pixel.min(2) {
                    0 => [0, 0],
                    1 => [dbg.final_color_idx, dbg.final_color_idx],
                    _ => [dbg.quad_color_idx[0], dbg.quad_color_idx[1]],
                }
            } else {
                [0, 0]
            };
            {
                let rows: &[u32] = if lace {
                    if agnus.lof {
                        &[local_y]
                    } else {
                        &[local_y + 1]
                    }
                } else {
                    &[local_y, local_y + 1]
                };
                for &dy in rows {
                    for (dx, color_idx) in cols.iter().enumerate() {
                        // Resolve through the chip's colour path — 24-bit
                        // palette on AGA, 12-bit upscaled on OCS/ECS (#93).
                        let pixel = self.ocs.resolve_color_argb(*color_idx);
                        let x = local_x * 2 + dx as u32;
                        let idx = (dy * FB_WIDTH + x) as usize;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = pixel;
                        }
                    }
                }
            }

            // Tail fill — extend COLOR00 to the right border.
            if phase == 1 && hpos == agnus.current_line_ccks() - 1 {
                let tail_start = u32::from(hpos - VIEWPORT_H_START_CCK + 1) * 4;
                if tail_start < FB_WIDTH {
                    let bg_pixel = self.ocs.resolve_color_argb(0);
                    let rows: &[u32] = if lace {
                        if agnus.lof {
                            &[local_y]
                        } else {
                            &[local_y + 1]
                        }
                    } else {
                        &[local_y, local_y + 1]
                    };
                    for &dy in rows {
                        for x in tail_start..FB_WIDTH {
                            let idx = (dy * FB_WIDTH + x) as usize;
                            if idx < self.framebuffer.len() {
                                self.framebuffer[idx] = bg_pixel;
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn rgb12_to_argb(c12: u16) -> u32 {
    let r = ((c12 >> 8) & 0xF) as u32;
    let g = ((c12 >> 4) & 0xF) as u32;
    let b = (c12 & 0xF) as u32;
    let r8 = (r << 4) | r;
    let g8 = (g << 4) | g;
    let b8 = (b << 4) | b;
    0xFF00_0000 | (r8 << 16) | (g8 << 8) | b8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb12_conversion() {
        assert_eq!(rgb12_to_argb(0x0FFF), 0xFFFF_FFFF);
        assert_eq!(rgb12_to_argb(0x0000), 0xFF00_0000);
        assert_eq!(rgb12_to_argb(0x0F00), 0xFFFF_0000);
        assert_eq!(rgb12_to_argb(0x0444), 0xFF44_4444);
    }

    #[test]
    fn diw_vertical_window_pal_nominal() {
        let (vstart, vstop) = diw_vertical_window(0x2C81, 0x2CC1);
        assert_eq!(vstart, 0x2C);
        assert_eq!(vstop, 0x12C);
    }

    #[test]
    fn diw_vertical_window_ntsc_nominal() {
        let (vstart, vstop) = diw_vertical_window(0x2C81, 0xF4C1);
        assert_eq!(vstart, 0x2C);
        assert_eq!(vstop, 0xF4);
    }

    #[test]
    fn ddf_window_masks_to_fetch_alignment() {
        assert_eq!(ddf_window(0x0038, 0x00D0), (0x38, 0xD0));
        assert_eq!(ddf_window(0x003B, 0x00D2), (0x38, 0xD0));
        assert_eq!(ddf_window(0x003C, 0x00D3), (0x3C, 0xD0));
    }

    #[test]
    fn wide_fetch_advances_pointer_by_width_words_per_line() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        // Workbench 3.1 AGA: hires, 2 planes, FMODE=$0003 (64-bit). The
        // DMA scheduler grants 11 plane accesses per line; the fetch
        // loop turns each into 4 words, so each plane pointer must
        // advance by 11*4*2 = 88 bytes (44 words) per line, before the
        // end-of-line modulo. (Driven by Agnus FMODE, so the OCS chip
        // exercises the same fetch-loop path the AGA chip uses.)
        let mut agnus = Agnus::new();
        agnus.max_bitplanes = 8;
        agnus.dmacon = 0x0300; // DMAEN | BPLEN
        agnus.bplcon0 = 0xA302; // HIRES + 2 planes
        agnus.ddfstrt = 0x38;
        agnus.ddfstop = 0xD8;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.fmode = 0x0003;
        agnus.bpl_pt[0] = 0x2000;
        agnus.bpl_pt[1] = 0x3000;
        let (bpl0, bpl1) = (agnus.bpl_pt[0], agnus.bpl_pt[1]);

        let mem = Memory::new(vec![0u8; 0x4_0000]);
        let mut denise = Denise::<DeniseOcs>::new();

        // Sweep one whole line (vpos inside the 44..300 DIW window);
        // stop before wrapping to hpos 0 again so no modulo is applied.
        for h in 0u16..=0xE2 {
            agnus.hpos = h;
            denise.tick(0, 100, h, agnus.dmacon, &mut agnus, &mem);
        }

        assert_eq!(agnus.bpl_pt[0] - bpl0, 88, "BPL1 bytes/line (44 words)");
        assert_eq!(agnus.bpl_pt[1] - bpl1, 88, "BPL2 bytes/line (44 words)");
    }

    #[test]
    fn narrow_fetch_advances_pointer_by_one_word_per_access() {
        use commodore_agnus_ocs::Agnus;
        use commodore_denise_ocs::DeniseOcs;

        // OCS / ECS regression: FMODE=0 keeps 16-bit fetch. DDFSTRT=$40
        // DDFSTOP=$D0 hires = 38 accesses/plane → 38 words = 76 bytes.
        let mut agnus = Agnus::new();
        agnus.dmacon = 0x0300;
        agnus.bplcon0 = 0xA000; // HIRES + 2 planes
        agnus.ddfstrt = 0x40;
        agnus.ddfstop = 0xD0;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.bpl_pt[0] = 0x2000;
        agnus.bpl_pt[1] = 0x3000;
        let (bpl0, bpl1) = (agnus.bpl_pt[0], agnus.bpl_pt[1]);

        let mem = Memory::new(vec![0u8; 0x4_0000]);
        let mut denise = Denise::<DeniseOcs>::new();

        for h in 0u16..=0xE2 {
            agnus.hpos = h;
            denise.tick(0, 100, h, agnus.dmacon, &mut agnus, &mem);
        }

        assert_eq!(agnus.bpl_pt[0] - bpl0, 76, "BPL1 bytes/line (38 words)");
        assert_eq!(agnus.bpl_pt[1] - bpl1, 76, "BPL2 bytes/line (38 words)");
    }
}
