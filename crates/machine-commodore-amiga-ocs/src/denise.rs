//! Denise — display chip, per-CCK cycle-accurate.
//!
//! Each CCK while the beam is in the visible region:
//!   1. If the current CCK is a bitplane-fetch slot (per the lores
//!      DMA schedule) and bitplane DMA is enabled, fetch a 16-bit
//!      word from the appropriate `BPL[n]PT`, advance the pointer
//!      by 2, and latch into `bpl_data[n]`.
//!   2. At the slot boundary (end of the 8-CCK block), load
//!      `bpl_shift[n] ← bpl_data[n]` for each active plane.
//!   3. Output 2 lores pixels: combine the MSB of each plane's
//!      shift register into a color index, look up `chipset.color`,
//!      write to the framebuffer (with horizontal pixel-doubling
//!      and vertical line-doubling for square-pixel 4:3 output).
//!      Then left-shift each plane's shift register by 1.
//!
//! End of line: advance `BPL[n]PT` by the line modulo.
//!
//! This is the minimum cycle-accurate form. Hires + dual-playfield
//! + HAM + EHB are NOT supported — those land when a test demands.

use crate::chipset::Chipset;
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
    /// ARGB8888 framebuffer (FB_WIDTH × FB_HEIGHT pixels).
    pub framebuffer: Vec<u32>,
    /// Per-bitplane shift registers — current 16 bits being clocked out.
    pub bpl_shift: [u16; 6],
    /// Per-bitplane data registers — latched fetched word, copied
    /// into shift register at the next slot boundary.
    pub bpl_data: [u16; 6],
    /// Bytes fetched on the current line per plane (used to compute
    /// modulo adjustment at end of line).
    bytes_this_line: u32,
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
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            bpl_shift: [0; 6],
            bpl_data: [0; 6],
            bytes_this_line: 0,
        }
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

    /// Tick one master/4 period (= 1 lores pixel, = half a CCK).
    /// `phase` selects which half of the CCK this tick belongs to:
    ///   - `0`: first lores pixel of the CCK. This is the CCK boundary
    ///     where bitplane fetch, shift-register reload, and end-of-line
    ///     modulo events land.
    ///   - `1`: second lores pixel of the CCK. Pixel output only.
    ///
    /// Every tick advances the shift register by 1 bit and outputs 1
    /// lores pixel to the framebuffer (when the beam is inside the
    /// display window). Called from `AmigaOcs::tick` after the beam
    /// advance on phase 0.
    pub fn tick(
        &mut self,
        phase: u8,
        vpos: u16,
        hpos: u16,
        chipset: &mut Chipset,
        memory: &Memory,
    ) {
        let (vstart, vstop) =
            diw_vertical_window(chipset.diwstrt, chipset.diwstop);
        let (ddf_start, ddf_stop) =
            ddf_window(chipset.ddfstrt, chipset.ddfstop);

        let in_visible_line = (vstart..vstop).contains(&vpos);
        let bpl_dma_on = chipset.dmacon & 0x0300 == 0x0300;
        let bpu = chipset.num_bitplanes();
        let in_ddf =
            ddf_stop > ddf_start && (ddf_start..ddf_stop).contains(&hpos);

        // ── CCK-boundary events (phase 0 only) ──────────────────
        if phase == 0 {
            // 1. Bitplane fetch — one slot per CCK at the scheduled
            //    hpos position within the 8-CCK block.
            if in_visible_line && bpl_dma_on && in_ddf {
                let slot_in_block = (hpos - ddf_start) % 8;
                if let Some(plane) = lores_fetch_plane(slot_in_block, bpu) {
                    let addr = chipset.bpl_pt[plane];
                    // DMA word read — drives the chip bus, so updates
                    // the floating-bus residue.
                    self.bpl_data[plane] = memory.read_chip_ram_word(addr);
                    chipset.bpl_pt[plane] =
                        chipset.bpl_pt[plane].wrapping_add(2);
                    self.bytes_this_line += 2;
                }
            }

            // 2. Reload shift registers at slot 7 of each 8-CCK block.
            if in_visible_line && in_ddf {
                let slot_in_block = (hpos - ddf_start) % 8;
                if slot_in_block == 7 {
                    for p in 0..bpu as usize {
                        self.bpl_shift[p] = self.bpl_data[p];
                    }
                }
            }

            // 3. End-of-line modulo — applied the moment hpos wraps
            //    to zero (i.e. at the start of the next line).
            if hpos == 0 && self.bytes_this_line > 0 && bpl_dma_on {
                for p in 0..bpu as usize {
                    let modulo = if p & 1 == 0 {
                        i32::from(chipset.bpl1mod)
                    } else {
                        i32::from(chipset.bpl2mod)
                    };
                    chipset.bpl_pt[p] =
                        chipset.bpl_pt[p].wrapping_add(modulo as u32);
                }
                self.bytes_this_line = 0;
            }
        }

        // ── Per-tick: output 1 lores pixel ──────────────────────
        if in_visible_line && in_ddf {
            let local_y = u32::from(vpos.saturating_sub(vstart)) * 2;
            // Lores-pixel position within the DDF window:
            //   each CCK holds 2 lores pixels; `phase` selects which.
            let local_x = u32::from(hpos - ddf_start) * 2 + u32::from(phase);

            let mut index = 0u8;
            for p in 0..bpu as usize {
                if (self.bpl_shift[p] >> 15) & 1 != 0 {
                    index |= 1 << p;
                }
                self.bpl_shift[p] <<= 1;
            }
            // Palette has 32 entries. At BPU > 5 the 6-bit index
            // selects HAM or EHB semantics we don't yet model — fall
            // back to low-5-bit lookup so indexing stays in range.
            let palette_idx = (index & 0x1F) as usize;
            let colour = chipset.color[palette_idx] & 0x0FFF;
            let pixel = rgb12_to_argb(colour);
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
