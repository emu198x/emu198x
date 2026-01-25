//! NES Picture Processing Unit (PPU 2C02).
//!
//! The PPU handles all video output:
//! - 256x240 pixel output
//! - 2 pattern tables (CHR ROM/RAM)
//! - 4 nametables (2KB internal, mirrored)
//! - 64 sprites (8 per scanline)
//! - 64-color palette (52 distinct colors)

use crate::memory::NesMemory;

/// PPU control register flags ($2000).
pub mod ctrl {
    pub const NAMETABLE_X: u8 = 0x01;
    pub const NAMETABLE_Y: u8 = 0x02;
    pub const VRAM_INCREMENT: u8 = 0x04; // 0=+1, 1=+32
    pub const SPRITE_PATTERN: u8 = 0x08;
    pub const BG_PATTERN: u8 = 0x10;
    pub const SPRITE_SIZE: u8 = 0x20; // 0=8x8, 1=8x16
    pub const MASTER_SLAVE: u8 = 0x40;
    pub const NMI_ENABLE: u8 = 0x80;
}

/// PPU mask register flags ($2001).
pub mod mask {
    pub const GREYSCALE: u8 = 0x01;
    pub const BG_LEFT: u8 = 0x02;
    pub const SPRITE_LEFT: u8 = 0x04;
    pub const BG_ENABLE: u8 = 0x08;
    pub const SPRITE_ENABLE: u8 = 0x10;
    pub const EMPHASIZE_RED: u8 = 0x20;
    pub const EMPHASIZE_GREEN: u8 = 0x40;
    pub const EMPHASIZE_BLUE: u8 = 0x80;
}

/// PPU status register flags ($2002).
pub mod status {
    pub const SPRITE_OVERFLOW: u8 = 0x20;
    pub const SPRITE_0_HIT: u8 = 0x40;
    pub const VBLANK: u8 = 0x80;
}

/// NES PPU.
pub struct Ppu {
    /// Control register ($2000).
    pub ctrl: u8,
    /// Mask register ($2001).
    pub mask: u8,
    /// Status register ($2002).
    pub status: u8,
    /// OAM address ($2003).
    pub oam_addr: u8,
    /// OAM data (256 bytes, 64 sprites x 4 bytes).
    pub oam: [u8; 256],
    /// Current VRAM address (loopy_v).
    vram_addr: u16,
    /// Temporary VRAM address (loopy_t).
    temp_addr: u16,
    /// Fine X scroll (3 bits).
    fine_x: u8,
    /// Write toggle (for $2005/$2006).
    write_toggle: bool,
    /// Data buffer for $2007 reads.
    data_buffer: u8,
    /// Current scanline (0-261).
    scanline: u16,
    /// Current cycle within scanline (0-340).
    cycle: u16,
    /// Frame is odd (for NTSC skip).
    odd_frame: bool,
    /// NMI occurred this frame.
    nmi_occurred: bool,
}

impl Ppu {
    /// Create a new PPU.
    pub fn new() -> Self {
        Self {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            oam: [0; 256],
            vram_addr: 0,
            temp_addr: 0,
            fine_x: 0,
            write_toggle: false,
            data_buffer: 0,
            scanline: 0,
            cycle: 0,
            odd_frame: false,
            nmi_occurred: false,
        }
    }

    /// Reset the PPU.
    pub fn reset(&mut self) {
        self.ctrl = 0;
        self.mask = 0;
        self.status = 0;
        self.write_toggle = false;
        self.scanline = 0;
        self.cycle = 0;
        self.odd_frame = false;
        self.nmi_occurred = false;
    }

    /// Read PPU register (memory-mapped $2000-$2007).
    pub fn read_register(&mut self, addr: u16, memory: &NesMemory) -> u8 {
        match addr & 0x07 {
            // $2002 - Status
            2 => {
                let status = self.status;
                self.status &= !status::VBLANK;
                self.write_toggle = false;
                self.nmi_occurred = false;
                status
            }
            // $2004 - OAM data
            4 => self.oam[self.oam_addr as usize],
            // $2007 - VRAM data
            7 => {
                let addr = self.vram_addr & 0x3FFF;
                let data = if addr >= 0x3F00 {
                    // Palette reads are immediate
                    memory.ppu_read(addr)
                } else {
                    // Other reads are buffered
                    let buffered = self.data_buffer;
                    self.data_buffer = memory.ppu_read(addr);
                    buffered
                };
                self.increment_vram_addr();
                data
            }
            _ => 0,
        }
    }

    /// Write PPU register (memory-mapped $2000-$2007).
    pub fn write_register(&mut self, addr: u16, value: u8, memory: &mut NesMemory) {
        match addr & 0x07 {
            // $2000 - Control
            0 => {
                self.ctrl = value;
                // t: ...GH.. ........ <- d: ......GH
                self.temp_addr = (self.temp_addr & 0xF3FF) | ((value as u16 & 0x03) << 10);
            }
            // $2001 - Mask
            1 => self.mask = value,
            // $2003 - OAM address
            3 => self.oam_addr = value,
            // $2004 - OAM data
            4 => {
                self.oam[self.oam_addr as usize] = value;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            // $2005 - Scroll
            5 => {
                if !self.write_toggle {
                    // First write: X scroll
                    self.temp_addr = (self.temp_addr & 0xFFE0) | ((value as u16) >> 3);
                    self.fine_x = value & 0x07;
                } else {
                    // Second write: Y scroll
                    self.temp_addr = (self.temp_addr & 0x8C1F)
                        | ((value as u16 & 0x07) << 12)
                        | ((value as u16 & 0xF8) << 2);
                }
                self.write_toggle = !self.write_toggle;
            }
            // $2006 - VRAM address
            6 => {
                if !self.write_toggle {
                    // First write: high byte
                    self.temp_addr = (self.temp_addr & 0x00FF) | ((value as u16 & 0x3F) << 8);
                } else {
                    // Second write: low byte
                    self.temp_addr = (self.temp_addr & 0xFF00) | (value as u16);
                    self.vram_addr = self.temp_addr;
                }
                self.write_toggle = !self.write_toggle;
            }
            // $2007 - VRAM data
            7 => {
                memory.ppu_write(self.vram_addr & 0x3FFF, value);
                self.increment_vram_addr();
            }
            _ => {}
        }
    }

    /// Increment VRAM address by 1 or 32 based on CTRL register.
    fn increment_vram_addr(&mut self) {
        let increment = if self.ctrl & ctrl::VRAM_INCREMENT != 0 {
            32
        } else {
            1
        };
        self.vram_addr = self.vram_addr.wrapping_add(increment);
    }

    /// Tick PPU for one cycle.
    /// Returns (nmi_triggered, pixel_output).
    pub fn tick(&mut self, _memory: &mut NesMemory) -> (bool, Option<u8>) {
        let mut nmi = false;
        let pixel = None; // TODO: implement rendering

        // Pre-render scanline
        if self.scanline == 261 {
            if self.cycle == 1 {
                // Clear vblank, sprite 0 hit, overflow
                self.status &= !(status::VBLANK | status::SPRITE_0_HIT | status::SPRITE_OVERFLOW);
                self.nmi_occurred = false;
            }
        }

        // Visible scanlines (0-239) - rendering happens here
        // TODO: implement actual rendering

        // Vblank start (scanline 241)
        if self.scanline == 241 && self.cycle == 1 {
            self.status |= status::VBLANK;
            if self.ctrl & ctrl::NMI_ENABLE != 0 && !self.nmi_occurred {
                nmi = true;
                self.nmi_occurred = true;
            }
        }

        // Advance cycle/scanline
        self.cycle += 1;
        if self.cycle > 340 {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > 261 {
                self.scanline = 0;
                self.odd_frame = !self.odd_frame;
            }
        }

        (nmi, pixel)
    }

    /// Check if rendering is enabled.
    pub fn rendering_enabled(&self) -> bool {
        self.mask & (mask::BG_ENABLE | mask::SPRITE_ENABLE) != 0
    }

    /// Get current scanline.
    pub fn scanline(&self) -> u16 {
        self.scanline
    }

    /// Get current cycle.
    pub fn cycle(&self) -> u16 {
        self.cycle
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ppu() {
        let ppu = Ppu::new();
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.cycle(), 0);
    }

    #[test]
    fn test_vram_increment() {
        let mut ppu = Ppu::new();

        // Increment by 1 (default)
        ppu.vram_addr = 0x2000;
        ppu.ctrl = 0;
        ppu.increment_vram_addr();
        assert_eq!(ppu.vram_addr, 0x2001);

        // Increment by 32
        ppu.vram_addr = 0x2000;
        ppu.ctrl = ctrl::VRAM_INCREMENT;
        ppu.increment_vram_addr();
        assert_eq!(ppu.vram_addr, 0x2020);
    }
}
