//! Denise - Video output.

pub const FB_WIDTH: u32 = 320;
pub const FB_HEIGHT: u32 = 256;

pub struct Denise {
    pub palette: [u16; 32],
    pub framebuffer: Vec<u32>,
    pub bpl_data: [u16; 6],
}

impl Denise {
    pub fn new() -> Self {
        Self {
            palette: [0; 32],
            framebuffer: vec![0xFF000000; (FB_WIDTH * FB_HEIGHT) as usize],
            bpl_data: [0; 6],
        }
    }

    pub fn load_bitplane(&mut self, idx: usize, val: u16) {
        if idx < 6 {
            self.bpl_data[idx] = val;
        }
    }

    pub fn set_palette(&mut self, idx: usize, val: u16) {
        if idx < 32 {
            self.palette[idx] = val & 0x0FFF;
        }
    }

    pub fn output_pixel(&mut self, x: u32, y: u32) {
        if x < FB_WIDTH && y < FB_HEIGHT {
            // Very basic: just output background color (palette[0])
            let rgb12 = self.palette[0];
            let r = ((rgb12 >> 8) & 0xF) as u8;
            let g = ((rgb12 >> 4) & 0xF) as u8;
            let b = (rgb12 & 0xF) as u8;
            
            // Expand 4-bit to 8-bit
            let r8 = (r << 4) | r;
            let g8 = (g << 4) | g;
            let b8 = (b << 4) | b;
            
            let argb32 = 0xFF000000 | (u32::from(r8) << 16) | (u32::from(g8) << 8) | u32::from(b8);
            self.framebuffer[(y * FB_WIDTH + x) as usize] = argb32;
        }
    }
}
