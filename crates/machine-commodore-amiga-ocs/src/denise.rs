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
        let in_ddf =
            ddf_stop > ddf_start && (ddf_start..ddf_stop).contains(&hpos);

        // Keep the archive's BPLCON0 copy in lockstep with Agnus's —
        // Agnus owns the primary storage (it consumes BPU for the DMA
        // scheduler); Denise reads HIRES/HOMOD/DBLPF/LACE from it.
        self.ocs.bplcon0 = agnus.bplcon0;

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
        }

        // ── Per-tick: output one lores pixel ────────────────────
        if in_visible_line && in_ddf {
            let local_y = u32::from(vpos.saturating_sub(vstart)) * 2;
            // Lores-pixel position within the DDF window:
            //   each CCK holds 2 lores pixels; `phase` selects which.
            let local_x = u32::from(hpos - ddf_start) * 2 + u32::from(phase);

            let dbg = self.ocs.output_pixel_with_beam(
                local_x, local_y, local_x, local_y,
            );
            if dbg.called {
                let rgb12 = self.ocs.resolve_color_rgb12(dbg.final_color_idx);
                let pixel = rgb12_to_argb(rgb12);
                for dy in [local_y, local_y + 1] {
                    for dx in 0..2u32 {
                        let x = local_x * 2 + dx;
                        let idx = (dy * FB_WIDTH + x) as usize;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = pixel;
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
