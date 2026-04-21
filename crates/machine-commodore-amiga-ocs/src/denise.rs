//! Denise — display chip wiring for the OCS machine.
//!
//! Register + pixel-pipeline state lives in the upstream
//! `commodore_denise_ocs::DeniseOcs`. This module is the narrow wiring
//! layer that the machine uses to:
//!
//!   - dispatch custom-register writes into `DeniseOcs::write_word`
//!     (BPLCON0/1/2, CLXCON, BPL*DAT, SPR*, COLOR00..COLOR31),
//!   - drive the per-CCK bitplane DMA fetch + shift-load cycle that
//!     the archive leaves to the caller,
//!   - copy the archive's per-pixel output into the machine's own
//!     ARGB framebuffer for display.
//!
//! DDF / DIW windowing helpers and the bitplane-DMA slot schedule
//! (`dma_claim`, `lores_fetch_plane`) stay in this module because
//! Agnus (not Denise) owns those registers on real silicon — the
//! helpers only need the register values, not Denise state.
//!
//! HIRES / HAM / EHB / DPF / sprites / collisions all flow through
//! the archive's `output_pixel_with_beam` unchanged — wholesale
//! delegation per wiki/amiga/denise-ocs-porting-gap-list.md Phase 2b.

use crate::memory::Memory;
use commodore_agnus_ocs::PAL_CCKS_PER_LINE;
use commodore_denise_ocs::DeniseOcs;

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

/// Lores 8-CCK fetch block schedule:
///   slot 0: BPL4 (BPU >= 4)
///   slot 1: BPL6 (BPU >= 6) — HAM/EHB only
///   slot 2: BPL2 (BPU >= 2)
///   slot 3: BPL5 (BPU >= 5)
///   slot 4: BPL3 (BPU >= 3)
///   slot 5: (idle)
///   slot 6: BPL1 (BPU >= 1)
///   slot 7: (idle)
#[must_use]
pub fn lores_fetch_plane(slot_in_block: u16, bpu: u8) -> Option<usize> {
    match slot_in_block {
        0 if bpu >= 4 => Some(3),
        1 if bpu >= 6 => Some(5),
        2 if bpu >= 2 => Some(1),
        3 if bpu >= 5 => Some(4),
        4 if bpu >= 3 => Some(2),
        6 if bpu >= 1 => Some(0),
        _ => None,
    }
}

/// DMA arbitration claim for a given CCK. Currently only bitplane
/// DMA is modelled — refresh / audio / sprites / disk land in later
/// milestones with their own features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaClaim {
    /// CCK is free — copper, blitter, or CPU can use the chip bus.
    Free,
    /// CCK is claimed by bitplane DMA for plane index 0..=5.
    Bitplane(u8),
}

impl DmaClaim {
    #[must_use]
    pub const fn is_free(self) -> bool {
        matches!(self, DmaClaim::Free)
    }
}

/// Compute the DMA claim at the given horizontal beam position for
/// a given chipset state. Considers bitplane DMA only (M12 minimum).
///
/// Per HRM Chapter 2: "The Copper is a two-cycle processor that
/// requests the bus only during odd-numbered memory cycles. This
/// prevents collision with audio, disk, refresh, sprites, and most
/// low resolution display DMA access, all of which use only the
/// even-numbered memory cycles." — so bitplane DMA predominantly
/// claims even CCKs, but BPL5 / BPL6 claim odd CCKs, which is
/// exactly where copper competes.
#[must_use]
pub fn dma_claim(
    hpos: u16,
    dmacon: u16,
    bplcon0: u16,
    ddfstrt: u16,
    ddfstop: u16,
) -> DmaClaim {
    // Bitplane DMA requires DMACON.DMAEN + DMACON.BPLEN (bits 9 + 8).
    if dmacon & 0x0300 != 0x0300 {
        return DmaClaim::Free;
    }
    let (ddf_start, ddf_stop) = ddf_window(ddfstrt, ddfstop);
    if !(ddf_stop > ddf_start && (ddf_start..ddf_stop).contains(&hpos)) {
        return DmaClaim::Free;
    }
    let bpu = ((bplcon0 >> 12) & 0x07) as u8;
    let slot_in_block = (hpos - ddf_start) % 8;
    match lores_fetch_plane(slot_in_block, bpu) {
        Some(p) => DmaClaim::Bitplane(p as u8),
        None => DmaClaim::Free,
    }
}

pub struct Denise {
    /// Archive pixel pipeline — owns BPLCON1/2, COLOR palette, sprite
    /// registers, shift registers, HAM prev-RGB, collision state.
    pub ocs: DeniseOcs,
    /// ARGB8888 framebuffer (FB_WIDTH × FB_HEIGHT pixels) for the
    /// frontend. We resolve the archive's `final_color_idx` through
    /// its palette for each pixel and fill a 2×2 block here (pixel-
    /// doubling + line-doubling for square-pixel 4:3 output).
    pub framebuffer: Vec<u32>,
    /// Bytes fetched on the current line (used to decide when to
    /// apply the end-of-line modulo).
    bytes_this_line: u32,
    /// vpos of the most recent `begin_beam_line()` call — guards
    /// against multiple resets per line.
    last_begin_line: Option<u16>,
    /// vpos of the most recent sprite-DMA pass — guards against
    /// re-fetching the same sprite pair if tick is called twice on
    /// the same line for any reason.
    last_sprite_dma_line: Option<u16>,
    /// Per-sprite display-mode flag. When `true` the next line-start
    /// DMA pair fetches DATA+DATB (displaying); when `false` it
    /// fetches POS+CTL (waiting for vstart). Real Agnus transitions
    /// between these states based on comparing vpos against the
    /// latched VSTART/VSTOP.
    spr_displaying: [bool; 8],
    /// Latched sprite vertical window from the most recent POS+CTL
    /// fetch. `(vstart, vstop)`.
    spr_vwindow: [(u16, u16); 8],
}

impl Default for Denise {
    fn default() -> Self {
        Self::new()
    }
}

impl Denise {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ocs: DeniseOcs::new(),
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            bytes_this_line: 0,
            last_begin_line: None,
            last_sprite_dma_line: None,
            spr_displaying: [false; 8],
            spr_vwindow: [(0, 0); 8],
        }
    }

    /// CPU / copper write to a Denise-owned custom register.
    /// Thin forwarder into `DeniseOcs::write_word`.
    pub fn write_word(&mut self, offset: u16, val: u16) {
        self.ocs.write_word(offset, val);
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

    /// Read one COLOR palette entry. Palette storage lives on
    /// `self.ocs` now; this is a convenience for the AmigaOcs
    /// `color(idx)` accessor which many tests rely on.
    #[must_use]
    pub fn color(&self, idx: usize) -> u16 {
        if idx < 32 {
            self.ocs.palette[idx]
        } else {
            0
        }
    }

    /// Tick one master/4 period (= 1 lores pixel, = half a CCK).
    /// `phase` selects which half of the CCK this tick belongs to:
    ///   - `0`: first lores pixel of the CCK. CCK-boundary events
    ///     (fetch, end-of-line modulo, begin-line reset) fire here.
    ///   - `1`: second lores pixel of the CCK.
    ///
    /// Every tick advances the archive's shift register by the mode-
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
        let (vstart, vstop) =
            diw_vertical_window(agnus.diwstrt, agnus.diwstop);
        let (ddf_start, ddf_stop) =
            ddf_window(agnus.ddfstrt, agnus.ddfstop);

        let in_visible_line = (vstart..vstop).contains(&vpos);
        let bpl_dma_on = dmacon & 0x0300 == 0x0300;
        let bpu = agnus.num_bitplanes();
        // The OCS fetch sequencer completes the 8-CCK block
        // containing DDFSTOP *and one more* (per WinUAE's DDF state
        // machine). For DDFSTRT=$38 DDFSTOP=$D0 lores that's 20 full
        // blocks (spans $38-$D7), not 19 — matching Agnus's
        // `current_slot` arbitrator which schedules the same 20
        // blocks as `Bitplane`. Without the extra block Denise
        // fetches one word too few per line, advances the pointer
        // by N-1 words instead of N, and the following scanline
        // reads data drawn one word earlier — producing the
        // per-scanline horizontal shear on KS 1.3's insert-disk
        // graphic.
        let ddf_fetch_stop = ddf_stop.saturating_add(8);
        let in_ddf =
            ddf_fetch_stop > ddf_start
                && (ddf_start..ddf_fetch_stop).contains(&hpos);

        // Keep the archive's BPLCON0 copy in lockstep with Agnus's —
        // Agnus owns the primary storage (it consumes BPU for the DMA
        // scheduler); Denise reads HIRES/HOMOD/DBLPF/LACE from it.
        self.ocs.bplcon0 = agnus.bplcon0;
        // Mirror interlace state: Agnus toggles `lof` each frame when
        // BPLCON0 LACE (bit 2) is set; Denise consumes both for
        // per-field row interleaving.
        let lace = (agnus.bplcon0 & 0x0004) != 0;
        self.ocs.interlace_active = lace;
        self.ocs.lof = agnus.lof;

        // ── CCK-boundary events (phase 0 only) ──────────────────
        if phase == 0 {
            // Bitplane fetch — one slot per CCK at the scheduled hpos
            // position within the 8-CCK block. The fetch writes into
            // the archive's `bpl_data` latch; writing BPL1DAT (plane
            // 0) queues the parallel shift-load that BPLCON1 will
            // commit when its comparator matches.
            if in_visible_line && bpl_dma_on && in_ddf {
                let slot_in_block = (hpos - ddf_start) % 8;
                if let Some(plane) = lores_fetch_plane(slot_in_block, bpu) {
                    let addr = agnus.bpl_pt[plane];
                    let word = memory.read_chip_ram_word(addr);
                    self.ocs.load_bitplane(plane, word);
                    if plane == 0 {
                        self.ocs.queue_shift_load_from_bpl1dat();
                    }
                    agnus.bpl_pt[plane] =
                        agnus.bpl_pt[plane].wrapping_add(2);
                    self.bytes_this_line += 2;
                }
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
                    agnus.bpl_pt[p] =
                        agnus.bpl_pt[p].wrapping_add(modulo as u32);
                }
                self.bytes_this_line = 0;
            }

            // Line-start reset — clears the archive's BPLCON1 carry
            // and the HAM prev-RGB. Fire once per visible line.
            if in_visible_line && self.last_begin_line != Some(vpos) {
                self.ocs.begin_beam_line();
                self.last_begin_line = Some(vpos);
            } else if !in_visible_line {
                self.last_begin_line = None;
            }

            // Sprite DMA — one 2-word pair per sprite per line, done
            // synchronously at line start when DMACON.SPREN (bit 5) +
            // DMAEN (bit 9) are both set. Real hardware distributes
            // the 16 word-fetches across hpos $0B..$1A; we fetch them
            // all at once on hpos==0 since the live Denise pipeline
            // only needs the registers populated before the display
            // comparator fires on this line.
            //
            // Per sprite:
            //   - If displaying and vpos has advanced past vstop:
            //     switch back to "waiting" (will fetch POS+CTL below).
            //   - Fetch a 2-word pair from agnus.spr_pt[sprite].
            //     Waiting mode -> POS+CTL (decode vstart/vstop).
            //     Displaying mode -> DATA+DATB.
            //   - Advance the pointer by 4 regardless.
            //   - After POS+CTL, if vstart matches current vpos,
            //     promote to displaying for the next line.
            let spr_dma_on = dmacon & 0x0220 == 0x0220;
            if hpos == 0 && spr_dma_on && self.last_sprite_dma_line != Some(vpos) {
                for sprite in 0..8 {
                    let (vstart, vstop) = self.spr_vwindow[sprite];
                    if self.spr_displaying[sprite] && vpos >= vstop {
                        self.spr_displaying[sprite] = false;
                    }
                    let addr = agnus.spr_pt[sprite];
                    let w0 = memory.read_chip_ram_word(addr);
                    let w1 = memory.read_chip_ram_word(addr.wrapping_add(2));
                    agnus.spr_pt[sprite] = agnus.spr_pt[sprite].wrapping_add(4);
                    if self.spr_displaying[sprite] {
                        self.ocs.write_sprite_data(sprite, w0);
                        self.ocs.write_sprite_datb(sprite, w1);
                    } else {
                        self.ocs.write_sprite_pos(sprite, w0);
                        self.ocs.write_sprite_ctl(sprite, w1);
                        let new_vstart =
                            (((w1 >> 2) & 1) << 8) | ((w0 >> 8) & 0xFF);
                        let new_vstop =
                            (((w1 >> 1) & 1) << 8) | ((w1 >> 8) & 0xFF);
                        self.spr_vwindow[sprite] = (new_vstart, new_vstop);
                        // Arm if we've reached vstart. (vstart == 0
                        // with vstop == 0 means the sprite is off —
                        // leave it waiting.)
                        if new_vstart == vpos.wrapping_add(1)
                            && new_vstop > new_vstart
                        {
                            self.spr_displaying[sprite] = true;
                        }
                        let _ = vstart;
                    }
                }
                self.last_sprite_dma_line = Some(vpos);
            } else if hpos != 0 {
                self.last_sprite_dma_line = None;
            }
        }

        // ── Per-tick: output one lores pixel ────────────────────
        //
        // Every beam cycle in the visible viewport paints SOMETHING
        // into the framebuffer. Inside DIW+DDF with live bitplane
        // data we use the bitplane-decoded colour; outside that
        // region (border, pre-warmup slots, lines above/below the
        // DIW window) we paint COLOR00 — the background colour
        // Agnus/Denise outputs during blanking-free beam slots. KS
        // 1.3's insert-disk screen sets COLOR00=$0FFF (white), so
        // the borders should render white, not the framebuffer's
        // init-time black. The previous piecewise-port-state gated
        // all framebuffer writes on `in_visible_line && in_ddf`,
        // leaving the border at the init colour.
        //
        // Framebuffer layout matches the PAL "Standard" viewport
        // the archive used (`ViewportPreset::Standard`,
        // h_start_cck=$2C, v_start_line=$19). Aligning our per-
        // pixel write origin to that viewport puts the display
        // content at the same framebuffer coordinates FS-UAE
        // captures — which is what the golden PNGs were sampled
        // from.
        const VIEWPORT_H_START_CCK: u16 = 0x2C;
        const VIEWPORT_V_START_LINE: u16 = 0x19;
        const VIEWPORT_H_END_CCK: u16 = 0xEC;
        const VIEWPORT_V_END_LINE: u16 = 0x139;
        let in_viewport_h =
            hpos >= VIEWPORT_H_START_CCK && hpos < VIEWPORT_H_END_CCK;
        let in_viewport_v =
            vpos >= VIEWPORT_V_START_LINE && vpos < VIEWPORT_V_END_LINE;
        if in_viewport_h && in_viewport_v {
            let fb_y = u32::from(vpos - VIEWPORT_V_START_LINE) * 2;
            let fb_x_lores = u32::from(hpos - VIEWPORT_H_START_CCK) * 2
                + u32::from(phase);

            // Pipeline coordinates (DDF-relative) stay the same —
            // they're what Denise uses internally for sprite and
            // shift-register comparators.
            let pipeline_x = if hpos >= ddf_start {
                u32::from(hpos - ddf_start) * 2 + u32::from(phase)
            } else {
                0
            };
            let pipeline_y = u32::from(vpos.saturating_sub(vstart)) * 2;

            let local_x = fb_x_lores;
            let local_y = fb_y;

            // Only consult the bitplane pipeline inside DIW+DDF;
            // elsewhere the beam shows COLOR00 background.
            let color_idx = if in_visible_line && in_ddf {
                let dbg = self.ocs.output_pixel_with_beam(
                    pipeline_x, pipeline_y, pipeline_x, pipeline_y,
                );
                if dbg.called { Some(dbg.final_color_idx) } else { None }
            } else {
                None
            };
            let final_idx = color_idx.unwrap_or(0);
            {
                let rgb12 = self.ocs.resolve_color_rgb12(final_idx);
                let pixel = rgb12_to_argb(rgb12);
                // LACE: paint one row per field (long frame = even,
                // short frame = odd). Non-interlaced: paint both rows
                // of the doubled pair. Both cases pixel-double across
                // X for square-pixel 4:3.
                let rows: &[u32] = if lace {
                    if agnus.lof {
                        &[local_y]
                    } else {
                        // Short-field rows land immediately above the
                        // long-field row to match real-beam interleave.
                        &[local_y + 1]
                    }
                } else {
                    &[local_y, local_y + 1]
                };
                for &dy in rows {
                    for dx in 0..2u32 {
                        let x = local_x * 2 + dx;
                        let idx = (dy * FB_WIDTH + x) as usize;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = pixel;
                        }
                    }
                }
            }

            // Tail fill — extend COLOR00 to the right border.
            //
            // The Standard viewport is 192 CCKs wide ($2C..$EC) which
            // maps to all 768 FB columns. But PAL lines are only 227
            // CCKs long, so the beam's `hpos` never reaches the
            // viewport's right edge — cols [732..768) would otherwise
            // stay at the framebuffer's init colour for the entire
            // frame. Real CRTs still show the Amiga's current COLOR00
            // across the full visible scanline, so at the last
            // painted CCK of the line we spray COLOR00 across the
            // remaining FB columns. `resolve_color_rgb12(0)` matches
            // the value we already write for pre-DDF / post-DIW
            // pixels, keeping the border a single uniform colour.
            if phase == 1 && hpos == PAL_CCKS_PER_LINE - 1 {
                let tail_start = u32::from(hpos - VIEWPORT_H_START_CCK + 1)
                    * 4;
                if tail_start < FB_WIDTH {
                    let rgb12_bg = self.ocs.resolve_color_rgb12(0);
                    let bg_pixel = rgb12_to_argb(rgb12_bg);
                    let rows: &[u32] = if lace {
                        if agnus.lof { &[local_y] } else { &[local_y + 1] }
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

fn rgb12_to_argb(c12: u16) -> u32 {
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
    fn lores_fetch_schedule() {
        // BPU=1: only slot 6 (BPL1).
        assert_eq!(lores_fetch_plane(6, 1), Some(0));
        for s in [0, 1, 2, 3, 4, 5, 7] {
            assert_eq!(lores_fetch_plane(s, 1), None);
        }
        // BPU=3: slots 2 (BPL2), 4 (BPL3), 6 (BPL1).
        assert_eq!(lores_fetch_plane(2, 3), Some(1));
        assert_eq!(lores_fetch_plane(4, 3), Some(2));
        assert_eq!(lores_fetch_plane(6, 3), Some(0));
        assert_eq!(lores_fetch_plane(0, 3), None);
    }

    #[test]
    fn diw_vertical_window_pal_nominal() {
        // PAL nominal values per HRM Table 3-9: DIWSTRT $2C81,
        // DIWSTOP $2CC1. VSTOP byte $2C < $80 so MSB is 1 →
        // effective VSTOP = $12C (300).
        let (vstart, vstop) = diw_vertical_window(0x2C81, 0x2CC1);
        assert_eq!(vstart, 0x2C);
        assert_eq!(vstop, 0x12C);
    }

    #[test]
    fn diw_vertical_window_ntsc_nominal() {
        // NTSC DIWSTOP $F4C1. VSTOP byte $F4 >= $80 so MSB is 0 →
        // effective VSTOP = $F4 (244).
        let (vstart, vstop) = diw_vertical_window(0x2C81, 0xF4C1);
        assert_eq!(vstart, 0x2C);
        assert_eq!(vstop, 0xF4);
    }

    #[test]
    fn ddf_window_masks_to_fetch_alignment() {
        // Low 2 bits forced to zero (4-CCK alignment).
        assert_eq!(ddf_window(0x0038, 0x00D0), (0x38, 0xD0));
        assert_eq!(ddf_window(0x003B, 0x00D2), (0x38, 0xD0));
        assert_eq!(ddf_window(0x003C, 0x00D3), (0x3C, 0xD0));
    }

    // DMA-claim coverage. Values match the default lores boot
    // configuration: DMACON = $8300 (DMAEN + BPLEN), BPLCON0 with
    // BPU in bits 14-12, DDFSTRT = $38, DDFSTOP = $D0.
    const DMACON_BPL: u16 = 0x0300;
    const DDFSTRT: u16 = 0x0038;
    const DDFSTOP: u16 = 0x00D0;
    const fn bplcon0(bpu: u16) -> u16 { bpu << 12 }

    #[test]
    fn dma_claim_free_when_bpl_dma_disabled() {
        // DMACON has no bits set → bitplane DMA off.
        for hpos in 0..227 {
            assert_eq!(
                dma_claim(hpos, 0x0000, bplcon0(3), DDFSTRT, DDFSTOP),
                DmaClaim::Free,
            );
        }
    }

    #[test]
    fn dma_claim_free_outside_ddf_window() {
        // Before DDFSTRT and after DDFSTOP → always free.
        assert_eq!(
            dma_claim(0x30, DMACON_BPL, bplcon0(6), DDFSTRT, DDFSTOP),
            DmaClaim::Free,
        );
        assert_eq!(
            dma_claim(0xE0, DMACON_BPL, bplcon0(6), DDFSTRT, DDFSTOP),
            DmaClaim::Free,
        );
    }

    #[test]
    fn dma_claim_bpu1_claims_only_slot_6() {
        // Only BPL1 fetches, at slot 6 of each 8-CCK block (so even
        // CCKs only, starting at DDFSTRT + 6).
        assert_eq!(
            dma_claim(0x38 + 6, DMACON_BPL, bplcon0(1), DDFSTRT, DDFSTOP),
            DmaClaim::Bitplane(0),
        );
        // All other slots within DDF are free for BPU=1.
        for s in [0u16, 1, 2, 3, 4, 5, 7] {
            assert_eq!(
                dma_claim(0x38 + s, DMACON_BPL, bplcon0(1), DDFSTRT, DDFSTOP),
                DmaClaim::Free,
                "BPU=1, slot {s} should be free",
            );
        }
    }

    #[test]
    fn dma_claim_bpu6_claims_bpl5_and_bpl6_on_odd_slots() {
        // With BPU=6, BPL5 fetches at slot 3 (odd CCK) and BPL6 at
        // slot 1 (odd CCK). This is what blocks the copper per HRM.
        assert_eq!(
            dma_claim(0x38 + 1, DMACON_BPL, bplcon0(6), DDFSTRT, DDFSTOP),
            DmaClaim::Bitplane(5),
        );
        assert_eq!(
            dma_claim(0x38 + 3, DMACON_BPL, bplcon0(6), DDFSTRT, DDFSTOP),
            DmaClaim::Bitplane(4),
        );
        // Slots 5 and 7 (odd) remain free — these are the odd slots
        // that stay available for copper even at BPU=6.
        assert_eq!(
            dma_claim(0x38 + 5, DMACON_BPL, bplcon0(6), DDFSTRT, DDFSTOP),
            DmaClaim::Free,
        );
        assert_eq!(
            dma_claim(0x38 + 7, DMACON_BPL, bplcon0(6), DDFSTRT, DDFSTOP),
            DmaClaim::Free,
        );
    }
}
