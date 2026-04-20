//! Denise — display chip (M11 minimum).
//!
//! At M11 Denise just outputs the background color (COLOR00) for each
//! pixel in the visible PAL Standard viewport. No bitplane decoding
//! yet — bits move into Denise's shift registers in M11.1.
//!
//! Output format: 768 × 576 ARGB8888.
//!
//! Visible viewport (matches archived investigation's PAL Standard):
//!   - Horizontal: CCKs $2C..$EC (192 CCKs) → 384 lores px → 768 hires/displayed
//!   - Vertical: lines $19..$139 (288 lines) → 576 line-doubled rows
//!
//! Each CCK in the visible region produces 2 lores pixels (the
//! current implementation always renders lores; hires comes when
//! the boot demands it).

use crate::chipset::Chipset;
use crate::memory::Memory;

/// Display dimensions for PAL Standard (line-doubled, lores → 4:3).
pub const FB_WIDTH: u32 = 768;
pub const FB_HEIGHT: u32 = 576;

/// Visible viewport bounds (lines). Horizontal bounds weren't used
/// by the scanline renderer; DDFSTRT/STOP gating lives in M11.2.
pub const VIEWPORT_V_START_LINE: u16 = 0x19;
pub const VIEWPORT_V_END_LINE: u16 = 0x139;

pub struct Denise {
    /// ARGB8888 framebuffer (FB_WIDTH × FB_HEIGHT pixels).
    pub framebuffer: Vec<u32>,
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

    /// Render a single CCK's worth of pixels into the framebuffer at
    /// the given beam position. When `hpos == 0` and we're in the
    /// visible vertical window, compose the entire visible line from
    /// bitplane data + palette (scanline renderer; per-CCK bitplane
    /// fetch scheduling is left for a future refinement).
    pub fn tick_cck(
        &mut self,
        vpos: u16,
        hpos: u16,
        chipset: &mut Chipset,
        memory: &Memory,
    ) {
        if hpos != 0 {
            return;
        }
        if !(VIEWPORT_V_START_LINE..VIEWPORT_V_END_LINE).contains(&vpos) {
            return;
        }

        let local_y = (vpos - VIEWPORT_V_START_LINE) as u32 * 2;
        let bpl_dma_on =
            chipset.dmacon & 0x0100 != 0 && chipset.dmacon & 0x0200 != 0;
        let num_planes = chipset.num_bitplanes();

        // Display window is 384 lores pixels = 24 16-bit words per
        // bitplane per line. We simplify by always rendering that
        // full 384-pixel span (DIWSTRT/STOP and DDFSTRT/STOP are
        // stored but not yet gating the span).
        const PIXELS_PER_LINE: u32 = 384;
        const WORDS_PER_LINE: u32 = 24;

        for word_idx in 0..WORDS_PER_LINE {
            // Fetch one 16-bit word from each active bitplane, or
            // zero if bitplane DMA is off / no planes active.
            let mut words = [0u16; 6];
            for p in 0..num_planes as usize {
                if bpl_dma_on && num_planes > 0 {
                    let addr = chipset.bpl_pt[p].wrapping_add(word_idx * 2);
                    let hi = memory.read_chip_ram_byte(addr);
                    let lo = memory.read_chip_ram_byte(addr.wrapping_add(1));
                    words[p] = (u16::from(hi) << 8) | u16::from(lo);
                }
            }

            for bit in 0..16u32 {
                let mut index = 0u8;
                let shift = 15 - bit;
                for p in 0..num_planes as usize {
                    if (words[p] >> shift) & 1 != 0 {
                        index |= 1 << p;
                    }
                }
                let colour = chipset.color[index as usize] & 0x0FFF;
                let pixel = rgb12_to_argb(colour);
                let local_x = (word_idx * 16 + bit) * 2; // pixel-double
                if local_x + 1 >= PIXELS_PER_LINE * 2 {
                    continue;
                }
                for dy in [local_y, local_y + 1] {
                    for dx in 0..2u32 {
                        let x = local_x + dx;
                        let idx = (dy * FB_WIDTH + x) as usize;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = pixel;
                        }
                    }
                }
            }
        }

        // Advance bitplane pointers by line width + modulo.
        if bpl_dma_on && num_planes > 0 {
            let line_bytes = WORDS_PER_LINE * 2;
            for p in 0..num_planes as usize {
                let modulo = if p & 1 == 0 {
                    chipset.bpl1mod as i32
                } else {
                    chipset.bpl2mod as i32
                };
                chipset.bpl_pt[p] = chipset.bpl_pt[p]
                    .wrapping_add(line_bytes)
                    .wrapping_add(modulo as u32);
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
        assert_eq!(rgb12_to_argb(0x00F0), 0xFF00_FF00);
        assert_eq!(rgb12_to_argb(0x000F), 0xFF00_00FF);
        assert_eq!(rgb12_to_argb(0x0444), 0xFF44_4444);
    }

    #[test]
    fn outside_viewport_does_nothing() {
        let mut d = Denise::new();
        let mut c = Chipset::new();
        let mut mem = Memory::new(vec![0u8; 256 * 1024]);
        mem.set_overlay(false);
        c.color[0] = 0x0FFF;
        // hpos != 0 → no-op for the scanline renderer
        d.tick_cck(100, 50, &mut c, &mem);
        // vpos before viewport
        d.tick_cck(5, 0, &mut c, &mem);
        assert!(d.framebuffer.iter().all(|&p| p == 0xFF00_0000));
    }

    #[test]
    fn visible_line_fills_with_color00_when_no_bitplanes() {
        let mut d = Denise::new();
        let mut c = Chipset::new();
        let mut mem = Memory::new(vec![0u8; 256 * 1024]);
        mem.set_overlay(false);
        c.color[0] = 0x0F00;
        // BPU=0 → all pixels = color 0.
        c.bplcon0 = 0x0200;
        d.tick_cck(100, 0, &mut c, &mem);
        let local_y = (100 - VIEWPORT_V_START_LINE) as u32 * 2;
        let red = 0xFFFF_0000;
        for x in 0..16u32 {
            assert_eq!(
                d.framebuffer[(local_y * FB_WIDTH + x) as usize],
                red,
                "pixel {x} on filled line should be red (color 0)"
            );
        }
    }
}
