//! Denise: video output.
//!
//! Denise takes bitplane data from Agnus, shifts it out pixel by pixel,
//! and looks up colours in the 32-entry palette to produce the framebuffer.
//!
//! Framebuffer: 360×284 ARGB32 (320 active + margins).
//! Standard PAL display window: DIWSTRT=$2C81, DIWSTOP=$2CC1.

#![allow(clippy::cast_possible_truncation, clippy::large_stack_arrays)]

/// Framebuffer width (320 active + border margins).
pub const FB_WIDTH: u32 = 360;
/// Framebuffer height (256 active + top/bottom border).
pub const FB_HEIGHT: u32 = 284;

/// Convert 12-bit RGB to ARGB32.
fn rgb12_to_argb32(rgb12: u16) -> u32 {
    let r = u32::from((rgb12 >> 8) & 0xF);
    let g = u32::from((rgb12 >> 4) & 0xF);
    let b = u32::from(rgb12 & 0xF);
    0xFF00_0000 | ((r << 4 | r) << 16) | ((g << 4 | g) << 8) | (b << 4 | b)
}

/// Denise video chip.
pub struct Denise {
    /// 32-colour palette (12-bit RGB values).
    pub palette: [u16; 32],
    /// BPLCON0: bitplane control.
    pub bplcon0: u16,
    /// BPLCON1: horizontal scroll.
    pub bplcon1: u16,
    /// BPLCON2: sprite/playfield priority.
    pub bplcon2: u16,
    /// Bitplane shift registers (6 planes).
    pub shift_regs: [u16; 6],
    /// Framebuffer (ARGB32).
    framebuffer: Box<[u32; (FB_WIDTH * FB_HEIGHT) as usize]>,
}

impl Denise {
    #[must_use]
    pub fn new() -> Self {
        Self {
            palette: [0; 32],
            bplcon0: 0,
            bplcon1: 0,
            bplcon2: 0,
            shift_regs: [0; 6],
            framebuffer: Box::new([0; (FB_WIDTH * FB_HEIGHT) as usize]),
        }
    }

    /// Number of active bitplanes from BPLCON0 (bits 14-12).
    #[must_use]
    pub fn num_bitplanes(&self) -> u8 {
        ((self.bplcon0 >> 12) & 0x07) as u8
    }

    /// Load a bitplane data word into a shift register.
    pub fn load_bitplane(&mut self, plane: usize, data: u16) {
        if plane < 6 {
            self.shift_regs[plane] = data;
        }
    }

    /// Output one lo-res pixel from the shift registers.
    ///
    /// Call once per CCK in the active display area.
    /// `fb_x` and `fb_y` are framebuffer coordinates.
    pub fn output_pixel(&mut self, fb_x: u32, fb_y: u32) {
        if fb_x >= FB_WIDTH || fb_y >= FB_HEIGHT {
            return;
        }

        let num_bpl = self.num_bitplanes().min(6) as usize;
        let mut idx: u8 = 0;

        for i in 0..num_bpl {
            idx |= (((self.shift_regs[i] >> 15) & 1) as u8) << i;
            self.shift_regs[i] <<= 1;
        }

        // Shift remaining planes even if not used for index
        for i in num_bpl..6 {
            self.shift_regs[i] <<= 1;
        }

        let colour = self.palette[idx as usize & 0x1F];
        let argb = rgb12_to_argb32(colour);
        let offset = (fb_y * FB_WIDTH + fb_x) as usize;
        self.framebuffer[offset] = argb;
    }

    /// Output a background (COLOR00) pixel.
    pub fn output_background(&mut self, fb_x: u32, fb_y: u32) {
        if fb_x >= FB_WIDTH || fb_y >= FB_HEIGHT {
            return;
        }
        let argb = rgb12_to_argb32(self.palette[0]);
        let offset = (fb_y * FB_WIDTH + fb_x) as usize;
        self.framebuffer[offset] = argb;
    }

    /// Reference to the framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &*self.framebuffer
    }
}

impl Default for Denise {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb12_black() {
        assert_eq!(rgb12_to_argb32(0x000), 0xFF00_0000);
    }

    #[test]
    fn rgb12_white() {
        assert_eq!(rgb12_to_argb32(0xFFF), 0xFFFF_FFFF);
    }

    #[test]
    fn rgb12_red() {
        assert_eq!(rgb12_to_argb32(0xF00), 0xFFFF_0000);
    }

    #[test]
    fn num_bitplanes_from_bplcon0() {
        let mut denise = Denise::new();
        denise.bplcon0 = 0x5200; // 5 bitplanes (bits 14-12 = 101)
        assert_eq!(denise.num_bitplanes(), 5);
    }

    #[test]
    fn framebuffer_correct_size() {
        let denise = Denise::new();
        assert_eq!(denise.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
    }

    #[test]
    fn output_pixel_reads_shift_registers() {
        let mut denise = Denise::new();
        denise.bplcon0 = 0x1000; // 1 bitplane
        denise.palette[0] = 0x000; // Black
        denise.palette[1] = 0xFFF; // White
        denise.shift_regs[0] = 0x8000; // MSB set → palette index 1

        denise.output_pixel(0, 0);
        assert_eq!(denise.framebuffer[0], 0xFFFF_FFFF); // White

        // Shift register shifted left, MSB now 0
        denise.output_pixel(1, 0);
        assert_eq!(denise.framebuffer[1], 0xFF00_0000); // Black
    }
}
