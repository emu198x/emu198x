//! Commodore Denise OCS — video output, bitplane shifter, and sprite engine.
//!
//! Denise receives bitplane data from Agnus DMA and shifts it out pixel by
//! pixel, combining with the colour palette to produce the final framebuffer.

pub const FB_WIDTH: u32 = 320;
pub const FB_HEIGHT: u32 = 256;

pub struct DeniseOcs {
    pub palette: [u16; 32],
    pub framebuffer: Vec<u32>,
    pub bpl_data: [u16; 6],   // Holding latches: written by DMA
    pub bpl_shift: [u16; 6],  // Shift registers: loaded from latches on BPL1DAT write
    pub shift_count: u8,      // Pixels remaining in shift register (0 -> output COLOR00)
    pub bplcon1: u16,
    pub bplcon2: u16,
    pub spr_pos: [u16; 8],
    pub spr_ctl: [u16; 8],
    pub spr_data: [u16; 8],
    pub spr_datb: [u16; 8],
}

impl DeniseOcs {
    pub fn new() -> Self {
        Self {
            palette: [0; 32],
            framebuffer: vec![0xFF000000; (FB_WIDTH * FB_HEIGHT) as usize],
            bpl_data: [0; 6],
            bpl_shift: [0; 6],
            shift_count: 0,
            bplcon1: 0,
            bplcon2: 0,
            spr_pos: [0; 8],
            spr_ctl: [0; 8],
            spr_data: [0; 8],
            spr_datb: [0; 8],
        }
    }

    pub fn set_palette(&mut self, idx: usize, val: u16) {
        if idx < 32 {
            self.palette[idx] = val & 0x0FFF;
        }
    }

    pub fn load_bitplane(&mut self, idx: usize, val: u16) {
        if idx < 6 {
            self.bpl_data[idx] = val;
        }
    }

    /// Copy all bitplane holding latches into the shift registers.
    /// On real hardware this happens when BPL1DAT (plane 0) is written,
    /// which is always the last plane fetched in each 8-CCK DMA group.
    pub fn trigger_shift_load(&mut self) {
        for i in 0..6 {
            self.bpl_shift[i] = self.bpl_data[i];
        }
        self.shift_count = 16;
    }

    fn rgb12_to_argb32(rgb12: u16) -> u32 {
        let r = ((rgb12 >> 8) & 0xF) as u8;
        let g = ((rgb12 >> 4) & 0xF) as u8;
        let b = (rgb12 & 0xF) as u8;
        let r8 = (r << 4) | r;
        let g8 = (g << 4) | g;
        let b8 = (b << 4) | b;
        0xFF000000 | (u32::from(r8) << 16) | (u32::from(g8) << 8) | u32::from(b8)
    }

    pub fn output_pixel(&mut self, x: u32, y: u32) {
        if x < FB_WIDTH && y < FB_HEIGHT {
            let argb32 = if self.shift_count > 0 {
                // Compute color index from shifter bits (MSB first)
                let mut idx = 0u8;
                for plane in 0..6 {
                    if (self.bpl_shift[plane] & 0x8000) != 0 {
                        idx |= 1 << plane;
                    }
                    self.bpl_shift[plane] <<= 1;
                }
                self.shift_count -= 1;
                Self::rgb12_to_argb32(self.palette[idx as usize])
            } else {
                // Outside data fetch window or shift register exhausted: COLOR00
                Self::rgb12_to_argb32(self.palette[0])
            };
            self.framebuffer[(y * FB_WIDTH + x) as usize] = argb32;
        }
    }
}

impl Default for DeniseOcs {
    fn default() -> Self {
        Self::new()
    }
}
