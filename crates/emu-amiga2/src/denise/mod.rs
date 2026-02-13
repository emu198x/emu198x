//! Denise: video output.
//!
//! Denise takes bitplane data from Agnus, shifts it out pixel by pixel,
//! and looks up colours in the palette to produce the framebuffer.

pub mod video;

use crate::config::DeniseVariant;

pub use video::{FB_WIDTH, FB_HEIGHT};

/// Denise video chip.
pub struct Denise {
    /// Chip variant.
    #[allow(dead_code)]
    variant: DeniseVariant,
    /// 32-colour palette (12-bit RGB values for OCS/ECS).
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
    pub fn new(variant: DeniseVariant) -> Self {
        Self {
            variant,
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

        for i in num_bpl..6 {
            self.shift_regs[i] <<= 1;
        }

        let colour = self.palette[idx as usize & 0x1F];
        let argb = video::rgb12_to_argb32(colour);
        let offset = (fb_y * FB_WIDTH + fb_x) as usize;
        self.framebuffer[offset] = argb;
    }

    /// Output a background (COLOR00) pixel.
    pub fn output_background(&mut self, fb_x: u32, fb_y: u32) {
        if fb_x >= FB_WIDTH || fb_y >= FB_HEIGHT {
            return;
        }
        let argb = video::rgb12_to_argb32(self.palette[0]);
        let offset = (fb_y * FB_WIDTH + fb_x) as usize;
        self.framebuffer[offset] = argb;
    }

    /// Reference to the framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &*self.framebuffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_bitplanes_from_bplcon0() {
        let mut denise = Denise::new(DeniseVariant::Denise8362);
        denise.bplcon0 = 0x5200;
        assert_eq!(denise.num_bitplanes(), 5);
    }

    #[test]
    fn framebuffer_correct_size() {
        let denise = Denise::new(DeniseVariant::Denise8362);
        assert_eq!(denise.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
    }

    #[test]
    fn output_pixel_reads_shift_registers() {
        let mut denise = Denise::new(DeniseVariant::Denise8362);
        denise.bplcon0 = 0x1000; // 1 bitplane
        denise.palette[0] = 0x000;
        denise.palette[1] = 0xFFF;
        denise.shift_regs[0] = 0x8000;

        denise.output_pixel(0, 0);
        assert_eq!(denise.framebuffer[0], 0xFFFF_FFFF);

        denise.output_pixel(1, 0);
        assert_eq!(denise.framebuffer[1], 0xFF00_0000);
    }
}
