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

/// Visible viewport bounds (lines).
pub const VIEWPORT_V_START_LINE: u16 = 0x19;
pub const VIEWPORT_V_END_LINE: u16 = 0x139;

/// Display data fetch window — fixed at the boot's typical bounds for
/// now. Future refinement: respect DDFSTRT/STOP and DIWSTRT/STOP per
/// frame, including their CCK-quantised rules.
const DDF_START_CCK: u16 = 0x38;
const DDF_STOP_CCK: u16 = 0xD0;

/// Lores 8-CCK fetch block schedule:
///   slot 0: BPL4 (BPU >= 4)
///   slot 1: BPL6 (BPU >= 6) — HAM/EHB only
///   slot 2: BPL2 (BPU >= 2)
///   slot 3: BPL5 (BPU >= 5)
///   slot 4: BPL3 (BPU >= 3)
///   slot 5: (idle)
///   slot 6: BPL1 (BPU >= 1)
///   slot 7: (idle)
fn lores_fetch_plane(slot_in_block: u16, bpu: u8) -> Option<usize> {
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

    /// Tick one CCK. Performs at most one bitplane fetch (if scheduled)
    /// and outputs 2 lores pixels into the framebuffer (if in visible
    /// window). Called from `Amiga::tick_cck` after the beam advance.
    pub fn tick_cck(
        &mut self,
        vpos: u16,
        hpos: u16,
        chipset: &mut Chipset,
        memory: &Memory,
    ) {
        let in_visible_line =
            (VIEWPORT_V_START_LINE..VIEWPORT_V_END_LINE).contains(&vpos);
        let bpl_dma_on = chipset.dmacon & 0x0300 == 0x0300;
        let bpu = chipset.num_bitplanes();
        let in_ddf = (DDF_START_CCK..DDF_STOP_CCK).contains(&hpos);

        // ── 1. Bitplane fetch (one slot per CCK) ────────────────
        if in_visible_line && bpl_dma_on && in_ddf {
            let slot_in_block = (hpos - DDF_START_CCK) % 8;
            if let Some(plane) = lores_fetch_plane(slot_in_block, bpu) {
                let addr = chipset.bpl_pt[plane];
                let hi = memory.read_chip_ram_byte(addr);
                let lo = memory.read_chip_ram_byte(addr.wrapping_add(1));
                self.bpl_data[plane] = (u16::from(hi) << 8) | u16::from(lo);
                chipset.bpl_pt[plane] = chipset.bpl_pt[plane].wrapping_add(2);
                self.bytes_this_line += 2;
            }
        }

        // ── 2. Reload shift registers at slot 7 of each block ───
        // After all fetches in this 8-CCK block have completed, copy
        // each plane's latched fetch result into the shift register.
        if in_visible_line && in_ddf {
            let slot_in_block = (hpos - DDF_START_CCK) % 8;
            if slot_in_block == 7 {
                for p in 0..bpu as usize {
                    self.bpl_shift[p] = self.bpl_data[p];
                }
            }
        }

        // ── 3. Output 2 lores pixels ────────────────────────────
        if in_visible_line && in_ddf {
            let local_y = (vpos - VIEWPORT_V_START_LINE) as u32 * 2;
            let local_x_base = (hpos - DDF_START_CCK) as u32 * 2;
            for pixel_in_cck in 0..2u32 {
                let mut index = 0u8;
                for p in 0..bpu as usize {
                    if (self.bpl_shift[p] >> 15) & 1 != 0 {
                        index |= 1 << p;
                    }
                    self.bpl_shift[p] <<= 1;
                }
                let colour = chipset.color[index as usize] & 0x0FFF;
                let pixel = rgb12_to_argb(colour);
                let local_x = local_x_base + pixel_in_cck;
                // Pixel-double horizontally, line-double vertically.
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

        // ── 4. End-of-line modulo ───────────────────────────────
        // At the very last CCK before line wrap, apply modulo. The
        // beam wraps after `PAL_LINE_CCKS - 1`; we tick the modulo
        // when hpos == 0 (the line that just started). At hpos==0 we
        // know the previous line's fetches are done.
        if hpos == 0 && self.bytes_this_line > 0 && bpl_dma_on {
            for p in 0..bpu as usize {
                let modulo = if p & 1 == 0 {
                    chipset.bpl1mod as i32
                } else {
                    chipset.bpl2mod as i32
                };
                chipset.bpl_pt[p] = chipset.bpl_pt[p].wrapping_add(modulo as u32);
            }
            self.bytes_this_line = 0;
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
}
