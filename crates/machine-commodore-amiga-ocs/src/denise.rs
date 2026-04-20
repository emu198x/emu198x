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

/// Display dimensions for PAL Standard (line-doubled, lores → 4:3).
pub const FB_WIDTH: u32 = 768;
pub const FB_HEIGHT: u32 = 576;

/// Visible viewport bounds (CCKs / lines).
pub const VIEWPORT_H_START_CCK: u16 = 0x2C;
pub const VIEWPORT_H_END_CCK: u16 = 0xEC;
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
    /// the given beam position. Outputs nothing (no-op) when the beam
    /// is outside the visible viewport.
    pub fn tick_cck(&mut self, vpos: u16, hpos: u16, chipset: &Chipset) {
        if !(VIEWPORT_H_START_CCK..VIEWPORT_H_END_CCK).contains(&hpos) {
            return;
        }
        if !(VIEWPORT_V_START_LINE..VIEWPORT_V_END_LINE).contains(&vpos) {
            return;
        }

        // Lores: 2 pixels per CCK. The framebuffer is line-doubled
        // so each emulated line maps to two output rows.
        let local_x = (hpos - VIEWPORT_H_START_CCK) as u32 * 2;
        let local_y = (vpos - VIEWPORT_V_START_LINE) as u32 * 2;
        let pixel = rgb12_to_argb(chipset.color[0] & 0x0FFF);

        // Each CCK = 2 lores pixels = 4 displayed pixels (we double
        // horizontally too, since lores 384 px → 768 displayed via
        // pixel-doubling for square-pixel 4:3).
        let row_a = local_y;
        let row_b = local_y + 1;
        for dy in [row_a, row_b] {
            for dx in 0..4 {
                let x = local_x * 2 + dx;
                let idx = (dy * FB_WIDTH + x) as usize;
                if idx < self.framebuffer.len() {
                    self.framebuffer[idx] = pixel;
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
        assert_eq!(rgb12_to_argb(0x00F0), 0xFF00_FF00);
        assert_eq!(rgb12_to_argb(0x000F), 0xFF00_00FF);
        assert_eq!(rgb12_to_argb(0x0444), 0xFF44_4444);
    }

    #[test]
    fn outside_viewport_does_nothing() {
        let mut d = Denise::new();
        let mut c = Chipset::new();
        c.color[0] = 0x0FFF;
        // hpos before viewport
        d.tick_cck(100, 0, &c);
        // hpos after viewport
        d.tick_cck(100, 0xF0, &c);
        // vpos before viewport
        d.tick_cck(5, 0x80, &c);
        // All framebuffer pixels still default $FF00_0000
        assert!(d.framebuffer.iter().all(|&p| p == 0xFF00_0000));
    }

    #[test]
    fn visible_pixel_renders_color00() {
        let mut d = Denise::new();
        let mut c = Chipset::new();
        c.color[0] = 0x0F00;
        d.tick_cck(100, 0x80, &c);
        // One CCK at (100, 0x80) writes 4 horizontal × 2 vertical pixels.
        let local_x = (0x80 - VIEWPORT_H_START_CCK) as u32 * 2 * 2;
        let local_y = (100 - VIEWPORT_V_START_LINE) as u32 * 2;
        let idx = (local_y * FB_WIDTH + local_x) as usize;
        assert_eq!(d.framebuffer[idx], 0xFFFF_0000);
    }
}
